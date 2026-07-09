use crate::basic_app::MemoryDir;
use crate::plugins::crack_plugin::CrackClient;
use crate::plugins::map_plugin::MapTree;
use crate::plugins::states::InitialMapLoadFinished;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::tasks::futures_lite::future;
use bevy::world_serialization::WorldAssetRoot;
use game_logic::api::{FetchArgs, FetchFakeMapTiles};
use game_logic::map::{BBox, FakeMapTile};
use std::collections::BTreeMap;

const MAX_HORIZON_RINGS: u8 = 4;

#[derive(Component)]
pub struct CosmeticMapTile {
    pub octant_path: String,
    pub bbox: BBox,
}

#[derive(Component)]
struct PendingCosmeticVertexLower {
    sink_bbox: BBox,
}

#[derive(Component)]
struct PendingCosmeticShadowDisable;

#[derive(Component)]
struct PendingFakeRingFetch {
    tiles: Vec<FakeMapTile>,
    tasks: Vec<Option<bevy::tasks::Task<anyhow::Result<game_logic::tile::FetchTileResponse>>>>,
    tile_ids: Vec<String>,
    results: Vec<(String, game_logic::tile::FetchTileResponse)>,
}

#[derive(Resource, Default)]
struct FakeHorizonState {
    started: bool,
    tiles_by_depth: BTreeMap<i32, Vec<FakeMapTile>>,
    ring_count: u8,
    next_search_depth: i32,
    sink_bbox: BBox,
    tiles_fetch: Option<bevy::tasks::Task<anyhow::Result<Vec<FakeMapTile>>>>,
    tiles_loaded: bool,
    complete: bool,
}

pub struct FakeMapPlugin;

impl Plugin for FakeMapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FakeHorizonState>()
            .add_systems(
                OnEnter(InitialMapLoadFinished::Finished),
                start_fake_horizon,
            )
            .add_systems(
                Update,
                (
                    poll_fake_map_tiles,
                    begin_next_horizon_ring,
                    poll_fake_ring_fetches,
                    lower_cosmetic_vertices,
                    disable_cosmetic_shadows,
                )
                    .chain(),
            );
    }
}

fn start_fake_horizon(
    mut state: ResMut<FakeHorizonState>,
    map_tree: Res<MapTree>,
    client: Option<Res<CrackClient>>,
) {
    if state.started {
        return;
    }
    let Some(client) = client else {
        return;
    };

    state.started = true;
    state.sink_bbox = map_tree.bbox;
    state.next_search_depth = map_tree
        .roots
        .iter()
        .map(|r| r.path.0.len() as i32)
        .min()
        .unwrap_or(14)
        - 1;

    let api_client = client.0.clone();
    let base_url = crate::config::DATA_BASE_URL.to_string();
    state.tiles_fetch = Some(bevy::tasks::AsyncComputeTaskPool::get().spawn(async move {
        api_client
            .call::<FetchFakeMapTiles>(FetchArgs { base_url })
            .await
    }));

    info!(
        "Fake horizon: fetching coarse tiles (start depth {})",
        state.next_search_depth
    );
}

fn poll_fake_map_tiles(mut state: ResMut<FakeHorizonState>) {
    if state.tiles_loaded || state.complete {
        return;
    }
    let Some(mut task) = state.tiles_fetch.take() else {
        return;
    };

    if let Some(res) = future::block_on(future::poll_once(&mut task)) {
        match res {
            Ok(tiles) => {
                info!("Fake horizon: received {} coarse tiles", tiles.len());
                for tile in tiles {
                    state
                        .tiles_by_depth
                        .entry(tile.depth)
                        .or_default()
                        .push(tile);
                }
                state.tiles_loaded = true;
            }
            Err(e) => {
                tracing::error!("Fake map tiles RPC error: {e:?}");
                state.complete = true;
            }
        }
    } else {
        state.tiles_fetch = Some(task);
    }
}

fn select_ring_tiles(state: &FakeHorizonState) -> Option<(i32, Vec<FakeMapTile>)> {
    let mut depth = state.next_search_depth;
    while depth >= 0 {
        if let Some(tiles) = state.tiles_by_depth.get(&depth) {
            if !tiles.is_empty() && tiles.len() <= 4 {
                return Some((depth, tiles.clone()));
            }
        }
        depth -= 1;
    }
    None
}

