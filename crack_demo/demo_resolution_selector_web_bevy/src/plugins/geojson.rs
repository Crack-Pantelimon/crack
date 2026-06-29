use bevy::asset::{Asset, AssetLoader, LoadContext, io::Reader};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy_egui::EguiPrimaryContextPass;
use bevy_egui::{EguiContexts, egui};
use std::collections::BTreeMap;

use crate::plugins::game_freecam::camera_controls::ActiveCameraAnimation;
use crate::plugins::states::OsmDatabaseLoadFinished;

pub struct GeoJsonPlugin;

impl Plugin for GeoJsonPlugin {
    fn build(&self, app: &mut App) {
        info!("loading: GeoJsonPlugin...");
        app.init_asset::<GeoJsonTextAsset>()
            .init_asset_loader::<GeoJsonTextAssetLoader>()
            .init_resource::<GeoJsonDatabase>()
            .init_resource::<GeoJsonSearchState>()
            .init_resource::<GeoJsonSelection>()
            .init_resource::<GeoJsonLoaderState>()
            .init_resource::<GameLoadingStatus>()
            .init_resource::<TooltipNotificationState>()
            .init_resource::<OsmOverlayState>()
            .add_systems(
                EguiPrimaryContextPass,
                (geojson_ui_system, geojson_text_labels_system, osm_overlay_ui_system),
            )
            .add_systems(
                Update,
                (
                    trigger_geojson_loading,
                    check_geojson_loading,
                    project_geojson_coordinates,
                    geojson_gizmos_system,
                    update_geojson_loading_finished,
                    update_tooltip_timers,
                    osm_overlay_gizmos_system,
                    init_bus_route,
                    move_bus_system,
                ),
            );
        info!("done loading: GeoJsonPlugin");
    }
}

// ----------------------------------------------------
// Assets definition
// ----------------------------------------------------

#[derive(Asset, TypePath, Debug, Clone)]
pub struct GeoJsonTextAsset {
    pub text: String,
}

#[derive(Default, TypePath)]
pub struct GeoJsonTextAssetLoader;

impl AssetLoader for GeoJsonTextAssetLoader {
    type Asset = GeoJsonTextAsset;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let text = String::from_utf8(bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(GeoJsonTextAsset { text })
    }

    fn extensions(&self) -> &[&str] {
        &["txt", "geojson"]
    }
}

// ----------------------------------------------------
// Coordinates projection math
// ----------------------------------------------------

#[derive(Resource, Debug, Clone)]
pub struct GeoJsonCoordinatesResource {
    pub ref_point: Vec3,
    pub rot_matrix: [Vec3; 3],
}

fn get_enu_rotation_matrix(ref_point: Vec3) -> [Vec3; 3] {
    let rx = ref_point.x as f64;
    let ry = ref_point.y as f64;
    let rz = ref_point.z as f64;
    let l = (rx * rx + ry * ry + rz * rz).sqrt();
    if l == 0.0 {
        return [Vec3::X, Vec3::Y, Vec3::Z];
    }
    let u = Vec3::new((rx / l) as f32, (ry / l) as f32, (rz / l) as f32);

    let xy_len = (rx * rx + ry * ry).sqrt();
    let e = if xy_len > 0.0 {
        Vec3::new((-ry / xy_len) as f32, (rx / xy_len) as f32, 0.0)
    } else {
        Vec3::new(1.0, 0.0, 0.0)
    };

    let n = u.cross(e);
    [e, n, u]
}

fn lat_lon_to_ecef(lat_deg: f32, lon_deg: f32) -> Vec3 {
    let lat = (lat_deg as f64).to_radians();
    let lon = (lon_deg as f64).to_radians();
    let a = 6378137.0;
    let e2 = 0.00669437999014;
    let n = a / (1.0 - e2 * lat.sin().powi(2)).sqrt();
    let x = n * lat.cos() * lon.cos();
    let y = n * lat.cos() * lon.sin();
    let z = n * (1.0 - e2) * lat.sin();
    Vec3::new(x as f32, y as f32, z as f32)
}

fn lat_lon_to_bevy(lat_deg: f32, lon_deg: f32, ref_point: Vec3, rot_matrix: &[Vec3; 3]) -> Vec3 {
    let pt_ecef = lat_lon_to_ecef(lat_deg, lon_deg);
    let rel_ecef = pt_ecef - ref_point;
    let east = rel_ecef.dot(rot_matrix[0]);
    let north = rel_ecef.dot(rot_matrix[1]);
    let up = rel_ecef.dot(rot_matrix[2]);

    // Bevy X is East, Bevy Y is Up, Bevy Z is -North (North is -Z)
    Vec3::new(east, up, -north)
}

fn parse_bbox_from_txt(text: &str) -> Option<(f32, f32)> {
    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    if lines.len() != 2 {
        return None;
    }
    let p1: Vec<f32> = lines[0]
        .split(',')
        .map(|s| s.trim().parse::<f32>().ok())
        .flatten()
        .collect();
    let p2: Vec<f32> = lines[1]
        .split(',')
        .map(|s| s.trim().parse::<f32>().ok())
        .flatten()
        .collect();
    if p1.len() != 2 || p2.len() != 2 {
        return None;
    }
    let lat_deg = (p1[0] + p2[0]) / 2.0;
    let lon_deg = (p1[1] + p2[1]) / 2.0;
    Some((lat_deg, lon_deg))
}

// ----------------------------------------------------
// Octree geographic bounding box resolution
// ----------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct GeoBBox {
    pub north: f64,
    pub south: f64,
    pub west: f64,
    pub east: f64,
}

impl GeoBBox {
    pub fn contains(&self, lat: f64, lon: f64) -> bool {
        lat >= self.south && lat <= self.north && lon >= self.west && lon <= self.east
    }
}

pub fn octant_path_to_geobbox(path: &str) -> Option<GeoBBox> {
    if path.len() < 2 {
        return None;
    }
    let first_two = &path[0..2];
    let mut box_ = match first_two {
        "02" => GeoBBox {
            north: 0.0,
            south: -90.0,
            west: -180.0,
            east: -90.0,
        },
        "03" => GeoBBox {
            north: 0.0,
            south: -90.0,
            west: -90.0,
            east: 0.0,
        },
        "12" => GeoBBox {
            north: 0.0,
            south: -90.0,
            west: 0.0,
            east: 90.0,
        },
        "13" => GeoBBox {
            north: 0.0,
            south: -90.0,
            west: 90.0,
            east: 180.0,
        },
        "20" => GeoBBox {
            north: 90.0,
            south: 0.0,
            west: -180.0,
            east: -90.0,
        },
        "21" => GeoBBox {
            north: 90.0,
            south: 0.0,
            west: -90.0,
            east: 0.0,
        },
        "30" => GeoBBox {
            north: 90.0,
            south: 0.0,
            west: 0.0,
            east: 90.0,
        },
        "31" => GeoBBox {
            north: 90.0,
            south: 0.0,
            west: 90.0,
            east: 180.0,
        },
        _ => return None,
    };

    for ch in path[2..].chars() {
        let digit = ch.to_digit(10)? as i32;
        let lat_bit = (digit >> 1) & 1; // bit 1
        let lon_bit = digit & 1; // bit 0

        let mid_lat = (box_.north + box_.south) / 2.0;
        let mid_lon = (box_.west + box_.east) / 2.0;

        if lat_bit == 0 {
            box_.north = mid_lat;
        } else {
            box_.south = mid_lat;
        }

        if box_.north == 90.0 || box_.south == -90.0 {
            continue;
        }

        if lon_bit == 0 {
            box_.east = mid_lon;
        } else {
            box_.west = mid_lon;
        }
    }

    Some(box_)
}

