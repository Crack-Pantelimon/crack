//! Manifest loading + animation-catalog bootstrap via RPC.

use crate::basic_app::MemoryDir;
use crate::plugins::pedestrians::animation::{AnimationInfo, PedestrianAnimations};
use bevy::prelude::*;

/// A fully-recombined pedestrian asset URL (manifest folder + inner manifest line).
#[derive(Component, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PedestrianUrl(pub String);

/// Public manifest resource: the list of available pedestrian URLs and whether loading finished.
#[derive(Resource, Default)]
pub struct PedestrianManifest {
    pub urls: Vec<PedestrianUrl>,
    pub loaded: bool,
}

#[derive(Resource, Default)]
pub struct PedestrianManifestTasks {
    pub manifest_task:
        Option<bevy::tasks::Task<anyhow::Result<game_logic::pedestrian::PedestrianManifestResult>>>,
    pub first_glb_task:
        Option<bevy::tasks::Task<anyhow::Result<game_logic::glb::FetchGlbResponse>>>,
    pub first_gltf: Option<Handle<bevy::gltf::Gltf>>,
}

fn parse_url_to_rpc_args(url: &str) -> (String, String) {
    let base_url = crate::config::DATA_BASE_URL.trim_end_matches('/');
    let glb_path = if url.starts_with(base_url) {
        url[base_url.len()..].trim_start_matches('/').to_string()
    } else {
        if let Some(pos) = url.find("/3d_data/") {
            url[pos..].trim_start_matches('/').to_string()
        } else {
            url.to_string()
        }
    };
    let asset_id = url.split('/').last().unwrap_or(url).to_string();
    (glb_path, asset_id)
}

pub fn start_manifest_load(mut commands: Commands) {
    commands.init_resource::<PedestrianManifestTasks>();
}

pub fn spawn_pedestrian_manifest_task(
    mut tasks: ResMut<PedestrianManifestTasks>,
    manifest: Res<PedestrianManifest>,
    client: Option<Res<crate::plugins::crack_plugin::CrackClient>>,
) {
    let Some(client) = client else {
        return;
    };
    if !manifest.loaded
        && tasks.manifest_task.is_none()
        && tasks.first_glb_task.is_none()
        && tasks.first_gltf.is_none()
    {
        let api_client = client.0.clone();
        let base_url = crate::config::DATA_BASE_URL.to_string();
        let task = bevy::tasks::AsyncComputeTaskPool::get().spawn(async move {
            api_client
                .call::<game_logic::api::FetchPedestrianManifest>(game_logic::api::FetchArgs {
                    base_url,
                })
                .await
        });
        tasks.manifest_task = Some(task);
    }
}

pub fn poll_pedestrian_manifest_task(
    mut tasks: ResMut<PedestrianManifestTasks>,
    mut manifest: ResMut<PedestrianManifest>,
    client: Option<Res<crate::plugins::crack_plugin::CrackClient>>,
) {
    let Some(client) = client else {
        return;
    };
    if let Some(mut task) = tasks.manifest_task.take() {
        if let Some(res) = bevy::tasks::futures_lite::future::block_on(
            bevy::tasks::futures_lite::future::poll_once(&mut task),
        ) {
            match res {
                Ok(result) => {
                    info!("Parsed pedestrian manifest: {} entries.", result.urls.len());
                    manifest.urls = result
                        .urls
                        .iter()
                        .map(|u| PedestrianUrl(u.clone()))
                        .collect();

                    if let Some(first_url) = result.urls.first() {
                        let (glb_path, asset_id) = parse_url_to_rpc_args(first_url);
                        let api_client = client.0.clone();
                        let base_url = crate::config::DATA_BASE_URL.to_string();
                        let glb_task = bevy::tasks::AsyncComputeTaskPool::get().spawn(async move {
                            api_client
                                .call::<game_logic::api::FetchPedestrianModel>(
                                    game_logic::glb::FetchGlbRequest {
                                        base_url,
                                        glb_path,
                                        asset_id,
                                    },
                                )
                                .await
                        });
                        tasks.first_glb_task = Some(glb_task);
                    } else {
                        manifest.loaded = true;
                    }
                }
                Err(e) => {
                    tracing::error!("Pedestrian manifest RPC error: {e:?}");
                }
            }
        } else {
            tasks.manifest_task = Some(task);
        }
    }
}

