use bevy::asset::{Asset, AssetLoader, LoadContext, io::Reader};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bytes::Bytes;
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::Field;
use std::collections::{BTreeMap, BTreeSet};

use crate::plugins::map_plugin::{
    BBox, MapLODState, MapTileAssetId, MapTree, MapTreeAssetInfo, MapTreeNodeInfo, MapTreeNodePath,
};

#[derive(Resource)]
pub struct ParquetHandles {
    nodes: Handle<ParquetAsset>,
}

pub fn init_parquet_handles(mut commands: Commands, asset_server: Res<AssetServer>) {
    // Load parquet assets from HTTP URL
    let nodes_url = format!(
        "{}/3d_data/tree_nodes.parquet",
        crate::config::DATA_BASE_URL
    );

    info!("Loading nodes from: {}", nodes_url);

    let nodes_handle = asset_server.load(nodes_url);

    commands.insert_resource(ParquetHandles {
        nodes: nodes_handle,
    });
}

#[derive(Asset, TypePath, Debug, Clone)]
pub struct ParquetAsset {
    pub bytes: Vec<u8>,
}

#[derive(Default, TypePath)]
pub struct ParquetAssetLoader;

impl AssetLoader for ParquetAssetLoader {
    type Asset = ParquetAsset;
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
        Ok(ParquetAsset { bytes })
    }

    fn extensions(&self) -> &[&str] {
        &["parquet"]
    }
}

fn get_string(field: Field) -> Option<String> {
    match field {
        Field::Str(s) => Some(s),
        _ => None,
    }
}

fn get_int(field: Field) -> Option<i64> {
    match field {
        Field::Int(v) => Some(v as i64),
        Field::Long(v) => Some(v),
        Field::UInt(v) => Some(v as i64),
        Field::ULong(v) => Some(v as i64),
        Field::Short(v) => Some(v as i64),
        Field::UShort(v) => Some(v as i64),
        Field::Byte(v) => Some(v as i64),
        Field::UByte(v) => Some(v as i64),
        _ => None,
    }
}

fn get_float(field: Field) -> Option<f32> {
    match field {
        Field::Float(v) => Some(v),
        Field::Double(v) => Some(v as f32),
        _ => None,
    }
}

fn parse_tree_nodes(bytes: &[u8]) -> Vec<MapTreeAssetInfo> {
    let bytes_data = Bytes::copy_from_slice(bytes);
    let reader = match SerializedFileReader::new(bytes_data) {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to initialize SerializedFileReader: {:?}", e);
            return Vec::new();
        }
    };
    let mut nodes = Vec::new();
    let row_iter = match reader.get_row_iter(None) {
        Ok(it) => it,
        Err(e) => {
            error!("Failed to get row iterator: {:?}", e);
            return Vec::new();
        }
    };

    for row in row_iter {
        let row = match row {
            Ok(r) => r,
            Err(e) => {
                error!("Error reading node row: {:?}", e);
                continue;
            }
        };
        let mut name = String::new();
        let mut type_ = String::new();
        let mut level = None;
        let mut minx = 0.0;
        let mut maxx = 0.0;
        let mut miny = 0.0;
        let mut maxy = 0.0;
        let mut minz = 0.0;
        let mut maxz = 0.0;
        let mut octant_path = MapTreeNodePath(String::new());
        let mut filename = None;
        let mut vertex_count = None;

        for (col_name, field) in row.into_columns() {
            match col_name.as_str() {
                "name" => {
                    name = get_string(field).unwrap_or_default();
                }
                "type" => {
                    type_ = get_string(field).unwrap_or_default();
                }
                "level" => {
                    level = get_int(field).map(|v| v as i32);
                }
                "minx" => {
                    minx = get_float(field).unwrap_or(0.0);
                }
                "maxx" => {
                    maxx = get_float(field).unwrap_or(0.0);
                }
                "miny" => {
                    miny = get_float(field).unwrap_or(0.0);
                }
                "maxy" => {
                    maxy = get_float(field).unwrap_or(0.0);
                }
                "minz" => {
                    minz = get_float(field).unwrap_or(0.0);
                }
                "maxz" => {
                    maxz = get_float(field).unwrap_or(0.0);
                }
                "octant_path" => {
                    octant_path.0 = get_string(field).unwrap_or_default();
                }
                "filename" => {
                    filename = get_string(field);
                }
                "vertex_count" => {
                    vertex_count = get_int(field);
                }
                _ => {}
            }
        }

        nodes.push(MapTreeAssetInfo {
            name: MapTileAssetId(name),
            r#type: type_,
            level,
            _octant_path: octant_path,
            filename,
            vertex_count,
            bbox: BBox {
                min: Vec3::new(minx, minz, -miny),
                max: Vec3::new(maxx, maxz, -maxy),
            },
        });
    }
    nodes
}

