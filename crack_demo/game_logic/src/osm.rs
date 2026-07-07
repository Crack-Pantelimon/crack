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
