use crate::map::{BBox, MapTreeData, MapTreeNodeInfo};
use glam::Vec3;

/// Latitude/longitude bounding box in WGS84 degrees.
#[derive(Clone, Copy, Debug)]
pub struct GeoBBox {
    /// Northern latitude bound in degrees.
    pub north: f64,
    /// Southern latitude bound in degrees.
    pub south: f64,
    /// Western longitude bound in degrees.
    pub west: f64,
    /// Eastern longitude bound in degrees.
    pub east: f64,
}

impl GeoBBox {
    /// Returns true when `(lat, lon)` lies inside this box.
    pub fn contains(&self, lat: f64, lon: f64) -> bool {
        lat >= self.south && lat <= self.north && lon >= self.west && lon <= self.east
    }
}

/// Decodes a map octant path into its geographic bounding box.

pub fn octant_path_to_geobbox(path: &str) -> Option<GeoBBox> {
    if path.len() < 2 {
        return None;
    }
    let first_two = &path[0..2];
    let mut box_ = match first_two {
        "02" => GeoBBox {
            north: 0.0,
            south: -90.0,
            west: -180.0,
            east: -90.0,
        },
        "03" => GeoBBox {
            north: 0.0,
            south: -90.0,
            west: -90.0,
            east: 0.0,
        },
        "12" => GeoBBox {
            north: 0.0,
            south: -90.0,
            west: 0.0,
            east: 90.0,
        },
        "13" => GeoBBox {
            north: 0.0,
            south: -90.0,
            west: 90.0,
            east: 180.0,
        },
        "20" => GeoBBox {
            north: 90.0,
            south: 0.0,
            west: -180.0,
            east: -90.0,
        },
        "21" => GeoBBox {
            north: 90.0,
            south: 0.0,
            west: -90.0,
            east: 0.0,
        },
        "30" => GeoBBox {
            north: 90.0,
            south: 0.0,
            west: 0.0,
            east: 90.0,
        },
        "31" => GeoBBox {
            north: 90.0,
            south: 0.0,
            west: 90.0,
            east: 180.0,
        },
        _ => return None,
    };

    for ch in path[2..].chars() {
        let digit = ch.to_digit(10)? as i32;
        let lat_bit = (digit >> 1) & 1; // bit 1
        let lon_bit = digit & 1; // bit 0

        let mid_lat = (box_.north + box_.south) / 2.0;
        let mid_lon = (box_.west + box_.east) / 2.0;

        if lat_bit == 0 {
            box_.north = mid_lat;
        } else {
            box_.south = mid_lat;
        }

        if box_.north == 90.0 || box_.south == -90.0 {
            continue;
        }

        if lon_bit == 0 {
            box_.east = mid_lon;
        } else {
            box_.west = mid_lon;
        }
    }

    Some(box_)
}

/// Finds the deepest map-tree node whose geo bbox contains `(lat, lon)`.

pub fn find_tile_for_lat_lon<'a>(
    lat: f64,
    lon: f64,
    map_tree: &'a MapTreeData,
) -> Option<&'a MapTreeNodeInfo> {
    // Start from the roots
    let matching_roots: Vec<&crate::map::MapTreeNodePath> = map_tree
        .roots
        .iter()
        .filter(|node_path| {
            octant_path_to_geobbox(&node_path.0)
                .map(|geobbox| geobbox.contains(lat, lon))
                .unwrap_or(false)
        })
        .collect();

    if matching_roots.is_empty() {
        return None;
    }

    let mut current_node_path = matching_roots[0].clone();

    loop {
        let level = current_node_path.0.len();
        if level >= 20 {
            break;
        }

        let Some(children_set) = map_tree.children.get(&current_node_path) else {
            break;
        };

        if children_set.is_empty() {
            break;
        }

        let matching_children: Vec<&crate::map::MapTreeNodePath> = children_set
            .iter()
            .filter(|child_path| {
                octant_path_to_geobbox(&child_path.0)
                    .map(|geobbox| geobbox.contains(lat, lon))
                    .unwrap_or(false)
            })
            .collect();

        if matching_children.is_empty() {
            break;
        } else if matching_children.len() == 1 {
            current_node_path = matching_children[0].clone();
        } else {
            // Pick biggest by diagonal
            let mut best_child = None;
            let mut max_diagonal: f32 = -1.0;

            for child_path in matching_children {
                if let Some(node_info) = map_tree.all_nodes.get(child_path) {
                    let diag = (node_info.bbox.max - node_info.bbox.min).length();
                    if diag > max_diagonal {
                        max_diagonal = diag;
                        best_child = Some(child_path);
                    }
                }
            }

            if let Some(child) = best_child {
                current_node_path = child.clone();
            }
            break;
        }
    }

    map_tree.all_nodes.get(&current_node_path)
}

