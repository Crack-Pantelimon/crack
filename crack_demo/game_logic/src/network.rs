//! Game-specific network room definitions. The parametric/abstract chat
//! machinery lives in `net_crackpipe`; every concrete message type for this
//! game is defined here.

use net_crackpipe::IChatRoomType;
use net_crackpipe::network_manager::NetworkManagerConfig;
use serde::{Deserialize, Serialize};

/// Gossip topic id of the realtime gameplay sync room.
pub const GLOBAL_GAMEPLAY_TOPIC_ID: &str = "global_gameplay";

/// Network init config for this game: every bootstrap node also subscribes to
/// the gameplay topic (dropping all traffic), so that clients joining the
/// gameplay room always find a live, already-subscribed peer to bootstrap the
/// topic's gossip swarm from.
pub fn network_manager_config() -> NetworkManagerConfig {
    NetworkManagerConfig {
        bootstrap_topics: bootstrap_topics(),
    }
}

/// Topics every bootstrap node (matchmaker-owned or standalone worker-spawned)
/// must subscribe to, besides the global chat.
pub fn bootstrap_topics() -> Vec<String> {
    vec![GLOBAL_GAMEPLAY_TOPIC_ID.to_string()]
}

/// Room type for the realtime gameplay sync topic.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct GameplaySyncRoomType;

impl IChatRoomType for GameplaySyncRoomType {
    type M = GameplayChatMessageContent;
    type P = GameplayPresence;
    fn default_presence() -> Self::P {
        GameplayPresence::default()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GameplayChatMessageContent {
    GameSync { id: i64, payload: Vec<u8> },
}

#[derive(Clone, Debug, PartialEq, PartialOrd, Serialize, Deserialize, Default)]
pub struct GameplayPresence {
    pub url: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test as test;

    #[test]
    fn smoke_bootstrap_topics() {
        assert_eq!(
            bootstrap_topics(),
            vec![GLOBAL_GAMEPLAY_TOPIC_ID.to_string()]
        );
    }
}