fn begin_next_horizon_ring(
    mut commands: Commands,
    mut state: ResMut<FakeHorizonState>,
    client: Option<Res<CrackClient>>,
    q_pending: Query<&PendingFakeRingFetch>,
) {
    if !state.tiles_loaded || state.complete {
        return;
    }
    if state.ring_count >= MAX_HORIZON_RINGS {
        state.complete = true;
        return;
    }
    if !q_pending.is_empty() {
        return;
    }

    let Some((depth, tiles)) = select_ring_tiles(&state) else {
        info!("Fake horizon: no more suitable coarse levels");
        state.complete = true;
        return;
    };
    let Some(client) = client else {
        return;
    };

    state.next_search_depth = depth - 1;

    let mut tasks = Vec::new();
    let mut tile_ids = Vec::new();
    for tile in &tiles {
        let api_client = client.0.clone();
        let base_url = crate::config::DATA_BASE_URL.to_string();
        let glb_path = tile.glb_path.clone();
        let tile_id = tile.octant_path.clone();

        let task = bevy::tasks::AsyncComputeTaskPool::get().spawn(async move {
            api_client
                .call::<game_logic::api::FetchMapTile>(game_logic::tile::FetchTileRequest {
                    base_url,
                    glb_path,
                    tile_id,
                })
                .await
        });
        tasks.push(Some(task));
        tile_ids.push(tile.octant_path.clone());
    }

    info!(
        "Fake horizon: ring {} at depth {} ({} tiles)",
        state.ring_count + 1,
        depth,
        tiles.len()
    );

    commands.spawn(PendingFakeRingFetch {
        tiles,
        tasks,
        tile_ids,
        results: Vec::new(),
    });
}

fn poll_fake_ring_fetches(
    mut commands: Commands,
    mut state: ResMut<FakeHorizonState>,
    mut q_fetches: Query<(Entity, &mut PendingFakeRingFetch)>,
    memory_dir: ResMut<MemoryDir>,
    asset_server: Res<AssetServer>,
) {
    for (entity, mut fetch) in q_fetches.iter_mut() {
        for i in 0..fetch.tasks.len() {
            if let Some(mut task) = fetch.tasks[i].take() {
                if let Some(res) = future::block_on(future::poll_once(&mut task)) {
                    match res {
                        Ok(response) => {
                            let tile_id = fetch.tile_ids[i].clone();
                            fetch.results.push((tile_id, response));
                        }
                        Err(e) => {
                            tracing::error!("Fake horizon tile fetch error: {e:?}");
                        }
                    }
                } else {
                    fetch.tasks[i] = Some(task);
                }
            }
        }

        if !fetch.tasks.iter().all(|t| t.is_none()) {
            continue;
        }

        let sink_bbox = state.sink_bbox;
        let ring_tiles = fetch.tiles.clone();

        for (tile_id, response) in &fetch.results {
            let tile_meta = ring_tiles
                .iter()
                .find(|t| &t.octant_path == tile_id)
                .cloned();
            let Some(tile_meta) = tile_meta else {
                continue;
            };

            let sanitized_id = response
                .tile_id
                .replace('/', "_")
                .replace('\\', "_")
                .replace('.', "_");
            let memory_path = format!("fake_{}.glb", sanitized_id);

            memory_dir.dir.insert_asset(
                std::path::Path::new(&memory_path),
                response.glb_bytes.clone(),
            );

            let asset_path =
                GltfAssetLabel::Scene(0).from_asset(format!("memory://{}", memory_path));
            let handle = asset_server.load(asset_path);

            commands.spawn((
                WorldAssetRoot(handle),
                Visibility::Visible,
                Transform::from_xyz(0.0, 0.0, 0.0),
                CosmeticMapTile {
                    octant_path: tile_meta.octant_path,
                    bbox: tile_meta.bbox,
                },
                PendingCosmeticVertexLower { sink_bbox },
                PendingCosmeticShadowDisable,
            ));
        }

        for tile in &ring_tiles {
            state.sink_bbox = union_bbox(&state.sink_bbox, &tile.bbox);
        }
        state.ring_count += 1;

        info!(
            "Fake horizon: placed ring {}, sink bbox min={:?} max={:?}",
            state.ring_count, state.sink_bbox.min, state.sink_bbox.max
        );

        commands.entity(entity).despawn();
    }
}

