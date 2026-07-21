use std::{sync::Arc, time::Duration};

use n0_future::time::Instant;
use tokio::sync::Notify;

/// Sleep manager for interruptible sleeps.
/// Wraps an inner state that can be woken early via `wake_up()`.
#[derive(Clone, Debug)]
pub struct SleepManager {
    inner: Arc<SleepManagerInner>,
}

impl SleepManager {
    /// Creates a new sleep manager.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(SleepManagerInner::new()),
        }
    }
    /// Sleeps for the given duration, but can be woken early by calling `wake_up()`.
    pub async fn sleep(&self, duration: Duration) {
        self.inner.sleep(duration).await;
    }
    /// Wakes up all current and future sleep calls immediately.
    pub fn wake_up(&self) {
        self.inner.wake_up();
    }
}

#[derive(Debug)]
struct SleepManagerInner {
    trigger: Notify,
}

impl SleepManagerInner {
    fn new() -> Self {
        Self {
            trigger: Notify::new(),
        }
    }
    async fn sleep(&self, duration: Duration) {
        let mut millis_left = duration.as_micros() as i128;
        while millis_left > 0 {
            let now = Instant::now();
            n0_future::future::race(
                n0_future::time::sleep(Duration::from_micros(millis_left as u64)),
                self.trigger.notified(),
            )
            .await;
            millis_left -= now.elapsed().as_micros() as i128;
        }
    }
    fn wake_up(&self) {
        self.trigger.notify_waiters();
        self.trigger.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test as test;

    #[test]
    fn smoke_construct_and_wake() {
        let sm = SleepManager::new();
        sm.wake_up(); // waking with no sleepers must not panic
    }

    async fn sleep_body() {
        let sm = SleepManager::new();
        let before = crate::timestamp_micros();
        sm.sleep(Duration::from_millis(20)).await;
        let elapsed_ms = (crate::timestamp_micros() - before) / 1000;
        assert!(
            elapsed_ms >= 15,
            "sleep(20ms) returned after {elapsed_ms}ms"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn smoke_sleep_duration() {
        sleep_body().await;
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test::wasm_bindgen_test]
    async fn smoke_sleep_duration() {
        sleep_body().await;
    }
}
