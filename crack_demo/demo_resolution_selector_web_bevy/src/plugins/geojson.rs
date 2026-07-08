use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;
use bevy_egui::{EguiContexts, egui};
use std::collections::BTreeMap;

use crate::plugins::game_freecam::camera_controls::ActiveCameraAnimation;
use crate::plugins::states::OsmDatabaseLoadFinished;

pub struct GeoJsonPlugin;

impl Plugin for GeoJsonPlugin {
    fn build(&self, app: &mut App) {
        info!("loading: GeoJsonPlugin...");
        app.init_resource::<GeoJsonDatabase>()
            .init_resource::<GeoJsonSearchState>()
            .init_resource::<GeoJsonSelection>()
            .init_resource::<GameLoadingStatus>()
            .init_resource::<OsmOverlayState>()
            .add_systems(
                EguiPrimaryContextPass,
                (
                    geojson_ui_system,
                    geojson_text_labels_system,
                    osm_overlay_ui_system,
                ),
            )
            .add_systems(
                Update,
                (
                    update_geojson_loading_finished,
                    geojson_gizmos_system,
                    osm_overlay_gizmos_system,
                    init_bus_route,
                    move_bus_system,
                ),
            );
        info!("done loading: GeoJsonPlugin");
    }
}

pub use game_logic::osm::{FeatureGeometry, GeoJsonFeature};

#[derive(Resource, Default)]
pub struct GeoJsonDatabase {
    pub categories: BTreeMap<String, Vec<GeoJsonFeature>>,
    pub parsed: bool,
}

#[derive(Resource, Default)]
pub struct GeoJsonSearchState {
    pub query: String,
}

#[derive(Resource, Default)]
pub struct GeoJsonSelection {
    pub selected: Option<(String, usize)>,
}

#[derive(Resource, Debug, Default)]
pub struct GameLoadingStatus {
    pub map_loaded: bool,
    pub geojson_loaded: bool,
    pub geojson_loading_started: bool,
}

fn update_geojson_loading_finished(
    database: Res<GeoJsonDatabase>,
    mut loading_status: ResMut<GameLoadingStatus>,
    mut commands: Commands,
    mut next_state: ResMut<NextState<OsmDatabaseLoadFinished>>,
) {
    if database.parsed && !loading_status.geojson_loaded {
        loading_status.geojson_loaded = true;
        commands.trigger(crate::plugins::notifications::NotificationEvent::GeoJsonLoaded);
        next_state.set(OsmDatabaseLoadFinished::OsmFinished);
        info!("GeoJSON loading is fully completed!");
    }
}

