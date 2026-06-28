use bevy::prelude::*;
use bevy::asset::{Asset, AssetLoader, LoadContext, io::Reader};
use bevy::reflect::TypePath;
use bevy_egui::{EguiContexts, egui};
use bevy_egui::EguiPrimaryContextPass;
use std::collections::BTreeMap;

use crate::plugins::camera_controls::ActiveCameraAnimation;

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
            .add_systems(Startup, setup_geojson_loading)
            .add_systems(EguiPrimaryContextPass, geojson_ui_system)
            .add_systems(
                Update,
                (
                    check_geojson_loading,
                    project_geojson_coordinates,
                    geojson_gizmos_system,
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
        let text = String::from_utf8(bytes).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?;
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
    let lines: Vec<&str> = text.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    if lines.len() != 2 {
        return None;
    }
    let p1: Vec<f32> = lines[0].split(',').map(|s| s.trim().parse::<f32>().ok()).flatten().collect();
    let p2: Vec<f32> = lines[1].split(',').map(|s| s.trim().parse::<f32>().ok()).flatten().collect();
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
        "02" => GeoBBox { north: 0.0, south: -90.0, west: -180.0, east: -90.0 },
        "03" => GeoBBox { north: 0.0, south: -90.0, west: -90.0, east: 0.0 },
        "12" => GeoBBox { north: 0.0, south: -90.0, west: 0.0, east: 90.0 },
        "13" => GeoBBox { north: 0.0, south: -90.0, west: 90.0, east: 180.0 },
        "20" => GeoBBox { north: 90.0, south: 0.0, west: -180.0, east: -90.0 },
        "21" => GeoBBox { north: 90.0, south: 0.0, west: -90.0, east: 0.0 },
        "30" => GeoBBox { north: 90.0, south: 0.0, west: 0.0, east: 90.0 },
        "31" => GeoBBox { north: 90.0, south: 0.0, west: 90.0, east: 180.0 },
        _ => return None,
    };

    for ch in path[2..].chars() {
        let digit = ch.to_digit(10)? as i32;
        let lat_bit = (digit >> 1) & 1; // bit 1
        let lon_bit = digit & 1;        // bit 0

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
    let matching_roots: Vec<&crate::plugins::map_plugin::MapTreeNodePath> = map_tree.roots.iter()
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

        let matching_children: Vec<&crate::plugins::map_plugin::MapTreeNodePath> = children_set.iter()
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

                let x = node_info.bbox.min.x + u as f32 * (node_info.bbox.max.x - node_info.bbox.min.x);
                let z = node_info.bbox.max.z - v as f32 * (node_info.bbox.max.z - node_info.bbox.min.z);
                let y = node_info.bbox.min.y + 2.0; // Slightly above bottom to prevent ground clipping
                return Vec3::new(x, y, z);
            }
        }
    }

    // Fallback: mathematical ENU projection relative to reference point
    lat_lon_to_bevy(lat as f32, lon as f32, coord_res.ref_point, &coord_res.rot_matrix)
}

// ----------------------------------------------------
// Database and States
// ----------------------------------------------------

