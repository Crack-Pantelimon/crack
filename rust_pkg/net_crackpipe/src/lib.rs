//! net_crackpipe — P2P networking layer for crackpipe.
//!
//! Provides P2P networking via iroh, including global matchmaking, chat,
//! direct messaging, echo protocol, sleep management, and user identity.

use chrono::{DateTime, Utc};

pub mod _bootstrap_keys;
pub(crate) mod _random_word;

/// Chat subsystem: rooms, presence, direct messages and global chat protocol.
pub mod chat;
/// Echo protocol handler used by bootstrap nodes for connectivity checks.
pub mod echo;
/// `GlobalMatchmaker`: owns local + bootstrap nodes, global chat and periodic tasks.
pub mod global_matchmaker;
/// `MainNode`: full P2P node wrapper binding router, gossip, and protocols.
pub mod main_node;
/// Single entry-point wiring all running network code and joined rooms.
pub mod network_manager;
/// Shared `SleepManager` honouring early wake-ups across the net stack.
pub mod sleep;
/// User and node identities built on iroh public keys.
pub mod user_identity;
/// Signed message primitives and chat room type trait.
pub(crate) mod signed_message;

/// Returns the current UNIX timestamp in microseconds.
pub fn timestamp_micros() -> u128 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .unwrap()
        .as_micros()
}

/// Returns the current UTC datetime with microsecond precision.
pub fn datetime_now() -> DateTime<Utc> {
    let timestamp = timestamp_micros() as i64;
    DateTime::<Utc>::from_timestamp_micros(timestamp).unwrap()
}

pub use iroh::PublicKey;
pub use paste;
pub use postcard;
pub use signed_message::*;

// Browser tests (wasm-pack test --headless --firefox --chrome): rand ->
// getrandom needs a real browser, node is not sufficient.
#[cfg(all(test, target_arch = "wasm32"))]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test as test;

    #[test]
    fn smoke_timestamp_micros() {
        let ts = timestamp_micros();
        assert!(ts > 0, "timestamp should be positive, got {ts}");
    }

    #[test]
    fn smoke_datetime_now() {
        let now = datetime_now();
        assert!(
            now.timestamp() > 1_700_000_000,
            "datetime_now should be past Nov 2023, got {now}"
        );
    }
}
