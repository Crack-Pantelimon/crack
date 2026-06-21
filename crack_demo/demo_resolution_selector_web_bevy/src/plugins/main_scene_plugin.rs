use bevy::asset::{Asset, AssetLoader, LoadContext, io::Reader};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::world_serialization::WorldAssetRoot;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use bytes::Bytes;
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::Field;
use std::collections::{HashMap, HashSet};

pub struct MainScenePlugin;

impl Plugin for MainScenePlugin {
    fn build(&self, app: &mut App) {
        info!("loading: MainScenePlugin...");
        crate::ui_egui::web_set_loading_status(true, "Loading MainScenePlugin...");
        app.init_asset::<ParquetAsset>()
            .init_asset_loader::<ParquetAssetLoader>()
            .init_resource::<Data3DResource>()
            .add_systems(
                Startup,
                (setup_camera_and_load, || {
                    crate::ui_egui::web_set_loading_status(false, "");
                }),
            )
            .add_systems(EguiPrimaryContextPass, tree_navigator_ui)
            .add_systems(Update, (check_and_parse_parquet, draw_tree_bboxes, sync_node_models));
        info!("done loading: MainScenePlugin");
    }
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

#[derive(Clone, Debug)]
pub struct TreeNode {
    pub name: String,
    pub r#type: String,
    pub level: Option<i32>,
    pub minx: f32,
    pub maxx: f32,
    pub miny: f32,
    pub maxy: f32,
    pub minz: f32,
    pub maxz: f32,
    pub octant_path: String,
    pub filename: Option<String>,
    pub vertex_count: Option<i64>,
}

#[derive(Clone, Copy, Debug)]
pub struct BBox {
    pub min: Vec3,
    pub max: Vec3,
}

#[derive(Resource, Default, Debug)]
pub struct Data3DResource {
    pub nodes: HashMap<String, TreeNode>,
    pub children: HashMap<String, HashMap<char, String>>,
    pub parents: HashMap<String, String>,
    pub bbox: Option<BBox>,
    pub parsed: bool,
    pub rendered_nodes: HashSet<String>,
    pub selected_node: Option<String>,
}

#[derive(Resource)]
struct ParquetHandles {
    nodes: Handle<ParquetAsset>,
}

#[derive(Component)]
struct RenderedNodeModel {
    node_name: String,
}

fn setup_camera_and_load(mut commands: Commands, asset_server: Res<AssetServer>) {
    // Keep only default camera spawning
    commands.spawn((
        Transform::from_xyz(0.0, 10.5, -30.0).looking_at(Vec3::ZERO, Vec3::Y),
        Camera {
            clear_color: Color::BLACK.into(),
            ..default()
        },
        Camera3d::default(),
        Tonemapping::None,
    ));

    // Spawn directional light (sun)
    commands.spawn((
        DirectionalLight {
            illuminance: 10000.0,
            ..default()
        },
        Transform::from_xyz(10.0, 20.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

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

fn parse_tree_nodes(bytes: &[u8]) -> Vec<TreeNode> {
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
        let mut octant_path = String::new();
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
                    octant_path = get_string(field).unwrap_or_default();
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

        nodes.push(TreeNode {
            name,
            r#type: type_,
            level,
            minx,
            maxx,
            miny: minz,
            maxy: maxz,
            minz: -maxy,
            maxz: -miny,
            octant_path,
            filename,
            vertex_count,
        });
    }
    nodes
}

fn get_octant_path(name: &str) -> String {
    if let Some(idx) = name.rfind('_') {
        name[..idx].to_string()
    } else {
        name.to_string()
    }
}

fn check_and_parse_parquet(
    mut commands: Commands,
    handles: Option<Res<ParquetHandles>>,
    mut parquet_assets: ResMut<Assets<ParquetAsset>>,
    mut data_res: ResMut<Data3DResource>,
    mut camera_query: Query<&mut Transform, With<Camera>>,
) {
    if data_res.parsed {
        return;
    }

    if let Some(handles) = handles {
        if parquet_assets.get(&handles.nodes).is_some() {
            info!("Nodes parquet file loaded! Parsing...");

            let nodes_asset = parquet_assets.remove(&handles.nodes).unwrap();
            let parsed_nodes = parse_tree_nodes(&nodes_asset.bytes);

            info!("Parsed {} raw nodes.", parsed_nodes.len());

            let mut nodes = HashMap::new();
            for node in parsed_nodes {
                // skip non-mesh nodes
                if node.r#type == "mesh" {
                    nodes.insert(node.name.clone(), node);
                }
            }

            // group mesh names by octant_path
            let mut path_to_meshes: HashMap<String, Vec<String>> = HashMap::new();
            for mesh in nodes.values() {
                let path = get_octant_path(&mesh.name);
                path_to_meshes.entry(path).or_default().push(mesh.name.clone());
            }

            // establish parents and children maps from octant path
            let mut children: HashMap<String, HashMap<char, String>> = HashMap::new();
            let mut parents: HashMap<String, String> = HashMap::new();

            for mesh in nodes.values() {
                let path = get_octant_path(&mesh.name);
                if !path.is_empty() {
                    let parent_path = path[..path.len() - 1].to_string();
                    if let Some(parent_meshes) = path_to_meshes.get(&parent_path) {
                        for parent_name in parent_meshes {
                            parents.insert(mesh.name.clone(), parent_name.clone());

                            let mut char_key = path.chars().last().unwrap_or(' ');
                            let parent_children = children.entry(parent_name.clone()).or_default();
                            if parent_children.contains_key(&char_key) {
                                for c in "01234567abcdefghijklmnopqrstuvwxyz".chars() {
                                    if !parent_children.contains_key(&c) {
                                        char_key = c;
                                        break;
                                    }
                                }
                            }
                            parent_children.insert(char_key, mesh.name.clone());
                        }
                    }
                }
            }

            // Find roots (meshes in our nodes map that have no parent in parents map)
            let mut roots = Vec::new();
            for node_name in nodes.keys() {
                if !parents.contains_key(node_name) {
                    roots.push(node_name.clone());
                }
            }

            // Filter to roots with min name length (dropping outliers with a longer name)
            if !roots.is_empty() {
                let min_len = roots.iter().map(|r| r.len()).min().unwrap_or(0);
                roots.retain(|r| r.len() == min_len);
            }

            info!("Found {} root nodes after filtering.", roots.len());

            // Traverse and calculate depth (roots level = 0, child = parent + 1)
            let mut queue = Vec::new();
            for root in &roots {
                queue.push((root.clone(), 0));
            }
            while let Some((node_name, depth)) = queue.pop() {
                if let Some(node) = nodes.get_mut(&node_name) {
                    node.level = Some(depth);
                }
                if let Some(node_children) = children.get(&node_name) {
                    for child_name in node_children.values() {
                        queue.push((child_name.clone(), depth + 1));
                    }
                }
            }

            // originally keep all roots in rendered_nodes
            let mut rendered_nodes = HashSet::new();
            for root in &roots {
                rendered_nodes.insert(root.clone());
            }

            if !nodes.is_empty() {
                let mut min_x = f32::INFINITY;
                let mut max_x = -f32::INFINITY;
                let mut min_y = f32::INFINITY;
                let mut max_y = -f32::INFINITY;
                let mut min_z = f32::INFINITY;
                let mut max_z = -f32::INFINITY;

                for node in nodes.values() {
                    min_x = min_x.min(node.minx).min(node.maxx);
                    max_x = max_x.max(node.minx).max(node.maxx);
                    min_y = min_y.min(node.miny).min(node.maxy);
                    max_y = max_y.max(node.miny).max(node.maxy);
                    min_z = min_z.min(node.minz).min(node.maxz);
                    max_z = max_z.max(node.minz).max(node.maxz);
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
                    *cam_transform =
                        Transform::from_translation(camera_pos).looking_at(middle, Vec3::Y);
                }

                data_res.bbox = Some(bbox);
            }

            data_res.nodes = nodes;
            data_res.children = children;
            data_res.parents = parents;
            data_res.rendered_nodes = rendered_nodes;
            data_res.selected_node = None;
            data_res.parsed = true;

            commands.remove_resource::<ParquetHandles>();
        }
    }
}

fn draw_tree_bboxes(mut gizmos: Gizmos, data_res: Res<Data3DResource>) {
    if !data_res.parsed {
        return;
    }

    for node_name in &data_res.rendered_nodes {
        if let Some(node) = data_res.nodes.get(node_name) {
            let is_selected = data_res.selected_node.as_ref() == Some(node_name);
            let color = if is_selected {
                Color::srgb(1.0, 0.0, 0.0) // Red if selected
            } else if data_res.parents.get(node_name).is_none() {
                Color::srgb(0.0, 1.0, 0.0) // Green for root
            } else {
                Color::srgb(0.0, 0.5, 1.0) // Blue for others
            };
            draw_node_bbox(&mut gizmos, node, color);
        }
    }
}

fn draw_node_bbox(gizmos: &mut Gizmos, node: &TreeNode, color: Color) {
    let center = Vec3::new(
        (node.minx + node.maxx) / 2.0,
        (node.miny + node.maxy) / 2.0,
        (node.minz + node.maxz) / 2.0,
    );
    let size = Vec3::new(
        (node.maxx - node.minx).abs(),
        (node.maxy - node.miny).abs(),
        (node.maxz - node.minz).abs(),
    );
    let cuboid = Cuboid::new(size.x, size.y, size.z);
    gizmos.primitive_3d(&cuboid, Isometry3d::from_translation(center), color);
}

fn tree_navigator_ui(mut contexts: EguiContexts, mut data_res: ResMut<Data3DResource>) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if !data_res.parsed {
        return;
    }

    let mut node_to_select = None;
    let mut node_to_deselect = false;
    let mut node_to_expand = None;

    egui::Window::new("Tree Navigator").show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            let rendered_names: Vec<String> = data_res.rendered_nodes.iter().cloned().collect();

            for node_name in rendered_names {
                if let Some(node) = data_res.nodes.get(&node_name) {
                    let is_selected = data_res.selected_node.as_ref() == Some(&node_name);
                    let label_text = format!(
                        "Name: {} | Type: {} | Level: {:?} | Vertices: {:?}",
                        node.name,
                        node.r#type,
                        node.level.unwrap_or(0),
                        node.vertex_count.unwrap_or(0)
                    );

                    let has_children = data_res.children.contains_key(&node_name);

                    ui.horizontal(|ui| {
                        let resp = ui.selectable_label(is_selected, label_text);
                        if resp.clicked() {
                            if is_selected {
                                node_to_deselect = true;
                            } else {
                                node_to_select = Some(node_name.clone());
                            }
                        }

                        if has_children {
                            if ui.button("Expand").clicked() {
                                node_to_expand = Some(node_name.clone());
                            }
                        }
                    });
                }
            }
        });
    });

    if node_to_deselect {
        data_res.selected_node = None;
    } else if let Some(name) = node_to_select {
        data_res.selected_node = Some(name);
    }

    if let Some(name) = node_to_expand {
        // remove the expanded item from the rendered list
        data_res.rendered_nodes.remove(&name);
        
        let child_names: Vec<String> = if let Some(node_children) = data_res.children.get(&name) {
            node_children.values().cloned().collect()
        } else {
            Vec::new()
        };
        for child in child_names {
            data_res.rendered_nodes.insert(child);
        }
    }
}