pub fn check_and_parse_parquet(
    mut commands: Commands,
    handles: Option<Res<ParquetHandles>>,
    mut parquet_assets: ResMut<Assets<ParquetAsset>>,
    mut data_res: ResMut<MapTree>,
    mut lod_state: ResMut<MapLODState>,
    mut camera_query: Query<&mut Transform, With<Camera>>,
) {
    if data_res.parsed {
        return;
    }

    let Some(handles) = handles else {
        return;
    };
    if parquet_assets.get(&handles.nodes).is_none() {
        return;
    }
    info!("Nodes parquet file loaded! Parsing...");

    let nodes_asset = parquet_assets.remove(&handles.nodes).unwrap();
    let mut parsed_nodes = parse_tree_nodes(&nodes_asset.bytes);
    parsed_nodes.sort_by(|a, b| a.name.cmp(&b.name));

    info!("Parsed {} raw assets.", parsed_nodes.len());

    let mut assets = BTreeMap::new();
    for asset in parsed_nodes {
        // skip non-mesh assets
        if asset.r#type == "mesh" {
            assets.insert(asset.name.clone(), asset);
        }
    }

    // Group assets into nodes
    let mut nodes: BTreeMap<MapTreeNodePath, MapTreeNodeInfo> = BTreeMap::new();
    for asset in assets.values() {
        let path = asset.name.get_octant_path();
        if let Some(node_info) = nodes.get_mut(&path) {
            node_info.assets.push(asset.name.clone());
            node_info.bbox.min = node_info.bbox.min.min(asset.bbox.min);
            node_info.bbox.max = node_info.bbox.max.max(asset.bbox.max);
        } else {
            nodes.insert(
                path.clone(),
                MapTreeNodeInfo {
                    path: path.clone(),
                    assets: vec![asset.name.clone()],
                    bbox: asset.bbox,
                },
            );
        }
    }

    // Establish parent/child relationships between nodes based on their paths
    let mut children: BTreeMap<MapTreeNodePath, BTreeSet<MapTreeNodePath>> = BTreeMap::new();
    let mut parents: BTreeMap<MapTreeNodePath, MapTreeNodePath> = BTreeMap::new();

    for path in nodes.keys() {
        if let Some(parent_path) = path.get_parent() {
            if nodes.contains_key(&parent_path) {
                parents.insert(path.clone(), parent_path.clone());
                children
                    .entry(parent_path)
                    .or_default()
                    .insert(path.clone());
            }
        }
    }

    // Roots are nodes that have no parent in our parents map
    let mut roots = BTreeSet::new();
    for path in nodes.keys() {
        if !parents.contains_key(path) {
            roots.insert(path.clone());
        }
    }

    info!("Found {} root nodes.", roots.len());

    // Traverse and calculate depth/level for assets in the tree
    let mut queue = Vec::new();
    for root in &roots {
        queue.push((root.clone(), 0));
    }
    while let Some((node_path, depth)) = queue.pop() {
        if let Some(node_info) = nodes.get(&node_path) {
            for asset_id in &node_info.assets {
                if let Some(asset) = assets.get_mut(asset_id) {
                    asset.level = Some(depth);
                }
            }
        }
        if let Some(node_children) = children.get(&node_path) {
            for child_path in node_children {
                queue.push((child_path.clone(), depth + 1));
            }
        }
    }

    if !nodes.is_empty() {
        let mut min_x = f32::INFINITY;
        let mut max_x = -f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_y = -f32::INFINITY;
        let mut min_z = f32::INFINITY;
        let mut max_z = -f32::INFINITY;

        for node in nodes.values() {
            min_x = min_x.min(node.bbox.min.x).min(node.bbox.max.x);
            max_x = max_x.max(node.bbox.min.x).max(node.bbox.max.x);
            min_y = min_y.min(node.bbox.min.y).min(node.bbox.max.y);
            max_y = max_y.max(node.bbox.min.y).max(node.bbox.max.y);
            min_z = min_z.min(node.bbox.min.z).min(node.bbox.max.z);
            max_z = max_z.max(node.bbox.min.z).max(node.bbox.max.z);
        }

        let bbox = BBox {
            min: Vec3::new(min_x, min_y, min_z),
            max: Vec3::new(max_x, max_y, max_z),
        };

        info!("Computed entire scene bbox: {:?}", bbox);

        let middle = (bbox.min + bbox.max) / 2.0;
        let size = bbox.max - bbox.min;
        let offset_y = size.y.max(10.0) * 1.2;
        let camera_pos = Vec3::new(bbox.max.x, bbox.max.y + offset_y, bbox.max.z);

        info!("Placing camera at {:?} looking at {:?}", camera_pos, middle);
        for mut cam_transform in &mut camera_query {
            *cam_transform = Transform::from_translation(camera_pos).looking_at(middle, Vec3::Y);
        }

        data_res.bbox = bbox;
    }

    data_res.assets = assets;
    data_res.all_nodes = nodes;
    data_res.children = children;
    data_res.parents = parents;
    data_res.parsed = true;
    data_res.roots = roots.clone();

    lod_state.selected_node = None;
    let budget = roots
        .iter()
        .map(|i| data_res.all_nodes.get(i).unwrap().assets.len())
        .sum::<usize>()
        + 200;
    lod_state.lod_budget = budget as u32;
    let timeout = 0.1 + rand::random::<f32>() * 0.1;
    lod_state.lod_timer = Some(Timer::from_seconds(timeout, TimerMode::Once));

    commands.remove_resource::<ParquetHandles>();
}