// ----------------------------------------------------
// Tile Search & Linear Estimation
// ----------------------------------------------------

fn find_tile_for_lat_lon<'a>(
    lat: f64,
    lon: f64,
    map_tree: &'a crate::plugins::map_plugin::MapTree,
) -> Option<&'a crate::plugins::map_plugin::MapTreeNodeInfo> {
    // Start from the roots
    let matching_roots: Vec<&crate::plugins::map_plugin::MapTreeNodePath> = map_tree
        .roots
        .iter()
        .filter(|node_path| {
            octant_path_to_geobbox(&node_path.0)
                .map(|geobbox| geobbox.contains(lat, lon))
                .unwrap_or(false)
        })
        .collect();

    if matching_roots.is_empty() {
        return None;
    }

    let mut current_node_path = matching_roots[0].clone();

    loop {
        let level = current_node_path.0.len();
        if level >= 20 {
            break;
        }

        let Some(children_set) = map_tree.children.get(&current_node_path) else {
            break;
        };

        if children_set.is_empty() {
            break;
        }

        let matching_children: Vec<&crate::plugins::map_plugin::MapTreeNodePath> = children_set
            .iter()
            .filter(|child_path| {
                octant_path_to_geobbox(&child_path.0)
                    .map(|geobbox| geobbox.contains(lat, lon))
                    .unwrap_or(false)
            })
            .collect();

        if matching_children.is_empty() {
            break;
        } else if matching_children.len() == 1 {
            current_node_path = matching_children[0].clone();
        } else {
            // If there are multiple children that intersect the lat,lon,
            // pick the biggest one by diagonal in xyz bbox, and break the algorithm.
            let mut best_child = None;
            let mut max_diagonal: f32 = -1.0;

            for child_path in matching_children {
                if let Some(node_info) = map_tree.all_nodes.get(child_path) {
                    let diag = (node_info.bbox.max - node_info.bbox.min).length();
                    if diag > max_diagonal {
                        max_diagonal = diag;
                        best_child = Some(child_path);
                    }
                }
            }

            if let Some(child) = best_child {
                current_node_path = child.clone();
            }
            break;
        }
    }

    map_tree.all_nodes.get(&current_node_path)
}

fn project_point(
    lat: f64,
    lon: f64,
    map_tree: &crate::plugins::map_plugin::MapTree,
    coord_res: &GeoJsonCoordinatesResource,
) -> Vec3 {
    if let Some(node_info) = find_tile_for_lat_lon(lat, lon, map_tree) {
        if let Some(geobbox) = octant_path_to_geobbox(&node_info.path.0) {
            let width = geobbox.east - geobbox.west;
            let height = geobbox.north - geobbox.south;
            if width > 0.0 && height > 0.0 {
                let u = (lon - geobbox.west) / width;
                let v = (lat - geobbox.south) / height;

                let x =
                    node_info.bbox.min.x + u as f32 * (node_info.bbox.max.x - node_info.bbox.min.x);
                let z =
                    node_info.bbox.max.z - v as f32 * (node_info.bbox.max.z - node_info.bbox.min.z);
                let y = node_info.bbox.min.y + 2.0; // Slightly above bottom to prevent ground clipping
                return Vec3::new(x, y, z);
            }
        }
    }

    // Fallback: mathematical ENU projection relative to reference point
    lat_lon_to_bevy(
        lat as f32,
        lon as f32,
        coord_res.ref_point,
        &coord_res.rot_matrix,
    )
}

// ----------------------------------------------------
// Ground Raycasting helper
// ----------------------------------------------------

fn query_point_ground_y(
    x: f32,
    z: f32,
    map_tree: &crate::plugins::map_plugin::MapTree,
    spatial_query: &avian3d::prelude::SpatialQuery,
) -> f32 {
    let start_y = map_tree.bbox.max.y + 10.0;
    let ray_origin = Vec3::new(x, start_y, z);
    let ray_dir = Dir3::NEG_Y;
    let max_dist = (map_tree.bbox.max.y - map_tree.bbox.min.y) + 20.0;

    if let Some(hit) = spatial_query.cast_ray(
        ray_origin,
        ray_dir,
        max_dist,
        true,
        &avian3d::prelude::SpatialQueryFilter::default(),
    ) {
        start_y - hit.distance
    } else {
        map_tree.bbox.min.y
    }
}

// ----------------------------------------------------
// Database and States
// ----------------------------------------------------

#[derive(Debug, Clone)]
pub enum RawFeatureGeometry {
    Point((f64, f64)), // (lat, lon)
    LineString(Vec<(f64, f64)>),
    MultiLineString(Vec<Vec<(f64, f64)>>),
    Polygon(Vec<Vec<(f64, f64)>>),
}

#[derive(Debug, Clone)]
pub struct RawGeoJsonFeature {
    pub id: Option<i64>,
    pub osm_type: String,
    pub name: Option<String>,
    pub tags: BTreeMap<String, String>,
    pub raw_geometry: RawFeatureGeometry,
}

#[derive(Debug, Clone)]
pub enum FeatureGeometry {
    Point(Vec3),
    LineString(Vec<Vec3>),
    MultiLineString(Vec<Vec<Vec3>>),
    Polygon(Vec<Vec<Vec3>>),
}

#[derive(Debug, Clone)]
pub struct GeoJsonFeature {
    pub id: Option<i64>,
    pub osm_type: String,
    pub name: Option<String>,
    pub tags: BTreeMap<String, String>,
    pub geometry: FeatureGeometry,
    pub center: Vec3,
    pub bbox_min: Vec3,
    pub bbox_max: Vec3,
}

#[derive(Resource, Default)]
pub struct GeoJsonDatabase {
    pub categories: BTreeMap<String, Vec<GeoJsonFeature>>,
    pub parsed: bool,

    // Staging structure for loaded raw files
    pub raw_categories: BTreeMap<String, Vec<RawGeoJsonFeature>>,
    pub files_loaded: bool,
}

#[derive(Resource, Default)]
pub struct GeoJsonSearchState {
    pub query: String,
}

#[derive(Resource, Default)]
pub struct GeoJsonSelection {
    pub selected: Option<(String, usize)>, // (category_name, feature_index)
}

#[derive(Resource)]
pub struct GeoJsonHandles {
    pub bbox: Handle<GeoJsonTextAsset>,
    pub list: Handle<GeoJsonTextAsset>,
}

#[derive(Resource, Default, PartialEq, Eq)]
pub enum GeoJsonLoaderState {
    #[default]
    Idle,
    LoadingFiles {
        files: Vec<(String, Handle<GeoJsonTextAsset>)>,
    },
    Staged,
}

#[derive(Resource, Debug, Default)]
pub struct GameLoadingStatus {
    pub map_loaded: bool,
    pub geojson_loaded: bool,
    pub geojson_loading_started: bool,
}