pub fn poll_pedestrian_first_glb_task(
    mut tasks: ResMut<PedestrianManifestTasks>,
    memory_dir: ResMut<MemoryDir>,
    asset_server: Res<AssetServer>,
) {
    if let Some(mut task) = tasks.first_glb_task.take() {
        if let Some(res) = bevy::tasks::futures_lite::future::block_on(
            bevy::tasks::futures_lite::future::poll_once(&mut task),
        ) {
            match res {
                Ok(response) => {
                    let memory_path = "first_pedestrian.glb";
                    memory_dir.dir.insert_asset(
                        std::path::Path::new(memory_path),
                        response.glb_bytes.clone(),
                    );

                    let gltf_url = format!("memory://{}", memory_path);
                    let gltf_handle = asset_server.load::<bevy::gltf::Gltf>(gltf_url);
                    tasks.first_gltf = Some(gltf_handle);
                }
                Err(e) => {
                    tracing::error!("First pedestrian GLB fetch RPC error: {e:?}");
                }
            }
        } else {
            tasks.first_glb_task = Some(task);
        }
    }
}

pub fn load_pedestrian_manifest_system(
    mut bootstrap: ResMut<PedestrianManifestTasks>,
    mut manifest: ResMut<PedestrianManifest>,
    mut anims: ResMut<PedestrianAnimations>,
    gltf_assets: Res<Assets<bevy::gltf::Gltf>>,
    clip_assets: Res<Assets<AnimationClip>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    if manifest.loaded {
        return;
    }
    let Some(first_gltf) = bootstrap.first_gltf.clone() else {
        return;
    };
    let Some(gltf) = gltf_assets.get(&first_gltf) else {
        return;
    };

    let mut names = std::collections::BTreeSet::new();
    let mut clips = Vec::new();
    let mut clip_to_name = std::collections::HashMap::new();
    for (name, clip_handle) in &gltf.named_animations {
        let name_str = name.to_string();
        if names.insert(name_str.clone()) {
            clips.push(clip_handle.clone());
            clip_to_name.insert(clip_handle.id(), name_str);
        }
    }

    let (graph, node_indices) = AnimationGraph::from_clips(clips.clone());
    let graph_handle = graphs.add(graph);

    let mut nodes = std::collections::HashMap::new();
    let mut catalog = std::collections::BTreeMap::new();
    for (idx, clip_handle) in clips.iter().enumerate() {
        let Some(name) = clip_to_name.get(&clip_handle.id()) else {
            continue;
        };
        let node = node_indices[idx];
        nodes.insert(name.clone(), node);

        let duration = clip_assets
            .get(clip_handle)
            .map(|c| c.duration())
            .unwrap_or(0.0);
        let frames = (duration * 30.0).round() as u32;
        catalog.insert(
            name.clone(),
            AnimationInfo {
                name: name.clone(),
                duration,
                frames,
                node,
            },
        );
    }

    info!(
        "Pedestrian animation catalog ready: {} animations.",
        catalog.len()
    );

    anims.graph_handle = graph_handle;
    anims.nodes = nodes;
    anims.catalog = catalog;
    anims.ready = true;

    bootstrap.first_gltf = None;
    manifest.loaded = true;
}

#[derive(Asset, bevy::reflect::TypePath, Debug, Clone)]
pub struct TextAsset {
    pub text: String,
}

#[derive(Default, bevy::reflect::TypePath)]
pub struct TextAssetLoader;

impl bevy::asset::AssetLoader for TextAssetLoader {
    type Asset = TextAsset;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(
        &self,
        reader: &mut dyn bevy::asset::io::Reader,
        _settings: &Self::Settings,
        _load_context: &mut bevy::asset::LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let text = String::from_utf8(bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(TextAsset { text })
    }

    fn extensions(&self) -> &[&str] {
        &["txt"]
    }
}
