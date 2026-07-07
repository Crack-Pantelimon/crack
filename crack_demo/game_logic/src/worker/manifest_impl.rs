use crate::api::FetchArgs;
use crate::map::{
    BBox, MapManifestResult, MapTileAssetId, MapTreeAssetInfo, MapTreeData, MapTreeNodeInfo,
    MapTreeNodePath,
};
use bytes::Bytes;
use glam::Vec3;
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::Field;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use tokio::sync::RwLock;

static MANIFEST_CACHE: RwLock<Option<Arc<MapTreeData>>> = RwLock::const_new(None);

pub async fn get_manifest_cache() -> anyhow::Result<Arc<MapTreeData>> {
    let guard = MANIFEST_CACHE.read().await;
    if let Some(cache) = &*guard {
        return Ok(cache.clone());
    }
    anyhow::bail!("LOD requested before manifest was fetched")
}

pub async fn fetch_map_manifest(args: FetchArgs) -> anyhow::Result<MapManifestResult> {
    {
        let guard = MANIFEST_CACHE.read().await;
        if let Some(cache) = &*guard {
            return Ok(MapManifestResult {
                tree: (**cache).clone(),
            });
        }
    }

    let mut guard = MANIFEST_CACHE.write().await;
    if let Some(cache) = &*guard {
        return Ok(MapManifestResult {
            tree: (**cache).clone(),
        });
    }

    let url = format!("{}/3d_data_v2/data_out/manifest.parquet", args.base_url);
    tracing::info!("Worker fetching manifest from {}", url);

    let bytes = super::http::http_get_bytes(&url).await?;
    let tree = build_map_tree(&bytes)?;

    let arc = Arc::new(tree);
    *guard = Some(arc.clone());

    Ok(MapManifestResult {
        tree: (*arc).clone(),
    })
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

fn parse_tree_nodes(bytes: &[u8]) -> anyhow::Result<Vec<MapTreeAssetInfo>> {
    let bytes_data = Bytes::copy_from_slice(bytes);
    let reader = SerializedFileReader::new(bytes_data)
        .map_err(|e| anyhow::anyhow!("Failed to initialize SerializedFileReader: {:?}", e))?;

    let mut nodes = Vec::new();
    let row_iter = reader
        .get_row_iter(None)
        .map_err(|e| anyhow::anyhow!("Failed to get row iterator: {:?}", e))?;

    for row in row_iter {
        let row = row.map_err(|e| anyhow::anyhow!("Error reading node row: {:?}", e))?;
        let mut level = None;
        let mut x_min = 0.0;
        let mut x_max = 0.0;
        let mut y_min = 0.0;
        let mut y_max = 0.0;
        let mut z_min = 0.0;
        let mut z_max = 0.0;
        let mut octant_path = MapTreeNodePath(String::new());
        let mut glb_path = None;
        let mut vertex_count = None;
        let mut mesh_count = None;

        for (col_name, field) in row.into_columns() {
            match col_name.as_str() {
                "depth" => level = get_int(field).map(|v| v as i32),
                "x_min" => x_min = get_float(field).unwrap_or(0.0),
                "x_max" => x_max = get_float(field).unwrap_or(0.0),
                "y_min" => y_min = get_float(field).unwrap_or(0.0),
                "y_max" => y_max = get_float(field).unwrap_or(0.0),
                "z_min" => z_min = get_float(field).unwrap_or(0.0),
                "z_max" => z_max = get_float(field).unwrap_or(0.0),
                "octant_path" => octant_path.0 = get_string(field).unwrap_or_default(),
                "glb_path" => glb_path = get_string(field),
                "vertex_count" => vertex_count = get_int(field),
                "mesh_count" => mesh_count = get_int(field),
                _ => {}
            }
        }

        let name = MapTileAssetId(octant_path.0.clone());

        nodes.push(MapTreeAssetInfo {
            name,
            level,
            _octant_path: octant_path,
            glb_path,
            vertex_count,
            mesh_count,
            bbox: BBox {
                min: Vec3::new(x_min, y_min, z_min),
                max: Vec3::new(x_max, y_max, z_max),
            },
        });
    }
    Ok(nodes)
}

fn build_map_tree(bytes: &[u8]) -> anyhow::Result<MapTreeData> {
    let mut parsed_nodes = parse_tree_nodes(bytes)?;
    parsed_nodes.sort_by(|a, b| a.name.cmp(&b.name));

    tracing::info!("Worker parsed {} raw assets.", parsed_nodes.len());

    let mut assets = BTreeMap::new();
    for asset in parsed_nodes {
        if asset.level.unwrap_or(0) >= 14 {
            assets.insert(asset.name.clone(), asset);
        }
    }

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

    let mut roots = BTreeSet::new();
    for path in nodes.keys() {
        if !parents.contains_key(path) {
            roots.insert(path.clone());
        }
    }

    tracing::info!("Worker found {} root nodes.", roots.len());

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

    let mut bbox = BBox::default();
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

        bbox = BBox {
            min: Vec3::new(min_x, min_y, min_z),
            max: Vec3::new(max_x, max_y, max_z),
        };
    }

    Ok(MapTreeData {
        assets,
        all_nodes: nodes,
        children,
        parents,
        roots,
        bbox,
    })
}