#[derive(Resource, Debug, Default)]
pub struct TooltipNotificationState {
    pub map_loaded_timer: f32,
    pub geojson_loaded_timer: f32,
}

// ----------------------------------------------------
// Loading and Staging Systems
// ----------------------------------------------------

fn trigger_geojson_loading(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut loading_status: ResMut<GameLoadingStatus>,
    current_state: Res<State<OsmDatabaseLoadFinished>>,
) {
    if current_state.get() == &OsmDatabaseLoadFinished::MapFinished && !loading_status.geojson_loading_started {
        let bbox_url = format!(
            "{}3d_data_v2/data_in/zone-bbox.txt",
            crate::config::DATA_BASE_URL
        );
        let list_url = format!(
            "{}3d_data_v2/data_osm/_list.txt",
            crate::config::DATA_BASE_URL
        );

        info!(
            "Starting parallel GeoJSON load: bbox from {}, list from {}",
            bbox_url, list_url
        );

        let bbox_handle = asset_server.load(bbox_url);
        let list_handle = asset_server.load(list_url);

        commands.insert_resource(GeoJsonHandles {
            bbox: bbox_handle,
            list: list_handle,
        });

        loading_status.geojson_loading_started = true;
    }
}

fn update_geojson_loading_finished(
    database: Res<GeoJsonDatabase>,
    mut loading_status: ResMut<GameLoadingStatus>,
    mut tooltip_state: ResMut<TooltipNotificationState>,
    mut next_state: ResMut<NextState<OsmDatabaseLoadFinished>>
) {
    if database.parsed && !loading_status.geojson_loaded {
        loading_status.geojson_loaded = true;
        tooltip_state.geojson_loaded_timer = 3.0;
        next_state.set(OsmDatabaseLoadFinished::OsmFinished);
        info!("GeoJSON loading is fully completed!");
    }
}

fn update_tooltip_timers(time: Res<Time>, mut tooltip_state: ResMut<TooltipNotificationState>) {
    let dt = time.delta_secs();
    if tooltip_state.map_loaded_timer > 0.0 {
        tooltip_state.map_loaded_timer = (tooltip_state.map_loaded_timer - dt).max(0.0);
    }
    if tooltip_state.geojson_loaded_timer > 0.0 {
        tooltip_state.geojson_loaded_timer = (tooltip_state.geojson_loaded_timer - dt).max(0.0);
    }
}

fn check_geojson_loading(
    mut commands: Commands,
    handles: Option<Res<GeoJsonHandles>>,
    mut text_assets: ResMut<Assets<GeoJsonTextAsset>>,
    mut loader_state: ResMut<GeoJsonLoaderState>,
    mut database: ResMut<GeoJsonDatabase>,
    coord_res: Option<Res<GeoJsonCoordinatesResource>>,
    asset_server: Res<AssetServer>,
) {
    let Some(handles) = handles else {
        return;
    };

    match &*loader_state {
        GeoJsonLoaderState::Idle => {
            // First wait for coordinate ref point to load and parse
            if coord_res.is_none() {
                if let Some(bbox_asset) = text_assets.get(&handles.bbox) {
                    if let Some((lat, lon)) = parse_bbox_from_txt(&bbox_asset.text) {
                        let ref_point = lat_lon_to_ecef(lat, lon);
                        let rot_matrix = get_enu_rotation_matrix(ref_point);
                        commands.insert_resource(GeoJsonCoordinatesResource {
                            ref_point,
                            rot_matrix,
                        });
                        info!(
                            "GeoJSON reference point initialized at Lat: {}, Lon: {}",
                            lat, lon
                        );
                    } else {
                        error!("Failed to parse zone-bbox.txt");
                        commands.remove_resource::<GeoJsonHandles>();
                        return;
                    }
                } else {
                    return; // Wait for bbox to load
                }
            }

            // Once coordinates are ready, check if list is loaded
            let Some(_coord) = coord_res.as_ref() else {
                return;
            };

            if let Some(list_asset) = text_assets.get(&handles.list) {
                let lines: Vec<&str> = list_asset
                    .text
                    .lines()
                    .map(|l| l.trim())
                    .filter(|l| !l.is_empty())
                    .collect();

                let mut files = Vec::new();
                for line in lines {
                    let file_url = format!(
                        "{}3d_data_v2/data_osm/{}",
                        crate::config::DATA_BASE_URL,
                        line
                    );
                    info!("Loading GeoJSON file: {}", file_url);
                    let handle = asset_server.load(file_url);
                    let category_name = line.replace(".geojson", "");
                    files.push((category_name, handle));
                }

                *loader_state = GeoJsonLoaderState::LoadingFiles { files };
            }
        }

        GeoJsonLoaderState::LoadingFiles { files } => {
            // Check if all files have loaded
            for (_, handle) in files {
                if text_assets.get(handle).is_none() {
                    return; // Wait for all files to be loaded
                }
            }

            info!("All GeoJSON files loaded! Parsing raw features into staging...");

            for (category_name, handle) in files {
                let text_asset = text_assets.remove(handle).unwrap();

                let parsed_json: serde_json::Value = match serde_json::from_str(&text_asset.text) {
                    Ok(val) => val,
                    Err(e) => {
                        error!("Failed to parse JSON for {}: {:?}", category_name, e);
                        continue;
                    }
                };

                let mut category_raw_features = Vec::new();
                if let Some(features_arr) = parsed_json.get("features").and_then(|v| v.as_array()) {
                    for feat_val in features_arr {
                        if let Some(raw_feat) = parse_raw_geojson_feature(feat_val) {
                            category_raw_features.push(raw_feat);
                        }
                    }
                }

                if category_raw_features.is_empty() {
                    info!(
                        "Staging: Category '{}' has 0 features. Skipping to optimize memory.",
                        category_name
                    );
                } else {
                    info!(
                        "Staged {} raw features for category '{}'",
                        category_raw_features.len(),
                        category_name
                    );
                    database
                        .raw_categories
                        .insert(category_name.clone(), category_raw_features);
                }
            }

            database.files_loaded = true;
            *loader_state = GeoJsonLoaderState::Staged;
            commands.remove_resource::<GeoJsonHandles>();
            info!("GeoJSON staging complete. Awaiting MapTree to project coordinates.");
        }

        GeoJsonLoaderState::Staged => {}
    }
}

