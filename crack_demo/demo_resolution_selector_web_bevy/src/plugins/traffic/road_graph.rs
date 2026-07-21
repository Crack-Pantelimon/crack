use bevy::prelude::*;
use std::collections::HashMap;

/// traffic road graph.
#[derive(Resource, Default)]
pub struct TrafficRoadGraph {
/// segments field.
    pub segments: Vec<RoadSegment>,
    /// Quantized endpoint -> segment indices touching it
    pub node_index: HashMap<IVec2, Vec<usize>>,
/// built field.
    pub built: bool,
}

/// road segment.
#[derive(Clone, Debug)]
pub struct RoadSegment {
/// points field.
    pub points: Vec<Vec3>,
/// length field.
    pub length: f32,
}

/// quantize.
pub fn quantize(p: Vec3) -> IVec2 {
    IVec2::new(p.x.round() as i32, p.z.round() as i32)
}

/// build road graph.
pub fn build_road_graph(
    database: Res<crate::plugins::geojson::GeoJsonDatabase>,
    mut graph: ResMut<TrafficRoadGraph>,
) {
    if graph.built || !database.parsed {
        return;
    }

    info!("TrafficRoadGraph: starting build from GeoJsonDatabase...");
    let mut segments = Vec::new();
    let mut node_index: HashMap<IVec2, Vec<usize>> = HashMap::new();

    if let Some(roads) = database.categories.get("roads") {
        for feature in roads {
            match &feature.geometry {
                crate::plugins::geojson::FeatureGeometry::LineString(points) => {
                    process_points(points, &mut segments, &mut node_index);
                }
                crate::plugins::geojson::FeatureGeometry::MultiLineString(lines) => {
                    for points in lines {
                        process_points(points, &mut segments, &mut node_index);
                    }
                }
                _ => {}
            }
        }
    }

    graph.segments = segments;
    graph.node_index = node_index;
    graph.built = true;

    info!(
        "TrafficRoadGraph: built with {} segments and {} node junctions.",
        graph.segments.len(),
        graph.node_index.len()
    );
}

/// Maximum road segment inclination in degrees. Roads steeper than this (in any sub-segment)
/// are discarded — both from the traffic graph *and* the OSM road overlay — to remove broken /
/// steep OSM road markers. Kept to a few degrees so only genuinely flat, walkable/drivable
/// streets survive.
pub const MAX_ROAD_INCLINATION_DEG: f32 = 6.0;

/// True if any sub-segment of the polyline is steeper than [`MAX_ROAD_INCLINATION_DEG`] (or
/// degenerate). Shared by the traffic graph builder and the road overlay so both agree on which
/// roads exist.
pub fn road_too_steep(points: &[Vec3]) -> bool {
    let max_slope = MAX_ROAD_INCLINATION_DEG.to_radians().tan();
    for w in points.windows(2) {
        let dx = (w[1].x - w[0].x).hypot(w[1].z - w[0].z); // horizontal distance
        let dy = (w[1].y - w[0].y).abs(); // vertical distance
        if dx < 0.01 || dy / dx > max_slope {
            return true;
        }
    }
    false
}

fn process_points(
    points: &[Vec3],
    segments: &mut Vec<RoadSegment>,
    node_index: &mut HashMap<IVec2, Vec<usize>>,
) {
    if points.len() < 2 {
        return;
    }

    let length: f32 = points.windows(2).map(|w| w[0].distance(w[1])).sum();

    if length < 20.0 {
        return;
    }

    // reject segments where any sub-segment is steeper than threshold
    if road_too_steep(points) {
        return; // entire segment is discarded
    }

    let seg_idx = segments.len();
    segments.push(RoadSegment {
        points: points.to_vec(),
        length,
    });

    let first_node = quantize(points[0]);
    let last_node = quantize(*points.last().unwrap());

    node_index.entry(first_node).or_default().push(seg_idx);
    node_index.entry(last_node).or_default().push(seg_idx);
}

/// reroute mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RerouteMode {
/// Documented public item.
    ClosestAngle(Vec3), // incoming forward dir
/// random variant.
    Random,
}

/// Given the node we arrived at, the segment we came from, and a reroute mode,
/// pick a connected segment (excluding `from_seg`) and return its points oriented *away* from `node`.
pub fn pick_continuation(
    graph: &TrafficRoadGraph,
    node: IVec2,
    from_seg: usize,
    mode: RerouteMode,
) -> Option<(usize, Vec<Vec3>)> {
    let matching_segs = graph.node_index.get(&node)?;
    let candidates: Vec<usize> = matching_segs
        .iter()
        .copied()
        .filter(|&idx| idx != from_seg)
        .collect();

    if candidates.is_empty() {
        return None;
    }

    let chosen_seg_idx = match mode {
        RerouteMode::Random => {
            use rand::seq::IndexedRandom;
            *candidates.choose(&mut rand::rng())?
        }
        RerouteMode::ClosestAngle(incoming_dir) => {
            let mut best_seg = candidates[0];
            let mut best_dot = f32::MIN;

            for &seg_idx in &candidates {
                let seg = &graph.segments[seg_idx];
                if seg.points.len() < 2 {
                    continue;
                }
                let start_quant = quantize(seg.points[0]);
                let dir_out = if start_quant == node {
                    (seg.points[1] - seg.points[0]).normalize_or_zero()
                } else {
                    let len = seg.points.len();
                    (seg.points[len - 2] - seg.points[len - 1]).normalize_or_zero()
                };

                let dot = dir_out.dot(incoming_dir);
                if dot > best_dot {
                    best_dot = dot;
                    best_seg = seg_idx;
                }
            }
            best_seg
        }
    };

    let seg = &graph.segments[chosen_seg_idx];
    if seg.points.len() < 2 {
        return None;
    }

    let start_quant = quantize(seg.points[0]);
    let points = if start_quant == node {
        seg.points.clone()
    } else {
        seg.points.iter().cloned().rev().collect()
    };

    Some((chosen_seg_idx, points))
}
