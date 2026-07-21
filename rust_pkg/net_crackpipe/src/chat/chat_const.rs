use std::time::Duration;

pub const GLOBAL_CHAT_TOPIC_ID: &str = "global";

pub const PRESENCE_INTERVAL: Duration = Duration::from_secs(7);
pub const PRESENCE_IDLE: Duration = Duration::from_secs(16);
pub const PRESENCE_EXPIRATION: Duration = Duration::from_secs(30);
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
pub const GLOBAL_PERIODIC_TASK_INTERVAL: Duration = Duration::from_secs(5);

pub fn get_relay_domain() -> (String, String) {
    (
        String::from("https://net2.sparganothis.org"),
        String::from("https://net.sparganothis.org/pkarr"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test as test;

    #[test]
    fn smoke_get_relay_domain() {
        let (relay, pkarr) = get_relay_domain();
        assert_eq!(relay, "https://net2.sparganothis.org");
        assert_eq!(pkarr, "https://net.sparganothis.org/pkarr");
    }

    #[test]
    fn smoke_presence_constant_ordering() {
        // Idle must outlive a broadcast interval, expiration must outlive idle.
        assert!(PRESENCE_IDLE > PRESENCE_INTERVAL);
        assert!(PRESENCE_EXPIRATION > PRESENCE_IDLE);
        assert_eq!(GLOBAL_CHAT_TOPIC_ID, "global");
    }
}