fn parse_raw_geojson_feature(val: &serde_json::Value) -> Option<RawGeoJsonFeature> {
    let feature_obj = val.as_object()?;
    let properties = feature_obj.get("properties")?.as_object()?;
    let geometry_obj = feature_obj.get("geometry")?.as_object()?;

    let mut tags = BTreeMap::new();
    for (k, v) in properties {
        if k != "tags" && k != "nodes" {
            if let Some(s) = v.as_str() {
                tags.insert(k.clone(), s.to_string());
            } else if let Some(n) = v.as_f64() {
                tags.insert(k.clone(), n.to_string());
            } else if let Some(i) = v.as_i64() {
                tags.insert(k.clone(), i.to_string());
            } else if let Some(b) = v.as_bool() {
                tags.insert(k.clone(), b.to_string());
            }
        }
    }

    if let Some(tags_val) = properties.get("tags").and_then(|t| t.as_object()) {
        for (k, v) in tags_val {
            if let Some(s) = v.as_str() {
                tags.insert(k.clone(), s.to_string());
            } else if let Some(n) = v.as_f64() {
                tags.insert(k.clone(), n.to_string());
            } else if let Some(i) = v.as_i64() {
                tags.insert(k.clone(), i.to_string());
            } else if let Some(b) = v.as_bool() {
                tags.insert(k.clone(), b.to_string());
            }
        }
    }

    let name = tags.get("name").cloned();
    let id = properties
        .get("id")
        .and_then(|v| v.as_i64())
        .or_else(|| tags.get("id").and_then(|s| s.parse::<i64>().ok()));
    let osm_type = properties
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("node")
        .to_string();

    let geom_type = geometry_obj.get("type")?.as_str()?;
    let coords = geometry_obj.get("coordinates")?;

    let raw_geometry = match geom_type {
        "Point" => {
            let arr = coords.as_array()?;
            let lon = arr.get(0)?.as_f64()?;
            let lat = arr.get(1)?.as_f64()?;
            RawFeatureGeometry::Point((lat, lon))
        }
        "LineString" => {
            let arr = coords.as_array()?;
            let mut pts = Vec::new();
            for pt_val in arr {
                let pt_arr = pt_val.as_array()?;
                let lon = pt_arr.get(0)?.as_f64()?;
                let lat = pt_arr.get(1)?.as_f64()?;
                pts.push((lat, lon));
            }
            RawFeatureGeometry::LineString(pts)
        }
        "MultiLineString" => {
            let arr = coords.as_array()?;
            let mut lines = Vec::new();
            for line_val in arr {
                let line_arr = line_val.as_array()?;
                let mut line = Vec::new();
                for pt_val in line_arr {
                    let pt_arr = pt_val.as_array()?;
                    let lon = pt_arr.get(0)?.as_f64()?;
                    let lat = pt_arr.get(1)?.as_f64()?;
                    line.push((lat, lon));
                }
                lines.push(line);
            }
            RawFeatureGeometry::MultiLineString(lines)
        }
        "Polygon" => {
            let arr = coords.as_array()?;
            let mut rings = Vec::new();
            for ring_val in arr {
                let ring_arr = ring_val.as_array()?;
                let mut ring = Vec::new();
                for pt_val in ring_arr {
                    let pt_arr = pt_val.as_array()?;
                    let lon = pt_arr.get(0)?.as_f64()?;
                    let lat = pt_arr.get(1)?.as_f64()?;
                    ring.push((lat, lon));
                }
                rings.push(ring);
            }
            RawFeatureGeometry::Polygon(rings)
        }
        _ => return None,
    };

    Some(RawGeoJsonFeature {
        id,
        osm_type,
        name,
        tags,
        raw_geometry,
    })
}

// ----------------------------------------------------
// Post-Processing Projection System
// ----------------------------------------------------

fn project_geojson_coordinates(
    mut database: ResMut<GeoJsonDatabase>,
    map_tree: Res<crate::plugins::map_plugin::MapTree>,
    coord_res: Option<Res<GeoJsonCoordinatesResource>>,
) {
    if !database.files_loaded || database.parsed || !map_tree.parsed {
        return;
    }

    let Some(coord) = coord_res else {
        return;
    };

    info!("Projecting GeoJSON coordinates utilizing MapTree metadata...");

    let raw_categories = std::mem::take(&mut database.raw_categories);

    for (category_name, raw_features) in raw_categories {
        let mut projected_features = Vec::new();

        for raw in raw_features {
            let geometry = match &raw.raw_geometry {
                RawFeatureGeometry::Point((lat, lon)) => {
                    if find_tile_for_lat_lon(*lat, *lon, &map_tree).is_none() {
                        continue;
                    }
                    FeatureGeometry::Point(project_point(*lat, *lon, &map_tree, &coord))
                }
                RawFeatureGeometry::LineString(pts) => {
                    let projected_pts: Vec<Vec3> = pts
                        .iter()
                        .filter(|&&(lat, lon)| find_tile_for_lat_lon(lat, lon, &map_tree).is_some())
                        .map(|&(lat, lon)| project_point(lat, lon, &map_tree, &coord))
                        .collect();
                    if projected_pts.len() < 2 {
                        continue;
                    }
                    FeatureGeometry::LineString(projected_pts)
                }
                RawFeatureGeometry::MultiLineString(lines) => {
                    let mut projected_lines = Vec::new();
                    for pts in lines {
                        let projected_pts: Vec<Vec3> = pts
                            .iter()
                            .filter(|&&(lat, lon)| find_tile_for_lat_lon(lat, lon, &map_tree).is_some())
                            .map(|&(lat, lon)| project_point(lat, lon, &map_tree, &coord))
                            .collect();
                        if projected_pts.len() >= 2 {
                            projected_lines.push(projected_pts);
                        }
                    }
                    if projected_lines.is_empty() {
                        continue;
                    }
                    FeatureGeometry::MultiLineString(projected_lines)
                }
                RawFeatureGeometry::Polygon(rings) => {
                    let mut projected_rings = Vec::new();
                    for ring in rings {
                        let projected_ring: Vec<Vec3> = ring
                            .iter()
                            .filter(|&&(lat, lon)| find_tile_for_lat_lon(lat, lon, &map_tree).is_some())
                            .map(|&(lat, lon)| project_point(lat, lon, &map_tree, &coord))
                            .collect();
                        if projected_ring.len() >= 3 {
                            projected_rings.push(projected_ring);
                        }
                    }
                    if projected_rings.is_empty() {
                        continue;
                    }
                    FeatureGeometry::Polygon(projected_rings)
                }
            };

            let (bbox_min, bbox_max) = match &geometry {
                FeatureGeometry::Point(p) => (*p, *p),
                FeatureGeometry::LineString(pts) => {
                    if pts.is_empty() {
                        (Vec3::ZERO, Vec3::ZERO)
                    } else {
                        let mut min = pts[0];
                        let mut max = pts[0];
                        for p in pts {
                            min = min.min(*p);
                            max = max.max(*p);
                        }
                        (min, max)
                    }
                }
                FeatureGeometry::MultiLineString(lines) => {
                    if lines.is_empty() || lines[0].is_empty() {
                        (Vec3::ZERO, Vec3::ZERO)
                    } else {
                        let mut min = lines[0][0];
                        let mut max = lines[0][0];
                        for line in lines {
                            for p in line {
                                min = min.min(*p);
                                max = max.max(*p);
                            }
                        }
                        (min, max)
                    }
                }
                FeatureGeometry::Polygon(rings) => {
                    if rings.is_empty() || rings[0].is_empty() {
                        (Vec3::ZERO, Vec3::ZERO)
                    } else {
                        let mut min = rings[0][0];
                        let mut max = rings[0][0];
                        for ring in rings {
                            for p in ring {
                                min = min.min(*p);
                                max = max.max(*p);
                            }
                        }
                        (min, max)
                    }
                }
            };
            let center = (bbox_min + bbox_max) / 2.0;

            projected_features.push(GeoJsonFeature {
                id: raw.id,
                osm_type: raw.osm_type,
                name: raw.name,
                tags: raw.tags,
                geometry,
                center,
                bbox_min,
                bbox_max,
            });
        }

        info!(
            "Projected {} features for category '{}'",
            projected_features.len(),
            category_name
        );
        database
            .categories
            .insert(category_name, projected_features);
    }

    database.parsed = true;
    info!("All GeoJSON category features projected successfully!");
}

