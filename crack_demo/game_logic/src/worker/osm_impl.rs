use crate::api::FetchArgs;
use crate::geo::{
    ProjectionRef, get_enu_rotation_matrix, lat_lon_to_ecef, parse_bbox_from_txt, project_point,
};
use crate::osm::{
    FeatureGeometry, GeoJsonFeature, OsmDataResult, RawFeatureGeometry, RawGeoJsonFeature,
};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::RwLock;

static OSM_CACHE: RwLock<Option<Arc<OsmDataResult>>> = RwLock::const_new(None);

/// Fetches OSM features for the map extent and projects them to world space.
pub async fn fetch_osm_data(args: FetchArgs) -> anyhow::Result<OsmDataResult> {
    {
        let guard = OSM_CACHE.read().await;
        if let Some(cache) = &*guard {
            return Ok((**cache).clone());
        }
    }

    let mut guard = OSM_CACHE.write().await;
    if let Some(cache) = &*guard {
        return Ok((**cache).clone());
    }

    // Need manifest first to project points
    let manifest = super::manifest_impl::get_manifest_cache().await?;

    let bbox_url = format!("{}/3d_data_v2/data_in/zone-bbox.txt", args.base_url);
    let list_url = format!("{}/3d_data_v2/data_osm/original/_list.txt", args.base_url);

    let bbox_text = super::http::http_get_text(&bbox_url).await?;
    let list_text = super::http::http_get_text(&list_url).await?;

    let Some((lat, lon)) = parse_bbox_from_txt(&bbox_text) else {
        anyhow::bail!("Failed to parse zone-bbox.txt");
    };

    let ref_point = lat_lon_to_ecef(lat, lon);
    let rot_matrix = get_enu_rotation_matrix(ref_point);
    let projection = ProjectionRef {
        ref_point,
        rot_matrix,
    };

    let mut files = Vec::new();
    for line in list_text.lines() {
        let line = line.trim();
        if !line.is_empty() {
            let category_name = line.replace(".geojson", "");
            files.push((category_name, line.to_string()));
        }
    }

    let mut result_categories = BTreeMap::new();

    for (category_name, file_name) in files {
        let file_url = format!(
            "{}/3d_data_v2/data_osm/original/{}",
            args.base_url, file_name
        );
        tracing::info!("Worker loading GeoJSON file: {}", file_url);

        let geojson_text = match super::http::http_get_text(&file_url).await {
            Ok(text) => text,
            Err(e) => {
                tracing::error!("Failed to load GeoJSON file {}: {:?}", file_url, e);
                continue;
            }
        };

        let parsed_json: serde_json::Value = match serde_json::from_str(&geojson_text) {
            Ok(val) => val,
            Err(e) => {
                tracing::error!("Failed to parse JSON for {}: {:?}", category_name, e);
                continue;
            }
        };

        let mut features = Vec::new();
        if let Some(features_arr) = parsed_json.get("features").and_then(|v| v.as_array()) {
            for feat_val in features_arr {
                if let Some(raw_feat) = parse_raw_geojson_feature(feat_val) {
                    if let Some(proj_feat) = project_feature(raw_feat, &manifest, &projection) {
                        features.push(proj_feat);
                    }
                }
            }
        }

        if !features.is_empty() {
            result_categories.insert(category_name, features);
        }
    }

    let result = OsmDataResult {
        categories: result_categories,
    };
    let arc = Arc::new(result.clone());
    *guard = Some(arc);

    Ok(result)
}

fn project_feature(
    raw: RawGeoJsonFeature,
    map_tree: &crate::map::MapTreeData,
    coord_res: &ProjectionRef,
) -> Option<GeoJsonFeature> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut min_z = f32::INFINITY;
    let mut max_x = -f32::INFINITY;
    let mut max_y = -f32::INFINITY;
    let mut max_z = -f32::INFINITY;

    let mut update_bounds = |pt: glam::Vec3| {
        if pt.x < min_x {
            min_x = pt.x;
        }
        if pt.x > max_x {
            max_x = pt.x;
        }
        if pt.y < min_y {
            min_y = pt.y;
        }
        if pt.y > max_y {
            max_y = pt.y;
        }
        if pt.z < min_z {
            min_z = pt.z;
        }
        if pt.z > max_z {
            max_z = pt.z;
        }
    };

    let proj = match raw.raw_geometry {
        RawFeatureGeometry::Point((lat, lon)) => {
            let p = project_point(lat, lon, map_tree, coord_res);
            update_bounds(p);
            FeatureGeometry::Point(p)
        }
        RawFeatureGeometry::LineString(pts) => {
            let mut line = Vec::new();
            for (lat, lon) in pts {
                let p = project_point(lat, lon, map_tree, coord_res);
                update_bounds(p);
                line.push(p);
            }
            FeatureGeometry::LineString(line)
        }
        RawFeatureGeometry::MultiLineString(lines) => {
            let mut result = Vec::new();
            for pts in lines {
                let mut line = Vec::new();
                for (lat, lon) in pts {
                    let p = project_point(lat, lon, map_tree, coord_res);
                    update_bounds(p);
                    line.push(p);
                }
                result.push(line);
            }
            FeatureGeometry::MultiLineString(result)
        }
        RawFeatureGeometry::Polygon(rings) => {
            let mut result = Vec::new();
            for pts in rings {
                let mut ring = Vec::new();
                for (lat, lon) in pts {
                    let p = project_point(lat, lon, map_tree, coord_res);
                    update_bounds(p);
                    ring.push(p);
                }
                result.push(ring);
            }
            FeatureGeometry::Polygon(result)
        }
    };

    let center = glam::Vec3::new(
        (min_x + max_x) / 2.0,
        (min_y + max_y) / 2.0,
        (min_z + max_z) / 2.0,
    );

    Some(GeoJsonFeature {
        id: raw.id,
        osm_type: raw.osm_type,
        name: raw.name,
        tags: raw.tags,
        geometry: proj,
        center,
        bbox_min: glam::Vec3::new(min_x, min_y, min_z),
        bbox_max: glam::Vec3::new(max_x, max_y, max_z),
    })
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
    let id = properties
        .get("id")
        .and_then(|v| v.as_i64())
        .or_else(|| tags.get("id").and_then(|s| s.parse::<i64>().ok()));
    let osm_type = properties
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("node")
        .to_string();

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
        "MultiLineString" => {
            let arr = coords.as_array()?;
            let mut lines = Vec::new();
            for line_val in arr {
                let line_arr = line_val.as_array()?;
                let mut line = Vec::new();
                for pt_val in line_arr {
                    let pt_arr = pt_val.as_array()?;
                    let lon = pt_arr.get(0)?.as_f64()?;
                    let lat = pt_arr.get(1)?.as_f64()?;
                    line.push((lat, lon));
                }
                lines.push(line);
            }
            RawFeatureGeometry::MultiLineString(lines)
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