#[derive(Debug, Clone)]
pub enum RawFeatureGeometry {
    Point((f64, f64)), // (lat, lon)
    LineString(Vec<(f64, f64)>),
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

// ----------------------------------------------------
// Loading and Parsing Systems
// ----------------------------------------------------

fn setup_geojson_loading(mut commands: Commands, asset_server: Res<AssetServer>) {
    let bbox_url = format!(
        "{}3d_data_v2/data_in/zone-bbox.txt",
        crate::config::DATA_BASE_URL
    );
    let list_url = format!(
        "{}3d_data_v2/data_osm/_list.txt",
        crate::config::DATA_BASE_URL
    );

    info!("GeoJSON setups: Loading bbox from {}, list from {}", bbox_url, list_url);

    let bbox_handle = asset_server.load(bbox_url);
    let list_handle = asset_server.load(list_url);

    commands.insert_resource(GeoJsonHandles {
        bbox: bbox_handle,
        list: list_handle,
    });
}

fn check_geojson_loading(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    handles: Option<Res<GeoJsonHandles>>,
    mut text_assets: ResMut<Assets<GeoJsonTextAsset>>,
    mut loader_state: ResMut<GeoJsonLoaderState>,
    mut database: ResMut<GeoJsonDatabase>,
    coord_res: Option<Res<GeoJsonCoordinatesResource>>,
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
                        info!("GeoJSON reference point initialized at Lat: {}, Lon: {}", lat, lon);
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
                let lines: Vec<&str> = list_asset.text.lines()
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
                
                info!("Staged {} raw features for category '{}'", category_raw_features.len(), category_name);
                database.raw_categories.insert(category_name.clone(), category_raw_features);
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
    let id = properties.get("id").and_then(|v| v.as_i64()).or_else(|| tags.get("id").and_then(|s| s.parse::<i64>().ok()));
    let osm_type = properties.get("type").and_then(|v| v.as_str()).unwrap_or("node").to_string();
    
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
                    FeatureGeometry::Point(project_point(*lat, *lon, &map_tree, &coord))
                }
                RawFeatureGeometry::LineString(pts) => {
                    let projected_pts = pts.iter()
                        .map(|&(lat, lon)| project_point(lat, lon, &map_tree, &coord))
                        .collect();
                    FeatureGeometry::LineString(projected_pts)
                }
                RawFeatureGeometry::Polygon(rings) => {
                    let projected_rings = rings.iter()
                        .map(|ring| {
                            ring.iter()
                                .map(|&(lat, lon)| project_point(lat, lon, &map_tree, &coord))
                                .collect()
                        })
                        .collect();
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

        info!("Projected {} features for category '{}'", projected_features.len(), category_name);
        database.categories.insert(category_name, projected_features);
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
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    egui::Window::new("GeoJSON Database").show(ctx, |ui| {
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
        
        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            if query_trimmed.len() <= 3 {
                // Display first 10 items of each type
                for (cat_name, features) in &database.categories {
                    ui.collapsing(format!("{} ({})", cat_name, features.len()), |ui| {
                        let display_count = features.len().min(10);
                        for idx in 0..display_count {
                            let feature = &features[idx];
                            let display_name = feature.name.clone().unwrap_or_else(|| {
                                format!("{} #{}", feature.osm_type, feature.id.unwrap_or(idx as i64))
                            });
                            
                            let is_selected = selection.selected.as_ref() == Some(&(cat_name.clone(), idx));
                            if ui.selectable_label(is_selected, &display_name).clicked() {
                                select_and_animate(cat_name.clone(), idx, feature, &mut commands, &camera_query);
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
                    let is_selected = selection.selected.as_ref() == Some(&(cat_name.clone(), idx));
                    if ui.selectable_label(is_selected, &display_name).clicked() {
                        select_and_animate(cat_name.clone(), idx, feature, &mut commands, &camera_query);
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
            target_rot = Transform::from_translation(target_pos).looking_at(*p, Vec3::Y).rotation;
        }
        FeatureGeometry::LineString(_) | FeatureGeometry::Polygon(_) => {
            let center = feature.center;
            let size = feature.bbox_max - feature.bbox_min;
            let dist = size.x.max(size.z).max(10.0) * 1.5;
            
            target_pos = Vec3::new(center.x, center.y + dist, center.z + dist * 0.5);
            target_rot = Transform::from_translation(target_pos).looking_at(center, Vec3::Y).rotation;
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
    
    info!("Animating camera for selection '{}/{}': Target position {:?}", cat_name, idx, target_pos);
}

// ----------------------------------------------------
// Gizmos Highlight rendering
// ----------------------------------------------------

fn geojson_gizmos_system(
    mut gizmos: Gizmos,
    database: Res<GeoJsonDatabase>,
    selection: Res<GeoJsonSelection>,
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
    
    let color = Color::srgb(1.0, 1.0, 0.0); // Bright Yellow
    
    match &feature.geometry {
        FeatureGeometry::Point(p) => {
            let sphere = Sphere::new(3.0);
            gizmos.primitive_3d(&sphere, Isometry3d::from_translation(*p), color);
            gizmos.line(*p - Vec3::X * 10.0, *p + Vec3::X * 10.0, color);
            gizmos.line(*p - Vec3::Y * 10.0, *p + Vec3::Y * 10.0, color);
            gizmos.line(*p - Vec3::Z * 10.0, *p + Vec3::Z * 10.0, color);
        }
        FeatureGeometry::LineString(pts) => {
            for window in pts.windows(2) {
                gizmos.line(window[0], window[1], color);
            }
        }
        FeatureGeometry::Polygon(rings) => {
            for ring in rings {
                for window in ring.windows(2) {
                    gizmos.line(window[0], window[1], color);
                }
            }
        }
    }
}