// ----------------------------------------------------
// egui UI System
// ----------------------------------------------------

fn geojson_ui_system(
    mut contexts: EguiContexts,
    database: Res<GeoJsonDatabase>,
    mut search_state: ResMut<GeoJsonSearchState>,
    mut selection: ResMut<GeoJsonSelection>,
    mut commands: Commands,
    camera_query: Query<&Transform, With<Camera>>,
    ui_state: Option<ResMut<crate::ui_egui::UiState>>,
) {
    let Some(mut state) = ui_state else {
        return;
    };
    if !state.show_geojson_database {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    egui::Window::new("GeoJSON Database")
        .open(&mut state.show_geojson_database)
        .show(ctx, |ui| {
            if !database.parsed {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Staging & projecting coordinates from MapTree...");
                });
                return;
            }

            ui.horizontal(|ui| {
                ui.label("Search:");
                ui.text_edit_singleline(&mut search_state.query);
            });
            ui.separator();

            let query_trimmed = search_state.query.trim();

            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    if query_trimmed.len() <= 3 {
                        // Display first 10 items of each type
                        for (cat_name, features) in &database.categories {
                            ui.collapsing(format!("{} ({})", cat_name, features.len()), |ui| {
                                let display_count = features.len().min(10);
                                for idx in 0..display_count {
                                    let feature = &features[idx];
                                    let display_name = feature.name.clone().unwrap_or_else(|| {
                                        format!(
                                            "{} #{}",
                                            feature.osm_type,
                                            feature.id.unwrap_or(idx as i64)
                                        )
                                    });

                                    let is_selected = selection.selected.as_ref()
                                        == Some(&(cat_name.clone(), idx));
                                    if ui.selectable_label(is_selected, &display_name).clicked() {
                                        select_and_animate(
                                            cat_name.clone(),
                                            idx,
                                            feature,
                                            &mut commands,
                                            &camera_query,
                                        );
                                        selection.selected = Some((cat_name.clone(), idx));
                                    }
                                }
                            });
                        }
                    } else {
                        // Text search (case-insensitive) across names
                        let mut matches = Vec::new();
                        let query_lower = query_trimmed.to_lowercase();

                        'outer: for (cat_name, features) in &database.categories {
                            for (idx, feature) in features.iter().enumerate() {
                                if let Some(name) = &feature.name {
                                    if name.to_lowercase().contains(&query_lower) {
                                        matches.push((cat_name.clone(), idx, feature));
                                        if matches.len() >= 200 {
                                            break 'outer;
                                        }
                                    }
                                }
                            }
                        }

                        ui.label(format!("Found {} matches (max 200)", matches.len()));
                        for (cat_name, idx, feature) in matches {
                            let display_name = format!(
                                "[{}] {}",
                                cat_name,
                                feature.name.clone().unwrap_or_default()
                            );
                            let is_selected =
                                selection.selected.as_ref() == Some(&(cat_name.clone(), idx));
                            if ui.selectable_label(is_selected, &display_name).clicked() {
                                select_and_animate(
                                    cat_name.clone(),
                                    idx,
                                    feature,
                                    &mut commands,
                                    &camera_query,
                                );
                                selection.selected = Some((cat_name.clone(), idx));
                            }
                        }
                    }
                });

            // Detail panel for selected element
            if let Some((cat_name, idx)) = &selection.selected {
                if let Some(features) = database.categories.get(cat_name) {
                    if let Some(feature) = features.get(*idx) {
                        ui.separator();
                        ui.heading("Selected Feature Info");
                        ui.label(format!("Category: {}", cat_name));
                        ui.label(format!("OSM Type: {}", feature.osm_type));
                        ui.label(format!("ID: {}", feature.id.unwrap_or(0)));
                        if let Some(name) = &feature.name {
                            ui.label(format!("Name: {}", name));
                        }

                        ui.collapsing("All Tags", |ui| {
                            for (k, v) in &feature.tags {
                                ui.label(format!("{}: {}", k, v));
                            }
                        });
                    }
                }
            }
        });
}

fn select_and_animate(
    cat_name: String,
    idx: usize,
    feature: &GeoJsonFeature,
    commands: &mut Commands,
    camera_query: &Query<&Transform, With<Camera>>,
) {
    let Some(camera_transform) = camera_query.iter().next() else {
        return;
    };

    let start_pos = camera_transform.translation;
    let start_rot = camera_transform.rotation;

    let target_pos;
    let target_rot;

    match &feature.geometry {
        FeatureGeometry::Point(p) => {
            // Height 100, horizontal distance 200 along South direction (+Z)
            target_pos = Vec3::new(p.x, p.y + 100.0, p.z + 200.0);
            target_rot = Transform::from_translation(target_pos)
                .looking_at(*p, Vec3::Y)
                .rotation;
        }
        FeatureGeometry::LineString(_) | FeatureGeometry::MultiLineString(_) | FeatureGeometry::Polygon(_) => {
            let center = feature.center;
            let size = feature.bbox_max - feature.bbox_min;
            let dist = size.x.max(size.z).max(10.0) * 1.5;

            target_pos = Vec3::new(center.x, center.y + dist, center.z + dist * 0.5);
            target_rot = Transform::from_translation(target_pos)
                .looking_at(center, Vec3::Y)
                .rotation;
        }
    }

    commands.insert_resource(ActiveCameraAnimation {
        start_pos,
        start_rot,
        target_pos,
        target_rot,
        elapsed: 0.0,
        duration: 1.5,
    });

    info!(
        "Animating camera for selection '{}/{}': Target position {:?}",
        cat_name, idx, target_pos
    );
}

// ----------------------------------------------------
// Screen-Space Text Labels rendering system
// ----------------------------------------------------

