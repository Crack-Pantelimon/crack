use std::collections::BTreeSet;

pub use iroh::NodeId;
pub use iroh_gossip::proto::TopicId;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChatTicket {
    pub topic_id: TopicId,
    pub bootstrap: BTreeSet<NodeId>,
}

impl ChatTicket {
    pub fn new_str_bs(topic_id: &str, bs: BTreeSet<NodeId>) -> Self {
        let mut topic_bytes = [0; 32];
        let topic_str = topic_id.as_bytes().to_vec();
        // assert!(topic_str.len() <= 30);
        let len = 30.min(topic_str.len());
        topic_bytes[..len].copy_from_slice(&topic_str[..len]);
        Self {
            topic_id: TopicId::from_bytes(topic_bytes),
            bootstrap: bs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test as test;

    #[test]
    fn smoke_new_str_bs() {
        let peer = iroh::SecretKey::generate(rand::thread_rng()).public();
        let bs = BTreeSet::from([peer]);
        let ticket = ChatTicket::new_str_bs("hello", bs.clone());
        let mut expected = [0u8; 32];
        expected[..5].copy_from_slice(b"hello");
        assert_eq!(ticket.topic_id, TopicId::from_bytes(expected));
        assert_eq!(ticket.bootstrap, bs);
    }

    #[test]
    fn smoke_new_str_bs_truncates_long_topic() {
        // Topic strings longer than 30 bytes are truncated, not a panic.
        let ticket = ChatTicket::new_str_bs(
            "this-topic-name-is-far-longer-than-thirty-bytes",
            BTreeSet::new(),
        );
        let mut expected = [0u8; 32];
        expected[..30].copy_from_slice(b"this-topic-name-is-far-longer-"[..30].as_ref());
        assert_eq!(ticket.topic_id, TopicId::from_bytes(expected));
    }
}