pub(crate) fn query_point_ground_y(
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
        FeatureGeometry::LineString(_)
        | FeatureGeometry::MultiLineString(_)
        | FeatureGeometry::Polygon(_) => {
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
    osm_overlay: Res<OsmOverlayState>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if !database.parsed {
        return;
    }
    let Some((camera, camera_transform)) = camera_query.iter().next() else {
        return;
    };

    // 1. Draw Selected Feature labels (if any selected)
    if let Some((cat_name, idx)) = &selection.selected {
        if let Some(features) = database.categories.get(cat_name) {
            if let Some(feature) = features.get(*idx) {
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
                        if let Ok(p_edge) = camera
                            .world_to_viewport(camera_transform, pos + camera_right * sphere_radius)
                        {
                            let r_screen = p_center.distance(p_edge);
                            let font_size = (r_screen * 3.0).clamp(11.0, 36.0);

                            egui::Area::new(egui::Id::new(format!("lbl_{:?}_{}", pos, label)))
                                .fixed_pos(egui::pos2(
                                    p_center.x - 20.0,
                                    p_center.y - font_size - 8.0,
                                ))
                                .show(ctx, |ui| {
                                    ui.label(
                                        egui::RichText::new(&label)
                                            .color(egui::Color32::from_rgb(255, 60, 60))
                                            .size(font_size)
                                            .strong()
                                            .background_color(
                                                egui::Color32::from_rgba_premultiplied(
                                                    0, 0, 0, 180,
                                                ),
                                            ),
                                    );
                                });
                        }
                    }
                }
            }
        }
    }

    // 2. Draw active overlays names (composing overlapping roads and bus routes)
    let mut label_candidates = Vec::new();

    // 2.1 Roads (Streets)
    if osm_overlay.show_roads {
        if let Some(features) = database.categories.get("roads") {
            for feat in features {
                if let Some(name) = &feat.name {
                    if !name.trim().is_empty() {
                        match &feat.geometry {
                            FeatureGeometry::LineString(pts) => {
                                for (idx, pt) in pts.iter().enumerate() {
                                    if idx == 0 || idx % 8 == 0 || idx == pts.len() - 1 {
                                        label_candidates.push((
                                            *pt,
                                            name.clone(),
                                            "road".to_string(),
                                        ));
                                    }
                                }
                            }
                            FeatureGeometry::MultiLineString(lines) => {
                                for pts in lines {
                                    for (idx, pt) in pts.iter().enumerate() {
                                        if idx == 0 || idx % 8 == 0 || idx == pts.len() - 1 {
                                            label_candidates.push((
                                                *pt,
                                                name.clone(),
                                                "road".to_string(),
                                            ));
                                        }
                                    }
                                }
                            }
                            FeatureGeometry::Point(pt) => {
                                label_candidates.push((*pt, name.clone(), "road".to_string()));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    // 2.2 Bus/Tram/Trolleybus Routes
    if osm_overlay.show_bus_routes {
        if let Some(features) = database.categories.get("routes") {
            for feat in features {
                let route_type = feat.tags.get("route").map(|s| s.as_str()).unwrap_or("");
                let is_public_transport = route_type == "bus"
                    || route_type == "tram"
                    || route_type == "trolleybus"
                    || route_type == "trolley"
                    || route_type == "tramway";
                if !is_public_transport {
                    continue;
                }
                let ref_name = feat
                    .tags
                    .get("ref")
                    .cloned()
                    .or_else(|| feat.name.clone())
                    .unwrap_or_else(|| "Line".to_string());

                let label_text = if route_type == "bus" {
                    format!("Bus {}", ref_name)
                } else if route_type == "tram" || route_type == "tramway" {
                    format!("Tram {}", ref_name)
                } else if route_type == "trolleybus" || route_type == "trolley" {
                    format!("Trolley {}", ref_name)
                } else {
                    ref_name
                };

                match &feat.geometry {
                    FeatureGeometry::LineString(pts) => {
                        for (idx, pt) in pts.iter().enumerate() {
                            if idx == 0 || idx % 6 == 0 || idx == pts.len() - 1 {
                                label_candidates.push((
                                    *pt,
                                    label_text.clone(),
                                    "route".to_string(),
                                ));
                            }
                        }
                    }
                    FeatureGeometry::MultiLineString(lines) => {
                        for pts in lines {
                            for (idx, pt) in pts.iter().enumerate() {
                                if idx == 0 || idx % 6 == 0 || idx == pts.len() - 1 {
                                    label_candidates.push((
                                        *pt,
                                        label_text.clone(),
                                        "route".to_string(),
                                    ));
                                }
                            }
                        }
                    }
                    FeatureGeometry::Point(pt) => {
                        label_candidates.push((*pt, label_text.clone(), "route".to_string()));
                    }
                    _ => {}
                }
            }
        }
    }

    // 2.3 Businesses (Shops/Amenities/Offices/Craft)
    if osm_overlay.show_businesses {
        for cat in &["shops", "amenities", "offices", "craft"] {
            if let Some(features) = database.categories.get(*cat) {
                for feat in features {
                    if let Some(name) = &feat.name {
                        if !name.trim().is_empty() {
                            label_candidates.push((
                                feat.center,
                                name.clone(),
                                "business".to_string(),
                            ));
                        }
                    }
                }
            }
        }
    }

    let cam_translation = camera_transform.translation();
    let mut close_candidates = Vec::new();
    for (pos, name, type_) in label_candidates {
        let dist = cam_translation.distance(pos);
        if dist < 400.0 {
            // only show labels within 400 meters of camera to avoid screen clutter
            close_candidates.push((pos, name, type_, dist));
        }
    }

    // Cluster close label candidates
    let mut clusters: Vec<(Vec3, Vec<(String, String)>)> = Vec::new(); // average_pos, Vec<(name, type)>
    for (pos, name, type_, _dist) in close_candidates {
        let mut found_cluster_idx = None;
        for (idx, (c_pos, _)) in clusters.iter().enumerate() {
            let d_xz = Vec2::new(pos.x - c_pos.x, pos.z - c_pos.z).length();
            if d_xz < 40.0 {
                // 40m grouping radius
                found_cluster_idx = Some(idx);
                break;
            }
        }
        if let Some(idx) = found_cluster_idx {
            let count = clusters[idx].1.len() as f32;
            clusters[idx].0 = (clusters[idx].0 * count + pos) / (count + 1.0);
            clusters[idx].1.push((name, type_));
        } else {
            clusters.push((pos, vec![(name, type_)]));
        }
    }

    // Draw composed overlays
    for (mut pos, items) in clusters {
        let mut street_name = None;
        let mut bus_routes = std::collections::BTreeSet::new();
        let mut business_names = Vec::new();

        for (name, type_) in items {
            if type_ == "road" {
                street_name = Some(name);
            } else if type_ == "route" {
                bus_routes.insert(name);
            } else if type_ == "business" {
                if !business_names.contains(&name) {
                    business_names.push(name);
                }
            }
        }

        let mut parts = Vec::new();
        if let Some(street) = street_name {
            if !bus_routes.is_empty() {
                let routes_str = bus_routes.into_iter().collect::<Vec<_>>().join(", ");
                parts.push(format!("{} ({})", street, routes_str));
            } else {
                parts.push(street);
            }
        } else if !bus_routes.is_empty() {
            let routes_str = bus_routes.into_iter().collect::<Vec<_>>().join(", ");
            parts.push(routes_str);
        }

        for biz in &business_names {
            parts.push(biz.clone());
        }

        if parts.is_empty() {
            continue;
        }

        let final_label = parts.join(" | ");

        pos.y = query_point_ground_y(pos.x, pos.z, &map_tree, &spatial_query) + 2.0;

        if let Ok(p_center) = camera.world_to_viewport(camera_transform, pos) {
            let camera_right = camera_transform.right();
            let sphere_radius = 3.0;
            if let Ok(p_edge) =
                camera.world_to_viewport(camera_transform, pos + camera_right * sphere_radius)
            {
                let r_screen = p_center.distance(p_edge);
                let font_size = (r_screen * 2.0).clamp(10.0, 16.0); // Made font size smaller (max 16.0 instead of 24.0)

                let unique_id = format!("osm_lbl_{:?}_{}", pos, final_label);
                egui::Area::new(egui::Id::new(unique_id))
                    .fixed_pos(egui::pos2(p_center.x - 30.0, p_center.y - font_size - 4.0)) // Centered better
                    .show(ctx, |ui| {
                        ui.label(
                            egui::RichText::new(&final_label)
                                .color(egui::Color32::from_rgb(255, 235, 120)) // Gold text
                                .size(font_size)
                                .strong()
                                .background_color(egui::Color32::from_rgba_premultiplied(
                                    0, 0, 0, 200,
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
    pub show_lanes: bool,
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
            show_lanes: false,
        }
    }
}

fn osm_overlay_ui_system(mut contexts: EguiContexts, mut osm_overlay: ResMut<OsmOverlayState>) {
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
    let mut show_lanes = osm_overlay.show_lanes;

    egui::Window::new("OSM Overlays")
        .open(&mut show_window)
        .show(ctx, |ui| {
            ui.checkbox(&mut show_roads, "Show Roads (Streets)");
            ui.checkbox(&mut show_lanes, "Show Street Lanes");
            ui.checkbox(
                &mut show_bus_routes,
                "Show Public Transport Routes (Bus/Tram/Trolley)",
            );
            ui.checkbox(
                &mut show_businesses,
                "Show Businesses (Shops/Amenities/Offices/Craft)",
            );
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
    osm_overlay.show_lanes = show_lanes;
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
    let bus_route_color = Color::srgb(0.1, 0.45, 0.9);
    let business_color = Color::srgb(0.9, 0.1, 0.6);
    let railway_color = Color::srgb(0.5, 0.5, 0.5);
    let waterway_color = Color::srgb(0.0, 0.3, 0.9);
    let building_color = Color::srgb(1.0, 0.5, 0.0);

    let query_y =
        |x: f32, z: f32| -> f32 { query_point_ground_y(x, z, &map_tree, &spatial_query) + 0.2 };

    // Draw parallel segments to create thick lines
    let draw_lines = |gizmos: &mut Gizmos, pts: &[Vec3], color: Color| {
        if pts.len() < 2 {
            return;
        }
        let mut grounded = Vec::with_capacity(pts.len());
        for pt in pts {
            grounded.push(Vec3::new(pt.x, query_y(pt.x, pt.z), pt.z));
        }
        for window in grounded.windows(2) {
            let p1 = window[0];
            let p2 = window[1];
            gizmos.line(p1, p2, color);

            // Left/Right parallel lines offset by 0.4 meters for thickness
            let diff = p2 - p1;
            let dir = diff.normalize_or_zero();
            let perp = Vec3::new(-dir.z, 0.0, dir.x).normalize_or_zero() * 0.4;

            gizmos.line(p1 - perp, p2 - perp, color);
            gizmos.line(p1 + perp, p2 + perp, color);
        }
    };

    let draw_dashed_line = |gizmos: &mut Gizmos, p1: Vec3, p2: Vec3, color: Color| {
        let diff = p2 - p1;
        let dist = diff.length();
        let dir = diff.normalize_or_zero();

        let dash_len = 4.0;
        let gap_len = 4.0;
        let step = dash_len + gap_len;

        let mut current = 0.0;
        while current < dist {
            let start = p1 + dir * current;
            let end = p1 + dir * (current + dash_len).min(dist);
            gizmos.line(start, end, color);
            current += step;
        }
    };

    let draw_polygon = |gizmos: &mut Gizmos, rings: &[Vec<Vec3>], color: Color| {
        for ring in rings {
            if ring.is_empty() {
                continue;
            }
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

    if osm_overlay.show_lanes {
        if let Some(features) = database.categories.get("roads") {
            let border_color = Color::srgb(0.7, 0.7, 0.7); // Solid gray borders
            let divider_color = Color::srgb(0.9, 0.9, 0.0); // Dashed yellow dividers

            for feature in features {
                let lanes = feature
                    .tags
                    .get("lanes")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(2);
                let lane_width = 3.0;
                let total_width = lanes as f32 * lane_width;

                match &feature.geometry {
                    FeatureGeometry::LineString(pts) => {
                        if pts.len() < 2 {
                            continue;
                        }
                        let mut grounded = Vec::with_capacity(pts.len());
                        for pt in pts {
                            grounded.push(Vec3::new(pt.x, query_y(pt.x, pt.z), pt.z));
                        }
                        for window in grounded.windows(2) {
                            let p1 = window[0];
                            let p2 = window[1];
                            let dir = (p2 - p1).normalize_or_zero();
                            let perp = Vec3::new(-dir.z, 0.0, dir.x).normalize_or_zero();

                            // Draw outer solid borders
                            let left_p1 = p1 - perp * (total_width / 2.0);
                            let left_p2 = p2 - perp * (total_width / 2.0);
                            gizmos.line(left_p1, left_p2, border_color);

                            let right_p1 = p1 + perp * (total_width / 2.0);
                            let right_p2 = p2 + perp * (total_width / 2.0);
                            gizmos.line(right_p1, right_p2, border_color);

                            // Draw dashed lane dividers
                            for i in 1..lanes {
                                let offset = (i as f32 * lane_width) - (total_width / 2.0);
                                let div_p1 = p1 + perp * offset;
                                let div_p2 = p2 + perp * offset;
                                draw_dashed_line(&mut gizmos, div_p1, div_p2, divider_color);
                            }
                        }
                    }
                    FeatureGeometry::MultiLineString(lines) => {
                        for pts in lines {
                            if pts.len() < 2 {
                                continue;
                            }
                            let mut grounded = Vec::with_capacity(pts.len());
                            for pt in pts {
                                grounded.push(Vec3::new(pt.x, query_y(pt.x, pt.z), pt.z));
                            }
                            for window in grounded.windows(2) {
                                let p1 = window[0];
                                let p2 = window[1];
                                let dir = (p2 - p1).normalize_or_zero();
                                let perp = Vec3::new(-dir.z, 0.0, dir.x).normalize_or_zero();

                                // Draw outer solid borders
                                let left_p1 = p1 - perp * (total_width / 2.0);
                                let left_p2 = p2 - perp * (total_width / 2.0);
                                gizmos.line(left_p1, left_p2, border_color);

                                let right_p1 = p1 + perp * (total_width / 2.0);
                                let right_p2 = p2 + perp * (total_width / 2.0);
                                gizmos.line(right_p1, right_p2, border_color);

                                // Draw dashed lane dividers
                                for i in 1..lanes {
                                    let offset = (i as f32 * lane_width) - (total_width / 2.0);
                                    let div_p1 = p1 + perp * offset;
                                    let div_p2 = p2 + perp * offset;
                                    draw_dashed_line(&mut gizmos, div_p1, div_p2, divider_color);
                                }
                            }
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
                let route_type = feature.tags.get("route").map(|s| s.as_str()).unwrap_or("");
                let is_public_transport = route_type == "bus"
                    || route_type == "tram"
                    || route_type == "trolleybus"
                    || route_type == "trolley"
                    || route_type == "tramway";
                if !is_public_transport {
                    continue;
                }

                let color = if route_type == "bus" {
                    bus_route_color
                } else if route_type == "tram" || route_type == "tramway" {
                    Color::srgb(0.9, 0.3, 0.15) // Red/Orange for Trams
                } else if route_type == "trolleybus" || route_type == "trolley" {
                    Color::srgb(0.65, 0.15, 0.85) // Purple for Trolleybuses
                } else {
                    Color::srgb(0.9, 0.8, 0.1) // Yellow for others
                };

                match &feature.geometry {
                    FeatureGeometry::LineString(pts) => {
                        draw_lines(&mut gizmos, pts, color);
                    }
                    FeatureGeometry::MultiLineString(lines) => {
                        for pts in lines {
                            draw_lines(&mut gizmos, pts, color);
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
        info!(
            "Successfully found Bus 335 route with {} points! Initializing movement.",
            bus_route_points.len()
        );
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