fn geojson_text_labels_system(
    mut contexts: EguiContexts,
    database: Res<GeoJsonDatabase>,
    selection: Res<GeoJsonSelection>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    map_tree: Res<crate::plugins::map_plugin::MapTree>,
    spatial_query: avian3d::prelude::SpatialQuery,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if !database.parsed {
        return;
    }
    let Some((cat_name, idx)) = &selection.selected else {
        return;
    };
    let Some(features) = database.categories.get(cat_name) else {
        return;
    };
    let Some(feature) = features.get(*idx) else {
        return;
    };
    let Some((camera, camera_transform)) = camera_query.iter().next() else {
        return;
    };

    let raw_name = feature
        .name
        .clone()
        .unwrap_or_else(|| format!("ID: {}", feature.id.unwrap_or(0)));
    let mut name_15 = raw_name.chars().take(15).collect::<String>();
    if raw_name.chars().count() > 15 {
        name_15.push_str("...");
    }

    // Determine target points and label texts
    let mut target_points = Vec::new();
    match &feature.geometry {
        FeatureGeometry::Point(p) => {
            target_points.push((*p, name_15));
        }
        FeatureGeometry::LineString(pts) => {
            for (node_idx, pt) in pts.iter().enumerate() {
                let node_id_name = format!("Node #{}", node_idx);
                target_points.push((*pt, node_id_name));
            }
        }
        FeatureGeometry::MultiLineString(lines) => {
            for (line_idx, pts) in lines.iter().enumerate() {
                for (node_idx, pt) in pts.iter().enumerate() {
                    let node_id_name = format!("L{} Node #{}", line_idx, node_idx);
                    target_points.push((*pt, node_id_name));
                }
            }
        }
        FeatureGeometry::Polygon(_) => {
            target_points.push((feature.center, name_15));
        }
    }

    for (pt, label) in target_points {
        let mut pos = pt;
        pos.y = query_point_ground_y(pos.x, pos.z, &map_tree, &spatial_query) + 0.1;

        if cat_name == "amenities" || cat_name == "shops" {
            pos.y += 50.0;
        }

        if let Ok(p_center) = camera.world_to_viewport(camera_transform, pos) {
            let camera_right = camera_transform.right();
            let sphere_radius = 3.0;
            if let Ok(p_edge) =
                camera.world_to_viewport(camera_transform, pos + camera_right * sphere_radius)
            {
                let r_screen = p_center.distance(p_edge);
                let font_size = (r_screen * 3.0).clamp(11.0, 36.0);

                egui::Area::new(egui::Id::new(format!("lbl_{:?}_{}", pos, label)))
                    .fixed_pos(egui::pos2(p_center.x - 20.0, p_center.y - font_size - 8.0))
                    .show(ctx, |ui| {
                        ui.label(
                            egui::RichText::new(&label)
                                .color(egui::Color32::from_rgb(255, 60, 60))
                                .size(font_size)
                                .strong()
                                .background_color(egui::Color32::from_rgba_premultiplied(
                                    0, 0, 0, 180,
                                )),
                        );
                    });
            }
        }
    }
}

// ----------------------------------------------------
// Gizmos Highlight rendering
// ----------------------------------------------------

fn geojson_gizmos_system(
    mut gizmos: Gizmos,
    database: Res<GeoJsonDatabase>,
    selection: Res<GeoJsonSelection>,
    map_tree: Res<crate::plugins::map_plugin::MapTree>,
    spatial_query: avian3d::prelude::SpatialQuery,
) {
    if !database.parsed {
        return;
    }
    let Some((cat_name, idx)) = &selection.selected else {
        return;
    };
    let Some(features) = database.categories.get(cat_name) else {
        return;
    };
    let Some(feature) = features.get(*idx) else {
        return;
    };

    let red = Color::srgb(1.0, 0.0, 0.0);
    let black = Color::BLACK;

    // Helper for drawing 3D cross star marker of length 30 and black sphere
    let draw_star_marker = |gizmos: &mut Gizmos, center: Vec3| {
        // Draw 3 axes
        gizmos.line(center - Vec3::X * 15.0, center + Vec3::X * 15.0, red);
        gizmos.line(center - Vec3::Y * 15.0, center + Vec3::Y * 15.0, red);
        gizmos.line(center - Vec3::Z * 15.0, center + Vec3::Z * 15.0, red);

        // Draw 4 diagonals
        let d1 = Vec3::new(1.0, 1.0, 1.0).normalize() * 15.0;
        let d2 = Vec3::new(1.0, 1.0, -1.0).normalize() * 15.0;
        let d3 = Vec3::new(1.0, -1.0, 1.0).normalize() * 15.0;
        let d4 = Vec3::new(1.0, -1.0, -1.0).normalize() * 15.0;
        gizmos.line(center - d1, center + d1, red);
        gizmos.line(center - d2, center + d2, red);
        gizmos.line(center - d3, center + d3, red);
        gizmos.line(center - d4, center + d4, red);

        // Draw center black sphere
        let sphere = Sphere::new(3.0);
        gizmos.primitive_3d(&sphere, Isometry3d::from_translation(center), black);
    };

    match &feature.geometry {
        FeatureGeometry::Point(p) => {
            let mut pos = *p;
            pos.y = query_point_ground_y(pos.x, pos.z, &map_tree, &spatial_query) + 0.1;

            if cat_name == "amenities" || cat_name == "shops" {
                // SCI-FI BEACON
                let beacon_top = pos + Vec3::Y * 50.0;
                gizmos.line(pos, beacon_top, Color::srgb(1.0, 0.3, 0.3));
                draw_star_marker(&mut gizmos, beacon_top);
            } else {
                // NORMAL POINT
                draw_star_marker(&mut gizmos, pos);
            }
        }

        FeatureGeometry::LineString(pts) => {
            // Project each point to ground Y
            let mut grounded_pts = Vec::new();
            for pt in pts {
                let mut gp = *pt;
                gp.y = query_point_ground_y(gp.x, gp.z, &map_tree, &spatial_query) + 0.1;
                grounded_pts.push(gp);
            }

            // Draw a little star at each node
            for pt in &grounded_pts {
                draw_star_marker(&mut gizmos, *pt);
            }

            // Connect path nodes with lines and repeat at different Y levels
            if grounded_pts.len() >= 2 {
                let mut min_y = f32::INFINITY;
                let mut max_y = -f32::INFINITY;
                for pt in &grounded_pts {
                    min_y = min_y.min(pt.y);
                    max_y = max_y.max(pt.y);
                }

                if cat_name == "railways" {
                    // RAILWAY CROSS-TIES
                    let steps = 10;
                    for step in 0..=steps {
                        let y_level = if max_y > min_y {
                            min_y + (step as f32 / steps as f32) * (max_y - min_y)
                        } else {
                            min_y
                        };

                        for window in grounded_pts.windows(2) {
                            let p1 = Vec3::new(window[0].x, y_level, window[0].z);
                            let p2 = Vec3::new(window[1].x, y_level, window[1].z);
                            gizmos.line(p1, p2, Color::srgb(0.7, 0.7, 0.7)); // Silver rails

                            // Ties every 10 meters
                            let diff = p2 - p1;
                            let dist = diff.length();
                            let dir = diff.normalize_or_zero();
                            let perp = Vec3::new(-dir.z, 0.0, dir.x).normalize_or_zero();

                            let mut current_dist = 0.0;
                            while current_dist <= dist {
                                let tie_center = p1 + dir * current_dist;
                                gizmos.line(
                                    tie_center - perp * 2.0,
                                    tie_center + perp * 2.0,
                                    Color::srgb(0.5, 0.25, 0.1),
                                );
                                current_dist += 10.0;
                            }
                        }

                        if max_y <= min_y {
                            break;
                        }
                    }
                } else if cat_name == "waterways" {
                    // WATERWAY BANK LINES
                    let steps = 10;
                    for step in 0..=steps {
                        let y_level = if max_y > min_y {
                            min_y + (step as f32 / steps as f32) * (max_y - min_y)
                        } else {
                            min_y
                        };

                        for window in grounded_pts.windows(2) {
                            let p1 = Vec3::new(window[0].x, y_level, window[0].z);
                            let p2 = Vec3::new(window[1].x, y_level, window[1].z);

                            let dir = (p2 - p1).normalize_or_zero();
                            let perp = Vec3::new(-dir.z, 0.0, dir.x).normalize_or_zero();

                            gizmos.line(
                                p1 - perp * 2.0,
                                p2 - perp * 2.0,
                                Color::srgb(0.0, 0.0, 1.0),
                            );
                            gizmos.line(
                                p1 + perp * 2.0,
                                p2 + perp * 2.0,
                                Color::srgb(0.0, 0.0, 1.0),
                            );
                        }

                        if max_y <= min_y {
                            break;
                        }
                    }
                } else {
                    // NORMAL ROAD LINES
                    let steps = 10;
                    for step in 0..=steps {
                        let y_level = if max_y > min_y {
                            min_y + (step as f32 / steps as f32) * (max_y - min_y)
                        } else {
                            min_y
                        };

                        for window in grounded_pts.windows(2) {
                            let p1 = Vec3::new(window[0].x, y_level, window[0].z);
                            let p2 = Vec3::new(window[1].x, y_level, window[1].z);
                            gizmos.line(p1, p2, red);
                        }

                        if max_y <= min_y {
                            break;
                        }
                    }
                }
            }
        }

        FeatureGeometry::MultiLineString(lines) => {
            for pts in lines {
                // Project each point to ground Y
                let mut grounded_pts = Vec::new();
                for pt in pts {
                    let mut gp = *pt;
                    gp.y = query_point_ground_y(gp.x, gp.z, &map_tree, &spatial_query) + 0.1;
                    grounded_pts.push(gp);
                }

                // Draw a little star at each node
                for pt in &grounded_pts {
                    draw_star_marker(&mut gizmos, *pt);
                }

                // Connect path nodes with lines and repeat at different Y levels
                if grounded_pts.len() >= 2 {
                    let mut min_y = f32::INFINITY;
                    let mut max_y = -f32::INFINITY;
                    for pt in &grounded_pts {
                        min_y = min_y.min(pt.y);
                        max_y = max_y.max(pt.y);
                    }

                    let steps = 10;
                    for step in 0..=steps {
                        let y_level = if max_y > min_y {
                            min_y + (step as f32 / steps as f32) * (max_y - min_y)
                        } else {
                            min_y
                        };

                        for window in grounded_pts.windows(2) {
                            let p1 = Vec3::new(window[0].x, y_level, window[0].z);
                            let p2 = Vec3::new(window[1].x, y_level, window[1].z);
                            gizmos.line(p1, p2, red);
                        }

                        if max_y <= min_y {
                            break;
                        }
                    }
                }
            }
        }

        FeatureGeometry::Polygon(rings) => {
            // Draw polygon stack repeated 15 times from min Y to max Y of map bbox
            let min_y = map_tree.bbox.min.y;
            let max_y = map_tree.bbox.max.y;

            let steps = 15;
            for step in 0..steps {
                let t = step as f32 / (steps - 1) as f32;
                let y_level = min_y + t * (max_y - min_y);

                let orange_shade = Color::srgb(1.0, 0.2 + 0.6 * t, 0.0);

                for ring in rings {
                    // Draw outer boundary
                    for window in ring.windows(2) {
                        let p1 = Vec3::new(window[0].x, y_level, window[0].z);
                        let p2 = Vec3::new(window[1].x, y_level, window[1].z);
                        gizmos.line(p1, p2, orange_shade);
                    }
                    if ring.len() >= 3 {
                        let p1 = Vec3::new(ring[ring.len() - 1].x, y_level, ring[ring.len() - 1].z);
                        let p2 = Vec3::new(ring[0].x, y_level, ring[0].z);
                        gizmos.line(p1, p2, orange_shade);
                    }

                    // Surface Diagonals visual improvement
                    if cat_name == "landuse" || cat_name == "leisure" {
                        let len = ring.len();
                        if len >= 4 {
                            for k in 0..15 {
                                let idx1 = (k * 7) % len;
                                let idx2 = (idx1 + len / 2) % len;
                                if idx1 != idx2 {
                                    let p1 = Vec3::new(ring[idx1].x, y_level, ring[idx1].z);
                                    let p2 = Vec3::new(ring[idx2].x, y_level, ring[idx2].z);
                                    gizmos.line(p1, p2, orange_shade);
                                }
                            }
                        }
                    }
                }
            }

            // Buildings 3D extruded wireframe box visual improvement
            if cat_name == "buildings" {
                for ring in rings {
                    let mut ground_pts = Vec::new();
                    let mut roof_pts = Vec::new();

                    for pt in ring {
                        let mut gp = *pt;
                        gp.y = query_point_ground_y(gp.x, gp.z, &map_tree, &spatial_query) + 0.1;
                        ground_pts.push(gp);

                        let mut rp = gp;
                        rp.y += 15.0; // Extruded roof height
                        roof_pts.push(rp);
                    }

                    let count = ground_pts.len();
                    for idx in 0..count {
                        let next_idx = (idx + 1) % count;
                        // Bottom base
                        gizmos.line(
                            ground_pts[idx],
                            ground_pts[next_idx],
                            Color::srgb(1.0, 0.4, 0.0),
                        );
                        // Top roof
                        gizmos.line(
                            roof_pts[idx],
                            roof_pts[next_idx],
                            Color::srgb(1.0, 0.7, 0.0),
                        );
                        // Vertical pillars
                        gizmos.line(ground_pts[idx], roof_pts[idx], Color::srgb(1.0, 0.5, 0.0));
                    }
                }
            }
        }
    }
}

