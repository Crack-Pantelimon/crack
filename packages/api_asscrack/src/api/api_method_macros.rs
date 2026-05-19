use serde::{Deserialize, Serialize};

pub trait ApiGroup {
    const GROUP: &'static str;
}

pub trait ApiMethod {
    const NAME: &'static str;
    type Grp:  ApiGroup;
    type Arg: Clone + std::fmt::Debug + Serialize + for<'a> Deserialize<'a>;
    type Ret: std::fmt::Debug + Serialize + for<'a> Deserialize<'a>;
}


#[macro_export]
macro_rules! declare_api_group {
    ($name:tt) => {
        paste::paste! {
            pub struct $name;
            impl $crate::api::api_method_macros::ApiGroup for $name {
                const GROUP: &str = stringify!($name);
            }
            // inventory::submit!{
            //     $crate::server_chat_api::api_method_macros::ApiMethodInfoStatic::new(stringify!($name), stringify!($arg), stringify!($ret))
            // }
        }
    };
}

#[macro_export]
macro_rules! declare_api_method {
    ($grp:tt, $name:tt, $arg:ty, $ret:ty) => {
        paste::paste! {
            pub struct $name;
            impl $crate::api::api_method_macros::ApiMethod for $name {
                const NAME: &str = stringify!($name);
                type Grp = $grp;
                type Arg = $arg;
                type Ret = $ret;
            }
            // inventory::submit!{
            //     $crate::server_chat_api::api_method_macros::ApiMethodInfoStatic::new(stringify!($name), stringify!($arg), stringify!($ret))
            // }
        }
    };
}