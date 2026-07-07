use crack::api_asscrack::anyhow;
use crack::api_asscrack::api::api_worker_declarations::WorkerApiGroup2;
use crack::api_asscrack::crack_worker::api_worker::{ApiImplMapping, make_api_mapping};
use crack::api_asscrack::crack_worker::{WorkerLoaderFactory, WorkerPipe};
use crack::native_thread_worker::ThreadWorkerFactory;
use crack::storage_crackhouse::api::StorageCrackhouseApiGroup;
use game_logic::api::GameLogicApiGroup;
use std::sync::Arc;

use iroh::{Endpoint, SecretKey, RelayMap, RelayNode};
use crack::net_crackpipe::{
    _bootstrap_keys::BOOTSTRAP_SECRET_KEYS,
    chat::chat_const::get_relay_domain,
    echo::Echo,
    main_node::MainNode,
    user_identity::{NodeIdentity, UserIdentitySecrets},
};

pub fn make_registered_mapping() -> Arc<ApiImplMapping> {
    make_api_mapping(vec![
        Arc::new(WorkerApiGroup2),
        Arc::new(StorageCrackhouseApiGroup),
        Arc::new(GameLogicApiGroup),
    ])
}

pub async fn spawn_in_process_worker() -> anyhow::Result<WorkerPipe> {
    tokio::task::spawn(async {
        if let Err(e) = run_bootstrap_if_needed().await {
            tracing::error!("Failed to run bootstrap check: {:?}", e);
        }
    });

    ThreadWorkerFactory {
        impl_mapping: make_registered_mapping(),
    }
    .load_worker()
    .await
}

pub async fn run_bootstrap_if_needed() -> anyhow::Result<()> {
    let (relay_url, pkarr_url) = get_relay_domain();
    let relay_map = RelayMap::from_nodes([RelayNode {
        url: relay_url.parse().unwrap(),
        stun_only: false,
        stun_port: 31232,
        quic: None,
    }]).unwrap();
    
    // Create a temporary resolver
    let pkarr_resolver = iroh::discovery::pkarr::PkarrResolver::new(pkarr_url.parse().unwrap());
    
    // Bind a temporary endpoint to check connectivity
    let temp_key = SecretKey::generate(rand::thread_rng());
    let endpoint = Endpoint::builder()
        .secret_key(temp_key)
        .relay_mode(iroh::RelayMode::Custom(relay_map))
        .add_discovery(move |_| Some(pkarr_resolver.clone()))
        .bind()
        .await?;
        
    let mut any_alive = false;
    for bs_known_secret in BOOTSTRAP_SECRET_KEYS.iter() {
        let bs_node_id = SecretKey::from_bytes(bs_known_secret).public();
        tracing::info!("Checking bootstrap node: {:?}", bs_node_id);
        
        let conn_res = n0_future::time::timeout(
            std::time::Duration::from_millis(1500),
            endpoint.connect(bs_node_id, Echo::ALPN),
        ).await;
        
        if let Ok(Ok(_conn)) = conn_res {
            any_alive = true;
            tracing::info!("Found live bootstrap node: {:?}", bs_node_id);
            break;
        }
    }
    
    if !any_alive {
        tracing::info!("No live bootstrap nodes found. Spawning local bootstrap node index 0...");
        
        let bootstrap_idx = 0;
        let bootstrap_key = SecretKey::from_bytes(&BOOTSTRAP_SECRET_KEYS[bootstrap_idx]);
        
        let user_secrets = Arc::new(UserIdentitySecrets::generate());
        let node_identity = Arc::new(NodeIdentity::new(
            *user_secrets.user_identity(),
            bootstrap_key.public(),
            Some(bootstrap_idx as u32),
        ));
        let sleep_manager = crack::net_crackpipe::sleep::SleepManager::new();
        
        let bootstrap_node = MainNode::spawn(
            node_identity,
            Arc::new(bootstrap_key),
            None,
            user_secrets,
            sleep_manager,
        ).await?;
        
        tracing::info!("Bootstrap node successfully spawned! Node ID: {:?}", bootstrap_node.node_id());
        
        // Keep the bootstrap node alive in the background
        loop {
            n0_future::time::sleep(std::time::Duration::from_secs(3600)).await;
        }
    } else {
        tracing::info!("At least one bootstrap node is alive. No action needed.");
    }
    
    Ok(())
}

