use crate::map::{MapTreeData, MapTreeNodeInfo};
use glam::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct GeoBBox {
    pub north: f64,
    pub south: f64,
    pub west: f64,
    pub east: f64,
}

impl GeoBBox {
    pub fn contains(&self, lat: f64, lon: f64) -> bool {
        lat >= self.south && lat <= self.north && lon >= self.west && lon <= self.east
    }
}

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

#[derive(Debug, Clone)]
pub struct ProjectionRef {
    pub ref_point: Vec3,
    pub rot_matrix: [Vec3; 3],
}

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

pub fn parse_bbox_from_txt(text: &str) -> Option<(f32, f32)> {
    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    if lines.len() != 2 {
        return None;
    }
    let p1: Vec<f32> = lines[0]
        .split(',')
        .filter_map(|s| s.trim().parse::<f32>().ok())
        .collect();
    let p2: Vec<f32> = lines[1]
        .split(',')
        .filter_map(|s| s.trim().parse::<f32>().ok())
        .collect();
    if p1.len() != 2 || p2.len() != 2 {
        return None;
    }
    let lat_deg = (p1[0] + p2[0]) / 2.0;
    let lon_deg = (p1[1] + p2[1]) / 2.0;
    Some((lat_deg, lon_deg))
}

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