fn sync_node_models(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    data_res: Res<Data3DResource>,
    model_query: Query<(Entity, &RenderedNodeModel)>,
) {
    if !data_res.parsed {
        return;
    }

    // Despawn models for nodes that are no longer in rendered_nodes
    let mut spawned_names = HashSet::new();
    for (entity, model) in &model_query {
        if !data_res.rendered_nodes.contains(&model.node_name) {
            commands.entity(entity).despawn();
        } else {
            spawned_names.insert(model.node_name.clone());
        }
    }

    // Spawn models for nodes in rendered_nodes that aren't spawned yet
    for node_name in &data_res.rendered_nodes {
        if !spawned_names.contains(node_name) {
            if let Some(node) = data_res.nodes.get(node_name) {
                if let Some(ref filename) = node.filename {
                    let glb_url = format!("{}/3d_data/{}", crate::config::DATA_BASE_URL, filename);
                    let asset_path = GltfAssetLabel::Scene(0).from_asset(glb_url);
                    
                    commands.spawn((
                        WorldAssetRoot(asset_server.load(asset_path)),
                        Transform::from_xyz(0.0, 0.0, 0.0),
                        RenderedNodeModel {
                            node_name: node_name.clone(),
                        },
                        avian3d::prelude::RigidBody::Static,
                        avian3d::prelude::ColliderConstructorHierarchy::new(
                            avian3d::prelude::ColliderConstructor::TrimeshFromMesh,
                        ),
                    ));
                }
            }
        }
    }
}
