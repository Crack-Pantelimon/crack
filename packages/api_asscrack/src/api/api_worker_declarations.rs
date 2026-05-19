use crate::{declare_api_group, declare_api_method};


declare_api_group!(WorkerApiGroup);

declare_api_method!(WorkerApiGroup, WorkerOk, (), ());

declare_api_method!(WorkerApiGroup, RunSql, String, String);