use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use crate::{
    api::api_method_macros::{ApiGroupImpls, ApiMethodImpl, ApiMethodInfo},
    crack_worker::WorkerMessage,
};

pub struct ApiImplMapping {
    impl_fns: BTreeMap<String, ApiMethodImpl>,
    info_fns: BTreeMap<String, ApiMethodInfo>,
}

pub fn make_api_mapping(groups: Vec<Arc<dyn ApiGroupImpls>>) -> Arc<ApiImplMapping> {
    for item in &groups {
        tracing::info!("Loading API GROUP: {}", item.grp_name());
    }
    let mut impl_fns = BTreeMap::new();
    let mut info_fns = BTreeMap::new();

    for i in _get_infos2(groups.clone()) {
        let _r = info_fns.insert(i.fullname(), i.clone());
        if _r.is_some() {
            panic!("Duplicate declaration: fullname()={}", i.fullname())
        }
    }

    for i in _get_impls2(groups.clone()) {
        let _r = impl_fns.insert(i.fullname(), i.clone());
        if _r.is_some() {
            panic!("Duplicate implementation: fullname()={}", i.fullname())
        }
    }

    if impl_fns.len() != info_fns.len() {
        tracing::info!(
            "Mismatch in count of {} declarations vs. {} implementations!",
            info_fns.len(),
            impl_fns.len()
        );
        // TODO: get which is missing...
        let keys_a = BTreeSet::from_iter(info_fns.keys().cloned());
        let keys_b = BTreeSet::from_iter(impl_fns.keys().cloned());

        let diff_a = keys_a.difference(&keys_b).collect::<Vec<_>>();
        let diff_b = keys_b.difference(&keys_a).collect::<Vec<_>>();

        tracing::info!("Declarations that are not implemented: {:?}", diff_a);
        tracing::info!("Implementations that are not declared: {:?}", diff_b);

        panic!(
            "
        Mismatch in count of {} declarations vs. {} implementations!
        Declarations that are not implemented: {:?}
        Implementations that are not declared: {:?}
        ",
            info_fns.len(),
            impl_fns.len(),
            diff_a,
            diff_b
        );
    }

    tracing::info!(
        "Api Mapping Singleton Created! func impl count = {}",
        info_fns.len()
    );
    for info_fn in &info_fns {
        tracing::info!("Loaded API Implementation function: {}", info_fn.0);
    }

    Arc::new(ApiImplMapping { impl_fns, info_fns })
}

// async fn get_mapping() -> Arc<ApiImplMapping> {
//     use tokio::sync::OnceCell;
//     static GLOBAL_MAPPING: OnceCell<Arc<ApiImplMapping>> = OnceCell::const_new();

//     GLOBAL_MAPPING
//         .get_or_init(move || async move { Arc::new(make_api_mapping()) })
//         .await
//         .clone()
// }

pub async fn compute_response_message(
    _request: WorkerMessage,
    mapping: Arc<ApiImplMapping>,
) -> WorkerMessage {
    let key = _request.msg_type.clone();
    let (Some(_fn_info), Some(fn_impl)) = (mapping.info_fns.get(&key), mapping.impl_fns.get(&key))
    else {
        return WorkerMessage {
            msg_id: _request.msg_id,
            msg_type: format!("compute_response_message() error: missing key {}", key),
            msg_content: vec![],
        };
    };
    (fn_impl.func)(_request).await
}

fn _get_infos2(grps: Vec<Arc<dyn ApiGroupImpls>>) -> Vec<ApiMethodInfo> {
    let mut v = vec![];
    for grp in grps {
        let mut v2 = grp.method_infos().to_vec();
        v.append(&mut v2);
    }
    v
}

fn _get_impls2(grps: Vec<Arc<dyn ApiGroupImpls>>) -> Vec<ApiMethodImpl> {
    let mut v = vec![];
    for grp in grps {
        let mut v2 = grp.method_impls().to_vec();
        v.append(&mut v2);
    }
    v
}
