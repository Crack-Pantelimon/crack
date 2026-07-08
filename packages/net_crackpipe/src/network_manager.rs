//! Global network init: one entry point that owns all running network code.
//!
//! - Apps (game, chat_cli) call [`NetworkManager::init`] and get a single
//!   object holding the matchmaker (own node + bootstrap endpoint + global
//!   chats + periodic maintenance) and any rooms joined via
//!   [`NetworkManager::join_room`].
//! - Headless workers call [`run_standalone_bootstrap_if_needed`] to spawn a
//!   bootstrap node when none is reachable.
//!
//! This module is parametric over room types: game-specific message types
//! live in the game's own crates. Bootstrap nodes subscribe to the extra
//! topics listed in [`NetworkManagerConfig::bootstrap_topics`] at the *raw*
//! gossip layer only — they never decode the traffic, they just have to be
//! subscribed, because iroh-gossip can only bootstrap a topic swarm through
//! peers that are themselves subscribed to that topic.

use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result};
use iroh::{Endpoint, NodeId, RelayMap, RelayNode, SecretKey};
use n0_future::task::AbortOnDropHandle;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::{
    _bootstrap_keys::BOOTSTRAP_SECRET_KEYS,
    chat::{
        chat_const::{get_relay_domain, GLOBAL_CHAT_TOPIC_ID},
        chat_controller::{ChatController, IChatController, IChatRoomRaw, IChatSender},
        chat_ticket::ChatTicket,
        global_chat::{GlobalChatPresence, GlobalChatRoomType},
        room_raw::GossipChatRoom,
    },
    echo::Echo,
    global_matchmaker::GlobalMatchmaker,
    main_node::MainNode,
    signed_message::IChatRoomType,
    sleep::SleepManager,
    user_identity::{NodeIdentity, UserIdentitySecrets},
};

/// Configuration for the global network init.
#[derive(Debug, Clone, Default)]
pub struct NetworkManagerConfig {
    /// Extra gossip topics (besides the global chat) that every bootstrap
    /// node subscribes to, raw. All traffic on them is dropped (a drop
    /// counter is logged every 10 minutes). Bootstrap nodes subscribe only so
    /// that clients always find a live, already-subscribed peer to bootstrap
    /// the topic's gossip swarm from — otherwise two clients joining a topic
    /// at the same time would each sit alone in their own swarm.
    pub bootstrap_topics: Vec<String>,
}

/// Single "network manager" object containing all running network code:
/// the matchmaker (own node, optional bootstrap endpoint, global chats,
/// periodic maintenance) plus the per-room peer-join tasks spawned by
/// [`NetworkManager::join_room`].
#[derive(Debug, Clone)]
pub struct NetworkManager {
    mm: GlobalMatchmaker,
    tasks: Arc<RwLock<Vec<AbortOnDropHandle<()>>>>,
}

impl NetworkManager {
    /// Global network init. Creates the matchmaker: spawns the own node,
    /// takes over a free bootstrap slot if needed (the bootstrap node then
    /// joins the global chat *and* every topic in
    /// [`NetworkManagerConfig::bootstrap_topics`]), joins the global chat,
    /// and starts the periodic maintenance task.
    pub async fn init(
        secrets: Arc<UserIdentitySecrets>,
        config: NetworkManagerConfig,
    ) -> Result<Self> {
        let mm = GlobalMatchmaker::new_with_config(secrets, config).await?;
        Ok(Self {
            mm,
            tasks: Arc::new(RwLock::new(Vec::new())),
        })
    }

    pub fn matchmaker(&self) -> GlobalMatchmaker {
        self.mm.clone()
    }

    pub async fn global_chat_controller(&self) -> Option<ChatController<GlobalChatRoomType>> {
        self.mm.global_chat_controller().await
    }

