use api_asscrack::api::api_client::ApiClient;
use bevy::prelude::*;
use bevy::tasks::Task;
use std::sync::{Arc, Mutex};

pub mod lod_flow;
pub mod manifest_flow;
pub mod osm_flow;

#[derive(Resource, Clone)]
pub struct CrackClient(pub ApiClient);

#[derive(Resource, Clone)]
pub struct CrackClientSlot(pub Arc<Mutex<Option<anyhow::Result<ApiClient>>>>);

#[cfg(not(target_family = "wasm"))]
#[derive(Resource, Clone)]
pub struct CrackRuntime(pub Arc<tokio::runtime::Runtime>);

#[derive(Resource, Default)]
pub struct CrackTasks {
    pub manifest: Option<Task<anyhow::Result<game_logic::map::MapManifestResult>>>,
    pub osm: Option<Task<anyhow::Result<game_logic::osm::OsmDataResult>>>,
    pub lod: Option<Task<anyhow::Result<game_logic::lod::LodComputeResponse>>>,
}

pub struct CrackPlugin;

impl Plugin for CrackPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CrackTasks>()
            .insert_resource(CrackClientSlot(Arc::new(Mutex::new(None))))
            .add_systems(Startup, start_crack_client_init)
            .add_systems(
                Update,
                (
                    install_crack_client,
                    (
                        manifest_flow::poll_manifest_task,
                        manifest_flow::spawn_manifest_task,
                        osm_flow::poll_osm_task,
                        osm_flow::spawn_osm_task,
                        lod_flow::poll_lod_task,
                        lod_flow::spawn_lod_task,
                    )
                        .chain()
                        .run_if(resource_exists::<CrackClient>),
                ),
            );
    }
}

fn start_crack_client_init(mut commands: Commands) {
    let slot = Arc::new(Mutex::new(None));
    commands.insert_resource(CrackClientSlot(slot.clone()));

    #[cfg(not(target_family = "wasm"))]
    {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap(),
        );
        commands.insert_resource(CrackRuntime(runtime.clone()));

        // `ThreadWorkerFactory::load_worker` returns a `!Send` future, so it cannot
        // be `runtime.spawn`'d. Drive the init with `block_on` on a dedicated std
        // thread; the multi-thread runtime's worker threads keep the worker pipe
        // dispatcher (spawned during init) alive for the app's lifetime.
        let rt = runtime.clone();
        std::thread::spawn(move || {
            let res = rt.block_on(init_client());
            *slot.lock().unwrap() = Some(res);
        });
    }

    #[cfg(target_family = "wasm")]
    {
        wasm_bindgen_futures::spawn_local(async move {
            let res = init_client().await;
            *slot.lock().unwrap() = Some(res);
        });
    }
}

fn install_crack_client(
    mut commands: Commands,
    slot: Res<CrackClientSlot>,
    client: Option<Res<CrackClient>>,
) {
    if client.is_some() {
        return;
    }

    let mut guard = slot.0.lock().unwrap();
    if let Some(res) = guard.take() {
        match res {
            Ok(client) => {
                tracing::info!("CrackClient successfully initialized.");
                commands.insert_resource(CrackClient(client));
            }
            Err(e) => {
                tracing::error!("Failed to initialize CrackClient: {e:?}");
            }
        }
    }
}

async fn init_client() -> anyhow::Result<ApiClient> {
    #[cfg(not(target_family = "wasm"))]
    let factory = thread_worker::spawn_in_process_worker().await?;

    #[cfg(target_family = "wasm")]
    let factory = {
        use api_asscrack::crack_worker::WorkerLoaderFactory as _;
        web_serviceworker_crackloader::WebWorkerFactory {}
            .load_worker()
            .await?
    };

    let client = ApiClient::new(factory);
    client
        .call::<api_asscrack::api::api_worker_declarations::WorkerPing>(())
        .await?;

    // Run migrations
    client
        .call::<game_logic::api::RunGameMigrations>(())
        .await?;

    Ok(client)
}
