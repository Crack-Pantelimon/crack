use crate::crack_worker::WorkerMessage;
use serde::{Deserialize, Serialize};
pub trait ApiGroupDecl {
    const GROUP: &'static str;
}
pub trait ApiGroupMethods {
    fn grp_name(&self) -> &'static str;
    fn method_infos(&self) -> &'static [ApiMethodInfo];
}

pub trait ApiGroupImpls: ApiGroupMethods {
    fn method_impls(&self) -> &'static [ApiMethodImpl];
}

#[derive(Clone, Copy)]
pub struct ApiGroupDeclStatic {
    pub group: &'static str,
}

pub trait ApiMethodDecl {
    const NAME: &'static str;
    type Grp: ApiGroupDecl;
    type Arg: Clone + std::fmt::Debug + Serialize + for<'a> Deserialize<'a> + 'static + Send;
    type Ret: std::fmt::Debug + Serialize + for<'a> Deserialize<'a> + 'static + Send;

    fn fullname() -> String {
        let a = <Self::Grp as ApiGroupDecl>::GROUP;
        let b = Self::NAME;
        format!("{a}.{b}")
    }
    #[allow(clippy::type_complexity)]
    fn wrap_impl(
        _func: fn(
            Self::Arg,
        ) -> std::pin::Pin<
            Box<dyn futures::Future<Output = anyhow::Result<Self::Ret>> + Send>,
        >,
        msg: WorkerMessage,
    ) -> std::pin::Pin<Box<dyn futures::Future<Output = WorkerMessage> + Send>> {
        let msg_id = msg.msg_id;
        let arg = postcard::from_bytes::<Self::Arg>(&msg.msg_content);

        // let arg = post
        // let ret = func(arg);
        // let v = postcard::to_vec(value)
        use futures::FutureExt;
        async move {
            let arg = match arg {
                Ok(o) => o,
                Err(e) => {
                    return WorkerMessage {
                        msg_id,
                        msg_type: "error_deserialize_arg".to_string(),
                        msg_content: format!("{e:#?}").as_bytes().to_vec(),
                    };
                }
            };
            let ret = _func(arg).await;
            let ret: Result<<Self as ApiMethodDecl>::Ret, String> =
                ret.map_err(|e| format!("{e:#?}"));

            let msg_content: Vec<u8> = match postcard::to_stdvec(&ret) {
                Ok(m) => m,
                Err(e) => {
                    return WorkerMessage {
                        msg_id,
                        msg_type: "error_serialize_ret".to_string(),
                        msg_content: format!("{e:#?}").as_bytes().to_vec(),
                    };
                }
            };

            WorkerMessage {
                msg_id,
                msg_type: "return".to_string(),
                msg_content,
            }
        }
        .boxed()
    }
}

#[derive(Clone, Debug)]
pub struct ApiMethodInfo {
    pub name: &'static str,
    pub grp: &'static str,
    pub arg: &'static str,
    pub ret: &'static str,
}

#[derive(Clone)]
pub struct ApiMethodImpl {
    pub name: &'static str,
    pub grp: &'static str,
    pub func:
        fn(WorkerMessage) -> std::pin::Pin<Box<dyn futures::Future<Output = WorkerMessage> + Send>>,
}

impl ApiMethodImpl {
    pub fn fullname(&self) -> String {
        let b = self.name;
        let a = self.grp;
        format!("{a}.{b}")
    }
}

impl ApiMethodInfo {
    pub fn fullname(&self) -> String {
        let b = self.name;
        let a = self.grp;
        format!("{a}.{b}")
    }
}


#[macro_export]
macro_rules! declare_api_method_before2 {
    ($grp:tt, $name:tt, $arg:ty, $ret:ty) => {
        $crate::paste::paste! {
            #[derive(Debug, Clone, Copy)]
            pub struct $name;
            impl $crate::api::api_method_macros::ApiMethodDecl for $name {
                const NAME: &str = stringify!($name);
                type Grp = $grp;
                type Arg = $arg;
                type Ret = $ret;
            }
        }
    };
}

#[macro_export]
macro_rules! declare_api_method_after2 {
    ($grp:tt, $name:tt, $arg:ty, $ret:ty) => {
        $crate::paste::paste! {
                $crate::api::api_method_macros::ApiMethodInfo {
                    name: stringify!($name),
                    grp: stringify!($grp),
                    arg: stringify!($arg),
                    ret: stringify!($ret),
                }
        }
    };
}

#[macro_export]
macro_rules! declare_api_group2 {
    ($name:tt, [$(($mname:tt, $arg:ty, $ret:ty),)*]) => {
        #[derive(Debug, Copy, Clone)]
        pub struct $name;
        impl $crate::api::api_method_macros::ApiGroupDecl for $name {
            const GROUP: &str = stringify!($name);
        }
        $(
            $crate::declare_api_method_before2!( $name, $mname, $arg, $ret);
        )*

        impl $crate::api::api_method_macros::ApiGroupMethods for $name {
            fn grp_name(&self) -> &'static str {stringify!($name)}
            fn method_infos(&self)
            -> &'static [$crate::api::api_method_macros::ApiMethodInfo]
            {
            &[
                $(
                    $crate::declare_api_method_after2!(
                        $name, $mname, $arg, $ret),
                )*
            ]
            }
        }
    };
}

#[macro_export]
macro_rules! declare_api_method_impl_before2 {
    ($name:tt, $func:expr) => {
        $crate::paste::paste! {

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
            fn [<__ $name __wrapper_outer>] (msg: $crate::crack_worker::WorkerMessage) -> std::pin::Pin<Box<
                dyn $crate::futures::Future<Output=$crate::crack_worker::WorkerMessage>+Send
            >> {
                <$name as $crate::api::api_method_macros::ApiMethodDecl>::wrap_impl(
                    [<__ $name __wrapper1>],
                    msg,
                )
            }

        }
    };
}

#[macro_export]
macro_rules! declare_api_method_impl_after2 {
    ($name:tt, $func:expr) => {
        $crate::paste::paste! {

            $crate::api::api_method_macros::ApiMethodImpl {
                func: [<__ $name __wrapper_outer>],
                name: <$name as $crate::api::api_method_macros::ApiMethodDecl>::NAME,
                grp: <<$name as $crate::api::api_method_macros::ApiMethodDecl>::Grp
                as $crate::api::api_method_macros::ApiGroupDecl>::GROUP,
            }
        }
    };
}

#[macro_export]
macro_rules! implement_api_group2 {
    ($name:tt, [$(($mname:tt, $arg:expr),)*]) => {
           $(
                $crate::declare_api_method_impl_before2!($mname, $arg);
            )*
        impl $crate::api::api_method_macros::ApiGroupImpls for $name {
            fn method_impls(&self) -> &'static [$crate::api::api_method_macros::ApiMethodImpl] {
            &[
            $(
                $crate::declare_api_method_impl_after2!($mname, $arg),
            )*
                ]
            }
        }
    };
}
