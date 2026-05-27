use _crack_utils::sleep_ms;

use crate::declare_api_group2;
use crate::implement_api_group2;

declare_api_group2! {
    WorkerApiGroup2,
    [
        (WorkerPing, (), ()),
    ]
}

implement_api_group2! {
    WorkerApiGroup2,
    [
        (WorkerPing, worker_ping),
    ]
}

pub async fn worker_ping(_x: ()) -> anyhow::Result<()> {
    sleep_ms(1).await;
    Ok(())
}