    /// Join a gossip room on the own node. The ticket lists every known
    /// bootstrap node id *and* the node ids owning them, so two clients
    /// joining the same topic at the same time still meet through the
    /// bootstrap swarm.
    ///
    /// Also spawns a presence-driven `join_peers` refresher: whenever the
    /// room's presence list changes, a direct gossip connection to every
    /// present peer (and every known bootstrap node) is forced/refreshed.
    /// Without this, high-rate broadcasts get relayed through the bootstrap
    /// mesh, which drops peers under load.
    pub async fn join_room<T: IChatRoomType>(&self, topic_id: &str) -> Result<ChatController<T>> {
        let node = self.mm.own_node().await.context("join_room: no node")?;
        let ticket = ChatTicket::new_str_bs(topic_id, self.mm.bootstrap_nodes_set().await);
        let controller = node.join_chat::<T>(&ticket).await?;

        let presence = controller.chat_presence();
        let sender = controller.sender();
        let own_node_id = *self.mm.own_node_identity().node_id();
        let mm = self.mm.clone();
        let topic = topic_id.to_string();
        let task = AbortOnDropHandle::new(n0_future::task::spawn(async move {
            loop {
                presence.notified().await;
                let list = presence.get_presence_list().await;
                let mut peers = mm.bootstrap_nodes_set().await;
                peers.extend(list.0.iter().map(|p| *p.identity.node_id()));
                peers.remove(&own_node_id);
                let peers: Vec<NodeId> = peers.into_iter().collect();
                if !peers.is_empty() {
                    if let Err(e) = sender.join_peers(peers).await {
                        warn!("room '{topic}': presence join_peers failed: {e:?}");
                    }
                }
            }
        }));
        self.tasks.write().await.push(task);

        Ok(controller)
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.tasks.write().await.clear();
        self.mm.shutdown().await
    }
}

/// Drain a raw gossip room forever, dropping every message. Keeps a drop
/// counter and logs it every 10 minutes.
pub(crate) fn spawn_topic_drain_task(
    room: Arc<dyn IChatRoomRaw>,
    label: String,
) -> AbortOnDropHandle<()> {
    AbortOnDropHandle::new(n0_future::task::spawn(async move {
        const LOG_INTERVAL: Duration = Duration::from_secs(600);
        let mut total_msgs: u64 = 0;
        let mut total_bytes: u64 = 0;
        let mut window_msgs: u64 = 0;
        let mut window_bytes: u64 = 0;
        let mut last_log = n0_future::time::Instant::now();
        loop {
            // Timeout so the 10-minute stat line is printed even if the room
            // goes idle.
            match n0_future::time::timeout(Duration::from_secs(60), room.next_message()).await {
                Ok(Ok(Some(msg))) => {
                    window_msgs += 1;
                    window_bytes += msg.len() as u64;
                }
                Ok(Ok(None)) | Ok(Err(_)) => {
                    warn!("{label}: drain task exiting: room closed");
                    break;
                }
                Err(_timeout) => {}
            }
            if last_log.elapsed() >= LOG_INTERVAL {
                total_msgs += window_msgs;
                total_bytes += window_bytes;
                info!(
                    "{label}: dropped {window_msgs} msgs ({window_bytes} B) in the last 10 min \
                     (total: {total_msgs} msgs, {total_bytes} B)"
                );
                window_msgs = 0;
                window_bytes = 0;
                last_log = n0_future::time::Instant::now();
            }
        }
    }))
}

/// Probe the well-known bootstrap node ids over the Echo ALPN.
async fn any_bootstrap_alive() -> Result<bool> {
    let (relay_url, pkarr_url) = get_relay_domain();
    let relay_map = RelayMap::from_nodes([RelayNode {
        url: relay_url.parse().unwrap(),
        stun_only: false,
        stun_port: 31232,
        quic: None,
    }])
    .unwrap();
    let pkarr_resolver = iroh::discovery::pkarr::PkarrResolver::new(pkarr_url.parse().unwrap());

    let temp_key = SecretKey::generate(rand::thread_rng());
    let endpoint = Endpoint::builder()
        .secret_key(temp_key)
        .relay_mode(iroh::RelayMode::Custom(relay_map))
        .add_discovery(move |_| Some(pkarr_resolver.clone()))
        .bind()
        .await?;

    for bs_known_secret in BOOTSTRAP_SECRET_KEYS.iter() {
        let bs_node_id = SecretKey::from_bytes(bs_known_secret).public();
        info!("Checking bootstrap node: {:?}", bs_node_id);

        let conn_res = n0_future::time::timeout(
            Duration::from_millis(1500),
            endpoint.connect(bs_node_id, Echo::ALPN),
        )
        .await;

        if let Ok(Ok(_conn)) = conn_res {
            info!("Found live bootstrap node: {:?}", bs_node_id);
            return Ok(true);
        }
    }
    Ok(false)
}

