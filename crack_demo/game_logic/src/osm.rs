use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// OSM geometry in raw latitude/longitude coordinates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RawFeatureGeometry {
    /// Single point as `(lat, lon)`.
    Point((f64, f64)),
    /// Open polyline as `(lat, lon)` vertices.
    LineString(Vec<(f64, f64)>),
    /// Multiple polylines as `(lat, lon)` vertex lists.
    MultiLineString(Vec<Vec<(f64, f64)>>),
    /// Polygon rings as `(lat, lon)` vertex lists (first ring is outer).
    Polygon(Vec<Vec<(f64, f64)>>),
}

/// One OSM feature before world-space projection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawGeoJsonFeature {
    /// OSM object id when present.
    pub id: Option<i64>,
    /// OSM element type (`node`, `way`, or `relation`).
    pub osm_type: String,
    /// Display name from OSM tags when present.
    pub name: Option<String>,
    /// Raw OSM tag key/value pairs.
    pub tags: BTreeMap<String, String>,
    /// Geometry in geographic coordinates.
    pub raw_geometry: RawFeatureGeometry,
}

/// OSM geometry projected into Bevy world space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FeatureGeometry {
    /// Single world-space point.
    Point(Vec3),
    /// Open polyline in world space.
    LineString(Vec<Vec3>),
    /// Multiple polylines in world space.
    MultiLineString(Vec<Vec<Vec3>>),
    /// Polygon rings in world space (first ring is outer).
    Polygon(Vec<Vec<Vec3>>),
}

/// One OSM feature after projection with a precomputed bounds summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoJsonFeature {
    /// OSM object id when present.
    pub id: Option<i64>,
    /// OSM element type (`node`, `way`, or `relation`).
    pub osm_type: String,
    /// Display name from OSM tags when present.
    pub name: Option<String>,
    /// OSM tag key/value pairs.
    pub tags: BTreeMap<String, String>,
    /// Geometry in world space.
    pub geometry: FeatureGeometry,
    /// Centroid of the projected geometry.
    pub center: Vec3,
    /// Axis-aligned minimum corner of the feature bounds.
    pub bbox_min: Vec3,
    /// Axis-aligned maximum corner of the feature bounds.
    pub bbox_max: Vec3,
}

/// OSM fetch result grouped by category name.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OsmDataResult {
    /// Features keyed by category label (e.g. `roads`, `buildings`).
    pub categories: BTreeMap<String, Vec<GeoJsonFeature>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test as test;

    #[test]
    fn smoke_osm_data_result_serde_round_trip() {
        let mut tags = BTreeMap::new();
        tags.insert("highway".to_string(), "residential".to_string());
        let feature = GeoJsonFeature {
            id: Some(42),
            osm_type: "way".to_string(),
            name: Some("Main St".to_string()),
            tags,
            geometry: FeatureGeometry::LineString(vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 1.0),
            ]),
            center: Vec3::new(0.5, 0.0, 0.5),
            bbox_min: Vec3::new(0.0, 0.0, 0.0),
            bbox_max: Vec3::new(1.0, 0.0, 1.0),
        };
        let mut categories = BTreeMap::new();
        categories.insert("roads".to_string(), vec![feature]);
        let result = OsmDataResult { categories };
        let json = serde_json::to_string(&result).unwrap();
        let back: OsmDataResult = serde_json::from_str(&json).unwrap();
        assert_eq!(serde_json::to_string(&back).unwrap(), json);
    }
}
