pub use n0_future;

pub fn get_timestamp_now_ms() -> i64 {
    chrono::offset::Utc::now().timestamp_millis()
}

pub fn spawn<F>(f: F) -> n0_future::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    n0_future::task::spawn(f)
}

pub fn random_u32() -> u32 {
    ::rand::random()
}

pub async fn sleep_ms(dt_ms: u32) {
    let _sleep = n0_future::time::sleep(std::time::Duration::from_millis(dt_ms as u64)).await;
}

#[test]
fn test_get_timestamp_now_ms() {
    get_timestamp_now_ms();
}

#[test]
fn test_get_random_u32() {
    random_u32();
}