/// ECEF reference point and ENU rotation used for lat/lon projection.
#[derive(Debug, Clone)]
pub struct ProjectionRef {
    /// ECEF origin for local tangent-plane projection.
    pub ref_point: Vec3,
    /// East-north-up basis as three column vectors.
    pub rot_matrix: [Vec3; 3],
}

/// Builds the ENU rotation matrix for an ECEF reference point.

pub fn get_enu_rotation_matrix(ref_point: Vec3) -> [Vec3; 3] {
    let rx = ref_point.x as f64;
    let ry = ref_point.y as f64;
    let rz = ref_point.z as f64;
    let l = (rx * rx + ry * ry + rz * rz).sqrt();
    if l == 0.0 {
        return [Vec3::X, Vec3::Y, Vec3::Z];
    }
    let u = Vec3::new((rx / l) as f32, (ry / l) as f32, (rz / l) as f32);

    let xy_len = (rx * rx + ry * ry).sqrt();
    let e = if xy_len > 0.0 {
        Vec3::new((-ry / xy_len) as f32, (rx / xy_len) as f32, 0.0)
    } else {
        Vec3::new(1.0, 0.0, 0.0)
    };

    let n = u.cross(e);
    [e, n, u]
}

/// Converts WGS84 lat/lon (degrees) to ECEF meters.

pub fn lat_lon_to_ecef(lat_deg: f32, lon_deg: f32) -> Vec3 {
    let lat = (lat_deg as f64).to_radians();
    let lon = (lon_deg as f64).to_radians();
    let a = 6378137.0;
    let e2 = 0.00669437999014;
    let n = a / (1.0 - e2 * lat.sin().powi(2)).sqrt();
    let x = n * lat.cos() * lon.cos();
    let y = n * lat.cos() * lon.sin();
    let z = n * (1.0 - e2) * lat.sin();
    Vec3::new(x as f32, y as f32, z as f32)
}

/// Projects lat/lon into Bevy world space relative to a tangent frame.

pub fn lat_lon_to_bevy(
    lat_deg: f32,
    lon_deg: f32,
    ref_point: Vec3,
    rot_matrix: &[Vec3; 3],
) -> Vec3 {
    let pt_ecef = lat_lon_to_ecef(lat_deg, lon_deg);
    let rel_ecef = pt_ecef - ref_point;
    let east = rel_ecef.dot(rot_matrix[0]);
    let north = rel_ecef.dot(rot_matrix[1]);
    let up = rel_ecef.dot(rot_matrix[2]);

    Vec3::new(east, up, -north)
}

/// Parse `zone-bbox.txt`: two `lat,lon` corner lines defining the map extent.
pub fn parse_geo_bbox_from_txt(text: &str) -> Option<GeoBBox> {
    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    if lines.len() != 2 {
        return None;
    }
    let mut lats = [0.0f64; 2];
    let mut lons = [0.0f64; 2];
    for (i, line) in lines.iter().enumerate() {
        let parts: Vec<f64> = line
            .split(',')
            .filter_map(|s| s.trim().parse::<f64>().ok())
            .collect();
        if parts.len() != 2 {
            return None;
        }
        lats[i] = parts[0];
        lons[i] = parts[1];
    }
    Some(GeoBBox {
        north: lats[0].max(lats[1]),
        south: lats[0].min(lats[1]),
        west: lons[0].min(lons[1]),
        east: lons[0].max(lons[1]),
    })
}

/// Returns the geographic center `(lat, lon)` from a `zone-bbox.txt` file.
pub fn parse_bbox_from_txt(text: &str) -> Option<(f32, f32)> {
    let geo = parse_geo_bbox_from_txt(text)?;
    Some((
        ((geo.north + geo.south) / 2.0) as f32,
        ((geo.west + geo.east) / 2.0) as f32,
    ))
}

/// Returns the geographic center `(lat, lon)` from a `zone-bbox.txt` file.

