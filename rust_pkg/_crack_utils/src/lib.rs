//! Cross-platform asynchronous and time-related utilities for Crack projects.
//!
//! The crate presents a small, runtime-neutral surface that works on both
//! native targets and WebAssembly. It re-exports [`n0_future`] so callers can
//! use compatible task and timing primitives without adding a separate
//! dependency.

/// Re-exports the asynchronous runtime abstraction used by this crate.
pub use n0_future;

/// Returns the current UTC Unix timestamp in milliseconds.
///
/// The value is measured from the Unix epoch and is suitable for logging,
/// elapsed-time calculations, and protocol timestamps. It is not monotonic, so
/// wall-clock adjustments can make successive values move backward.
pub fn get_timestamp_now_ms() -> i64 {
    chrono::offset::Utc::now().timestamp_millis()
}

/// Spawns a `Send` future on the runtime selected by [`n0_future`].
///
/// `f` is scheduled for concurrent execution and may outlive the caller. Its
/// output must be `Send` because the runtime is free to run the task on another
/// thread. The returned handle can be awaited to retrieve that output.
pub fn spawn<F>(f: F) -> n0_future::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    n0_future::task::spawn(f)
}

/// Generates a uniformly distributed pseudorandom `u32`.
///
/// The value is obtained from the platform-backed random-number generator
/// configured by `rand`, including the WebAssembly-compatible backend on wasm
/// targets.
pub fn random_u32() -> u32 {
    ::rand::random()
}

/// Suspends the current task for at least `dt_ms` milliseconds.
///
/// `dt_ms` is converted to a [`std::time::Duration`] and scheduled through
/// [`n0_future::time::sleep`]. Actual wake-up time can be longer when the
/// runtime or platform scheduler is busy.
pub async fn sleep_ms(dt_ms: u32) {
    let _sleep = n0_future::time::sleep(std::time::Duration::from_millis(dt_ms as u64)).await;
}

#[cfg(test)]
mod tests {
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test as test;
    use super::*;

    #[test]
    fn smoke_get_timestamp_now_ms() {
        let ts = get_timestamp_now_ms();
        assert!(ts > 0, "timestamp should be positive, got {ts}");
    }

    #[test]
    fn smoke_random_u32() {
        // Two draws should (overwhelmingly) differ; guards against a stubbed RNG.
        assert_ne!(random_u32(), random_u32());
    }

    async fn sleep_ms_body() {
        let before = get_timestamp_now_ms();
        sleep_ms(20).await;
        let elapsed = get_timestamp_now_ms() - before;
        assert!(elapsed >= 15, "sleep_ms(20) returned after {elapsed}ms");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn smoke_sleep_ms() {
        sleep_ms_body().await;
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test::wasm_bindgen_test]
    async fn smoke_sleep_ms() {
        sleep_ms_body().await;
    }
}
