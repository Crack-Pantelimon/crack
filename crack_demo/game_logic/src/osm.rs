use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RawFeatureGeometry {
    Point((f64, f64)), // (lat, lon)
    LineString(Vec<(f64, f64)>),
    MultiLineString(Vec<Vec<(f64, f64)>>),
    Polygon(Vec<Vec<(f64, f64)>>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawGeoJsonFeature {
    pub id: Option<i64>,
    pub osm_type: String,
    pub name: Option<String>,
    pub tags: BTreeMap<String, String>,
    pub raw_geometry: RawFeatureGeometry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FeatureGeometry {
    Point(Vec3),
    LineString(Vec<Vec3>),
    MultiLineString(Vec<Vec<Vec3>>),
    Polygon(Vec<Vec<Vec3>>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OsmDataResult {
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