/// Y extent from the union of root-tile xyz bboxes in the manifest.
fn root_tile_y_extent(tree: &MapTreeData) -> (f32, f32) {
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for root_path in &tree.roots {
        let Some(node) = tree.all_nodes.get(root_path) else {
            continue;
        };
        min_y = min_y.min(node.bbox.min.y).min(node.bbox.max.y);
        max_y = max_y.max(node.bbox.min.y).max(node.bbox.max.y);
    }
    if min_y.is_finite() && max_y.is_finite() {
        (min_y, max_y)
    } else {
        (0.0, 0.0)
    }
}

/// Map playable extent: X/Z from the four cardinal corners of the lat/lon bbox, Y from root tiles.
pub fn apply_geo_extent_bbox(tree: &mut MapTreeData, geo_bbox: &GeoBBox) {
    let (min_y, max_y) = root_tile_y_extent(tree);

    let lat = ((geo_bbox.north + geo_bbox.south) / 2.0) as f32;
    let lon = ((geo_bbox.west + geo_bbox.east) / 2.0) as f32;
    let ref_point = lat_lon_to_ecef(lat, lon);
    let projection = ProjectionRef {
        ref_point,
        rot_matrix: get_enu_rotation_matrix(ref_point),
    };

    let corners = [
        (geo_bbox.north, geo_bbox.west),
        (geo_bbox.north, geo_bbox.east),
        (geo_bbox.south, geo_bbox.west),
        (geo_bbox.south, geo_bbox.east),
    ];

    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_z = f32::INFINITY;
    let mut max_z = f32::NEG_INFINITY;

    for (lat, lon) in corners {
        let p = project_point(lat, lon, tree, &projection);
        min_x = min_x.min(p.x);
        max_x = max_x.max(p.x);
        min_z = min_z.min(p.z);
        max_z = max_z.max(p.z);
    }

    tree.bbox = BBox {
        min: Vec3::new(min_x, min_y, min_z),
        max: Vec3::new(max_x, max_y, max_z),
    };
}

/// Projects one lat/lon point into world space using the map tree when possible.

pub fn project_point(
    lat: f64,
    lon: f64,
    map_tree: &MapTreeData,
    coord_res: &ProjectionRef,
) -> Vec3 {
    if let Some(node_info) = find_tile_for_lat_lon(lat, lon, map_tree) {
        if let Some(geobbox) = octant_path_to_geobbox(&node_info.path.0) {
            let width = geobbox.east - geobbox.west;
            let height = geobbox.north - geobbox.south;
            if width > 0.0 && height > 0.0 {
                let u = (lon - geobbox.west) / width;
                let v = (lat - geobbox.south) / height;

                let x =
                    node_info.bbox.min.x + u as f32 * (node_info.bbox.max.x - node_info.bbox.min.x);
                let z =
                    node_info.bbox.max.z - v as f32 * (node_info.bbox.max.z - node_info.bbox.min.z);
                let y = node_info.bbox.min.y + 2.0;
                return Vec3::new(x, y, z);
            }
        }
    }

    lat_lon_to_bevy(
        lat as f32,
        lon as f32,
        coord_res.ref_point,
        &coord_res.rot_matrix,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test as test;

    #[test]
    fn smoke_geo_coordinate_invariants() {
        // Root octant "02" covers the south-west quadrant of the globe.
        let bb = octant_path_to_geobbox("02").unwrap();
        assert_eq!(bb.north, 0.0);
        assert_eq!(bb.south, -90.0);
        assert_eq!(bb.west, -180.0);
        assert_eq!(bb.east, -90.0);
        assert!(bb.contains(-45.0, -135.0));
        assert!(!bb.contains(45.0, -135.0));

        // Too-short or unknown paths yield None.
        assert!(octant_path_to_geobbox("0").is_none());
        assert!(octant_path_to_geobbox("99").is_none());

        // Equator / prime meridian maps to (earth_radius, 0, 0).
        let p = lat_lon_to_ecef(0.0, 0.0);
        assert!((p.x - 6378137.0).abs() < 1.0);
        assert!(p.y.abs() < 1e-3);
        assert!(p.z.abs() < 1e-3);

        // Any lat/lon lands at roughly earth radius from the origin.
        let q = lat_lon_to_ecef(52.52, 13.405);
        assert!((q.length() - 6378137.0).abs() < 30_000.0);
    }
}
