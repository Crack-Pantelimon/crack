use crate::{crack_worker::WorkerMessage, declare_api_group, declare_api_method, impl_api_method};


declare_api_group!(WorkerApiGroup);

declare_api_method!(WorkerApiGroup, WorkerPing, (), ());

declare_api_method!(WorkerApiGroup, RunSql, String, String);


pub async fn worker2(_x: ()) -> anyhow::Result<()> {
    Ok(())
}

impl_api_method!(WorkerPing, worker2);
