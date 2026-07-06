use bevy::prelude::*;
use std::collections::HashMap;

#[derive(Resource, Default)]
pub struct TrafficRoadGraph {
    pub segments: Vec<RoadSegment>,
    /// Quantized endpoint -> segment indices touching it
    pub node_index: HashMap<IVec2, Vec<usize>>,
    pub built: bool,
}

#[derive(Clone, Debug)]
pub struct RoadSegment {
    pub points: Vec<Vec3>,
    pub length: f32,
}

pub fn quantize(p: Vec3) -> IVec2 {
    IVec2::new(p.x.round() as i32, p.z.round() as i32)
}

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

fn process_points(
    points: &[Vec3],
    segments: &mut Vec<RoadSegment>,
    node_index: &mut HashMap<IVec2, Vec<usize>>,
) {
    if points.len() < 2 {
        return;
    }

    let length: f32 = points
        .windows(2)
        .map(|w| w[0].distance(w[1]))
        .sum();

    if length < 20.0 {
        return;
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