fn union_bbox(a: &BBox, b: &BBox) -> BBox {
    BBox {
        min: a.min.min(b.min),
        max: a.max.max(b.max),
    }
}

fn xz_inside_bbox(world_pos: Vec3, bbox: &BBox) -> bool {
    let min_x = bbox.min.x.min(bbox.max.x);
    let max_x = bbox.min.x.max(bbox.max.x);
    let min_z = bbox.min.z.min(bbox.max.z);
    let max_z = bbox.min.z.max(bbox.max.z);
    world_pos.x >= min_x && world_pos.x <= max_x && world_pos.z >= min_z && world_pos.z <= max_z
}

fn lower_cosmetic_vertices(
    mut commands: Commands,
    query: Query<(Entity, &PendingCosmeticVertexLower, &Children)>,
    children_query: Query<&Children>,
    mesh_query: Query<&Mesh3d>,
    global_transform_query: Query<&GlobalTransform>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (root_entity, pending, children) in query.iter() {
        let mut mesh_entities = Vec::new();
        let mut queue: Vec<Entity> = children.to_vec();
        while let Some(ent) = queue.pop() {
            if let Ok(m) = mesh_query.get(ent) {
                mesh_entities.push((ent, m.0.clone()));
            }
            if let Ok(kids) = children_query.get(ent) {
                queue.extend(kids.iter());
            }
        }

        if mesh_entities.is_empty() {
            continue;
        }

        let mut all_loaded = true;
        for (_, handle) in &mesh_entities {
            if meshes.get(handle).is_none() {
                all_loaded = false;
                break;
            }
        }
        if !all_loaded {
            continue;
        }

        let sink_y = pending.sink_bbox.min.y - 1.0;
        let sink_bbox = pending.sink_bbox;

        for (ent, handle) in mesh_entities {
            let Ok(mesh_gt) = global_transform_query.get(ent) else {
                continue;
            };
            let Some(mesh) = meshes.get(&handle).cloned() else {
                continue;
            };
            let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION).map(|v| v.clone())
            else {
                continue;
            };

            let mesh_inv = mesh_gt.affine().inverse();
            let mut new_positions = positions;
            let mut changed = false;
            for pos in &mut new_positions {
                let world = mesh_gt.transform_point(Vec3::from(*pos));
                if xz_inside_bbox(world, &sink_bbox) {
                    let mut sunk = world;
                    sunk.y = sink_y;
                    let local = mesh_inv.transform_point3(sunk);
                    *pos = local.into();
                    changed = true;
                }
            }

            if changed {
                let mut cloned = mesh;
                cloned.insert_attribute(Mesh::ATTRIBUTE_POSITION, new_positions);
                let new_handle = meshes.add(cloned);
                commands.entity(ent).insert(Mesh3d(new_handle));
            }
        }

        commands
            .entity(root_entity)
            .remove::<PendingCosmeticVertexLower>();
    }
}

fn disable_cosmetic_shadows(
    mut commands: Commands,
    roots: Query<(Entity, &Children), With<PendingCosmeticShadowDisable>>,
    children_query: Query<&Children>,
    mesh_query: Query<(), With<Mesh3d>>,
) {
    for (root, children) in roots.iter() {
        let mut queue: Vec<Entity> = children.to_vec();
        let mut found_mesh = false;
        while let Some(ent) = queue.pop() {
            if mesh_query.get(ent).is_ok() {
                commands
                    .entity(ent)
                    .insert((NotShadowCaster, NotShadowReceiver));
                found_mesh = true;
            }
            if let Ok(kids) = children_query.get(ent) {
                queue.extend(kids.iter());
            }
        }
        if found_mesh {
            commands
                .entity(root)
                .remove::<PendingCosmeticShadowDisable>();
        }
    }
}