/// If no bootstrap node is reachable, spawn one locally on the well-known
/// key #0 and connect it to the correct networks: the global chat (typed,
/// with a "Bootstrap" presence, answering bootstrap queries) and every topic
/// in `extra_topics` (raw, all traffic dropped and counted). Never returns
/// in that case. Called from the headless workers.
pub async fn run_standalone_bootstrap_if_needed(extra_topics: Vec<String>) -> Result<()> {
    if any_bootstrap_alive().await? {
        info!("At least one bootstrap node is alive. No action needed.");
        return Ok(());
    }
    info!("No live bootstrap nodes found. Spawning local bootstrap node index 0...");

    let bootstrap_idx = 0;
    let bootstrap_key = SecretKey::from_bytes(&BOOTSTRAP_SECRET_KEYS[bootstrap_idx]);

    let user_secrets = Arc::new(UserIdentitySecrets::generate());
    let node_identity = Arc::new(NodeIdentity::new(
        *user_secrets.user_identity(),
        bootstrap_key.public(),
        Some(bootstrap_idx as u32),
    ));

    let bootstrap_node = MainNode::spawn(
        node_identity,
        Arc::new(bootstrap_key),
        None,
        user_secrets,
        SleepManager::new(),
    )
    .await?;

    info!(
        "Bootstrap node successfully spawned! Node ID: {:?}",
        bootstrap_node.node_id()
    );

    // All other well-known bootstrap ids, as the ticket bootstrap set.
    let bs_set = BOOTSTRAP_SECRET_KEYS
        .iter()
        .map(|k| SecretKey::from_bytes(k).public())
        .collect::<std::collections::BTreeSet<_>>();

    // Global chat: typed join, so the node shows up as "Bootstrap" presence
    // and answers server-list bootstrap queries like matchmaker-owned
    // bootstrap nodes do.
    let global_ticket = ChatTicket::new_str_bs(GLOBAL_CHAT_TOPIC_ID, bs_set.clone());
    let _global_chat: Option<(ChatController<GlobalChatRoomType>, AbortOnDropHandle<()>)> =
        match bootstrap_node
            .join_chat::<GlobalChatRoomType>(&global_ticket)
            .await
        {
            Ok(cc) => {
                cc.sender()
                    .set_presence(&GlobalChatPresence {
                        url: "".to_string(),
                        platform: "Bootstrap".to_string(),
                        is_server: None,
                    })
                    .await;
                let cc2 = cc.clone();
                let task = AbortOnDropHandle::new(n0_future::task::spawn(async move {
                    if let Err(e) = crate::global_matchmaker::run_bs_global_chat_task(cc2).await {
                        warn!("standalone bootstrap global chat task exited: {e:?}");
                    }
                }));
                Some((cc, task))
            }
            Err(e) => {
                warn!("standalone bootstrap failed to join global chat: {e:?}");
                None
            }
        };

    // Extra topics (e.g. the gameplay room): raw subscribe + drop all traffic.
    let mut _drain_tasks = Vec::new();
    for topic in &extra_topics {
        let ticket = ChatTicket::new_str_bs(topic, bs_set.clone());
        match GossipChatRoom::new(&bootstrap_node, &ticket).await {
            Ok(room) => {
                info!("standalone bootstrap joined extra gossip topic '{topic}'");
                _drain_tasks.push(spawn_topic_drain_task(
                    Arc::new(room),
                    format!("standalone bootstrap, topic '{topic}'"),
                ));
            }
            Err(e) => {
                warn!("standalone bootstrap failed to join extra topic '{topic}': {e:?}");
            }
        }
    }

    // Keep the bootstrap node (and its room tasks) alive in the background.
    loop {
        n0_future::time::sleep(Duration::from_secs(3600)).await;
    }
}