// ----------------------------------------------------
// OSM Overlays Debug Settings
// ----------------------------------------------------

#[derive(Resource, Debug, Clone)]
pub struct OsmOverlayState {
    pub show_window: bool,
    pub show_roads: bool,
    pub show_bus_routes: bool,
    pub show_businesses: bool,
    pub show_railways: bool,
    pub show_waterways: bool,
    pub show_buildings: bool,
}

impl Default for OsmOverlayState {
    fn default() -> Self {
        Self {
            show_window: false,
            show_roads: false,
            show_bus_routes: false,
            show_businesses: false,
            show_railways: false,
            show_waterways: false,
            show_buildings: false,
        }
    }
}

fn osm_overlay_ui_system(
    mut contexts: EguiContexts,
    mut osm_overlay: ResMut<OsmOverlayState>,
) {
    if !osm_overlay.show_window {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let mut show_window = osm_overlay.show_window;
    let mut show_roads = osm_overlay.show_roads;
    let mut show_bus_routes = osm_overlay.show_bus_routes;
    let mut show_businesses = osm_overlay.show_businesses;
    let mut show_railways = osm_overlay.show_railways;
    let mut show_waterways = osm_overlay.show_waterways;
    let mut show_buildings = osm_overlay.show_buildings;

    egui::Window::new("OSM Overlays")
        .open(&mut show_window)
        .show(ctx, |ui| {
            ui.checkbox(&mut show_roads, "Show Roads (Streets)");
            ui.checkbox(&mut show_bus_routes, "Show Bus Routes");
            ui.checkbox(&mut show_businesses, "Show Businesses (Shops/Amenities/Offices/Craft)");
            ui.checkbox(&mut show_railways, "Show Railways");
            ui.checkbox(&mut show_waterways, "Show Waterways");
            ui.checkbox(&mut show_buildings, "Show Buildings");
        });

    osm_overlay.show_window = show_window;
    osm_overlay.show_roads = show_roads;
    osm_overlay.show_bus_routes = show_bus_routes;
    osm_overlay.show_businesses = show_businesses;
    osm_overlay.show_railways = show_railways;
    osm_overlay.show_waterways = show_waterways;
    osm_overlay.show_buildings = show_buildings;
}

fn osm_overlay_gizmos_system(
    mut gizmos: Gizmos,
    database: Res<GeoJsonDatabase>,
    osm_overlay: Res<OsmOverlayState>,
    map_tree: Res<crate::plugins::map_plugin::MapTree>,
    spatial_query: avian3d::prelude::SpatialQuery,
) {
    if !database.parsed {
        return;
    }

    let road_color = Color::srgb(0.9, 0.9, 0.9);
    let bus_route_color = Color::srgb(0.1, 0.4, 0.9);
    let business_color = Color::srgb(0.9, 0.1, 0.6);
    let railway_color = Color::srgb(0.5, 0.5, 0.5);
    let waterway_color = Color::srgb(0.0, 0.3, 0.9);
    let building_color = Color::srgb(1.0, 0.5, 0.0);

    let query_y = |x: f32, z: f32| -> f32 {
        query_point_ground_y(x, z, &map_tree, &spatial_query) + 0.2
    };

    let draw_lines = |gizmos: &mut Gizmos, pts: &[Vec3], color: Color| {
        if pts.len() < 2 { return; }
        let mut grounded = Vec::with_capacity(pts.len());
        for pt in pts {
            grounded.push(Vec3::new(pt.x, query_y(pt.x, pt.z), pt.z));
        }
        for window in grounded.windows(2) {
            gizmos.line(window[0], window[1], color);
        }
    };

    let draw_polygon = |gizmos: &mut Gizmos, rings: &[Vec<Vec3>], color: Color| {
        for ring in rings {
            if ring.is_empty() { continue; }
            let mut grounded = Vec::with_capacity(ring.len());
            for pt in ring {
                grounded.push(Vec3::new(pt.x, query_y(pt.x, pt.z), pt.z));
            }
            for window in grounded.windows(2) {
                gizmos.line(window[0], window[1], color);
            }
            if grounded.len() >= 3 {
                gizmos.line(grounded[grounded.len() - 1], grounded[0], color);
            }
        }
    };

    if osm_overlay.show_roads {
        if let Some(features) = database.categories.get("roads") {
            for feature in features {
                match &feature.geometry {
                    FeatureGeometry::LineString(pts) => {
                        draw_lines(&mut gizmos, pts, road_color);
                    }
                    FeatureGeometry::MultiLineString(lines) => {
                        for pts in lines {
                            draw_lines(&mut gizmos, pts, road_color);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    if osm_overlay.show_bus_routes {
        if let Some(features) = database.categories.get("routes") {
            for feature in features {
                let is_bus = feature.tags.get("route").map(|r| r == "bus").unwrap_or(false);
                if !is_bus { continue; }

                match &feature.geometry {
                    FeatureGeometry::LineString(pts) => {
                        draw_lines(&mut gizmos, pts, bus_route_color);
                    }
                    FeatureGeometry::MultiLineString(lines) => {
                        for pts in lines {
                            draw_lines(&mut gizmos, pts, bus_route_color);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    if osm_overlay.show_businesses {
        for cat in &["shops", "amenities", "offices", "craft"] {
            if let Some(features) = database.categories.get(*cat) {
                for feature in features {
                    let mut pos = feature.center;
                    pos.y = query_y(pos.x, pos.z);
                    let sphere = Sphere::new(1.0);
                    gizmos.primitive_3d(&sphere, Isometry3d::from_translation(pos), business_color);
                }
            }
        }
    }

    if osm_overlay.show_railways {
        if let Some(features) = database.categories.get("railways") {
            for feature in features {
                match &feature.geometry {
                    FeatureGeometry::LineString(pts) => {
                        draw_lines(&mut gizmos, pts, railway_color);
                    }
                    FeatureGeometry::MultiLineString(lines) => {
                        for pts in lines {
                            draw_lines(&mut gizmos, pts, railway_color);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    if osm_overlay.show_waterways {
        if let Some(features) = database.categories.get("waterways") {
            for feature in features {
                match &feature.geometry {
                    FeatureGeometry::LineString(pts) => {
                        draw_lines(&mut gizmos, pts, waterway_color);
                    }
                    FeatureGeometry::MultiLineString(lines) => {
                        for pts in lines {
                            draw_lines(&mut gizmos, pts, waterway_color);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    if osm_overlay.show_buildings {
        if let Some(features) = database.categories.get("buildings") {
            for feature in features {
                match &feature.geometry {
                    FeatureGeometry::Polygon(rings) => {
                        draw_polygon(&mut gizmos, rings, building_color);
                    }
                    _ => {}
                }
            }
        }
    }
}

// ----------------------------------------------------
// Bus 335 Path Following Animation
// ----------------------------------------------------

#[derive(Component)]
pub struct Bus335Marker;

#[derive(Component)]
pub struct MovingBus {
    pub points: Vec<Vec3>,
    pub current_index: usize,
    pub speed: f32,
}

fn init_bus_route(
    mut commands: Commands,
    database: Res<GeoJsonDatabase>,
    current_state: Res<State<OsmDatabaseLoadFinished>>,
    query: Query<(Entity, &Transform), (With<Bus335Marker>, Without<MovingBus>)>,
) {
    if current_state.get() != &OsmDatabaseLoadFinished::OsmFinished {
        return;
    }
    let Some((entity, _transform)) = query.iter().next() else {
        return;
    };

    let mut bus_route_points = Vec::new();
    if let Some(features) = database.categories.get("routes") {
        for feature in features {
            if feature.tags.get("ref").map(|s| s == "335").unwrap_or(false) {
                match &feature.geometry {
                    FeatureGeometry::LineString(pts) => {
                        if pts.len() > bus_route_points.len() {
                            bus_route_points = pts.clone();
                        }
                    }
                    FeatureGeometry::MultiLineString(lines) => {
                        for line in lines {
                            if line.len() > bus_route_points.len() {
                                bus_route_points = line.clone();
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    if !bus_route_points.is_empty() {
        info!("Successfully found Bus 335 route with {} points! Initializing movement.", bus_route_points.len());
        commands.entity(entity).insert(MovingBus {
            points: bus_route_points,
            current_index: 0,
            speed: 25.0,
        });
    } else {
        warn!("Failed to find Bus 335 route in loaded geojson databases!");
    }
}

fn move_bus_system(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &mut MovingBus)>,
    map_tree: Res<crate::plugins::map_plugin::MapTree>,
    spatial_query: avian3d::prelude::SpatialQuery,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    for (mut transform, mut bus) in &mut query {
        if bus.points.is_empty() {
            continue;
        }
        let mut target = bus.points[bus.current_index];
        target.y = query_point_ground_y(target.x, target.z, &map_tree, &spatial_query) + 1.4;

        let diff = target - transform.translation;
        let dist = diff.length();

        let step = bus.speed * dt;
        if dist <= step {
            transform.translation = target;
            bus.current_index = (bus.current_index + 1) % bus.points.len();
        } else {
            let direction = diff / dist;
            transform.translation += direction * step;

            if direction.length_squared() > 0.001 {
                let look_target = transform.translation + direction;
                let target_rotation = Transform::from_translation(transform.translation)
                    .looking_at(look_target, Vec3::Y)
                    .rotation;
                transform.rotation = transform.rotation.lerp(target_rotation, 5.0 * dt);
            }
        }
    }
}
