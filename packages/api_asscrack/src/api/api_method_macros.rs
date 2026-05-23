use serde::{Deserialize, Serialize};

pub trait ApiGroupDecl {
    const GROUP: &'static str;
}

pub trait ApiMethodDecl {
    const NAME: &'static str;
    type Grp:  ApiGroupDecl;
    type Arg: Clone + std::fmt::Debug + Serialize + for<'a> Deserialize<'a>+'static+Send;
    type Ret: std::fmt::Debug + Serialize + for<'a> Deserialize<'a>+'static+Send;

    fn fullname() -> String {
        let a = <Self::Grp as ApiGroupDecl>::GROUP;
        let b = Self::NAME;
        format!("{a}.{b}")
    }
    fn wrap_impl(
        _func: fn(Self::Arg)->std::pin::Pin<Box<
            dyn futures::Future<Output=anyhow::Result<Self::Ret>>+Send
        >>,
        msg: WorkerMessage
    ) -> std::pin::Pin<Box<dyn futures::Future<Output=WorkerMessage>>> {
        let msg_id = msg.msg_id;
        let arg = postcard::from_bytes::<Self::Arg>(&msg.msg_content);
        
        

        // let arg = post
        // let ret = func(arg);
        // let v = postcard::to_vec(value)
        use futures::FutureExt;
        async move {

            let arg = match arg  {
                Ok(o) => {
                    o
                }
                Err(e) => {
                    return WorkerMessage {
                        msg_id: msg_id,
                        msg_type: "error_deserialize_arg".to_string(),
                        msg_content: format!("{e:#?}").as_bytes().to_vec(),
                    };
                }
            };
            let ret = _func(arg).await;
            let ret: Result<<Self as ApiMethodDecl>::Ret, String> = ret.map_err(|e| format!("{e:#?}"));

            let msg_content: Vec<u8> = match postcard::to_stdvec(&ret) {
                Ok(m) => {
                    m
                },
                 Err(e) => {
                    return WorkerMessage {
                        msg_id: msg_id,
                        msg_type: "error_serialize_ret".to_string(),
                        msg_content: format!("{e:#?}").as_bytes().to_vec(),
                    };
                }
            };
            

            WorkerMessage {
                msg_id,
                msg_type: "return".to_string(),
                msg_content
            }
        }.boxed()
    }
}

use linkme::distributed_slice;

use crate::crack_worker::WorkerMessage;

#[distributed_slice]
pub static API_GROUP_DECLS: [&'static str];



#[distributed_slice]
pub static API_METHOD_DECLS: [ApiMethodInfo];

pub struct ApiMethodInfo {
    pub name: &'static str,
    pub grp: &'static str,
    pub arg: &'static str,
    pub ret: &'static str,
}


pub struct ApiMethodImpl {
    pub name: &'static str,
    pub func: fn (WorkerMessage) -> std::pin::Pin<Box<dyn futures::Future<Output=WorkerMessage>>>,
}

#[distributed_slice]
pub static API_METHOD_IMPLS: [ApiMethodImpl];

#[macro_export]
macro_rules! declare_api_group {
    ($name:tt) => {
        paste::paste! {
            pub struct $name;
            impl $crate::api::api_method_macros::ApiGroupDecl for $name {
                const GROUP: &str = stringify!($name);
            }
            #[crate::linkme::distributed_slice(crate::api::api_method_macros::API_GROUP_DECLS)]
            #[allow(non_upper_case_globals)]
            static [<__API_GROUPS_DECL_INVENTORY_ $name >]: &'static str = stringify!($name);
        }
    };
}

#[macro_export]
macro_rules! declare_api_method {
    ($grp:tt, $name:tt, $arg:ty, $ret:ty) => {
        paste::paste! {
            pub struct $name;
            impl $crate::api::api_method_macros::ApiMethodDecl for $name {
                const NAME: &str = stringify!($name);
                type Grp = $grp;
                type Arg = $arg;
                type Ret = $ret;
            }
            #[crate::linkme::distributed_slice(crate::api::api_method_macros::API_METHOD_DECLS)]
            #[allow(non_upper_case_globals)]
            static [<__API_METHODS_DECL_INVENTORY_ $name>]: crate::api::api_method_macros::ApiMethodInfo = crate::api::api_method_macros::ApiMethodInfo {
                name: stringify!($name),
                grp: stringify!($grp),
                arg: stringify!($arg),
                ret: stringify!($ret),
            };
        }
    };
}

#[macro_export]
macro_rules! impl_api_method {
    ($name:tt, $func:expr) => {
        paste::paste! {
            
            #[crate::linkme::distributed_slice(crate::api::api_method_macros::API_METHOD_IMPLS)]
            #[allow(non_upper_case_globals)]
            static [<__impl__ $name __ $func>]: crate::api::api_method_macros::ApiMethodImpl 
                    = crate::api::api_method_macros::ApiMethodImpl {
                func: [<__ $name __wrapper_outer>],
                name: <$name as $crate::api::api_method_macros::ApiMethodDecl>::NAME,
            };

            #[allow(nonstandard_style)]
            fn [<__ $name __wrapper1>](
                x: <$name as $crate::api::api_method_macros::ApiMethodDecl>::Arg
            ) -> std::pin::Pin<Box<
                dyn $crate::futures::Future<
                    Output=$crate::anyhow::Result<
                        <$name as $crate::api::api_method_macros::ApiMethodDecl>::Ret
                    >
                >+Send
            >> {
                use $crate::futures::FutureExt;
                $func(x).boxed()
            }



            #[allow(nonstandard_style)]
            fn [<__ $name __wrapper_outer>] (msg: WorkerMessage) -> std::pin::Pin<Box<
                dyn $crate::futures::Future<Output=WorkerMessage>
            >> {
                <$name as $crate::api::api_method_macros::ApiMethodDecl>::wrap_impl(
                    [<__ $name __wrapper1>],
                    msg,
                )
            }



        }
    };
}