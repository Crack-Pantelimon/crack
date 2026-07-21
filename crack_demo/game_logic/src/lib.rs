//! Shared game logic: map tiles, LOD, OSM features, and worker fetch APIs.

/// RPC argument types and the client-facing API group declaration.
pub mod api;
/// Geographic projection, octant paths, and lat/lon helpers.
pub mod geo;
/// GLB fetch request/response types for character and weapon models.
pub mod glb;
/// Level-of-detail scoring, visibility gating, and split/merge requests.
pub mod lod;
/// Map octree manifest types: tiles, nodes, bounding boxes, and summaries.
pub mod map;
/// Game-specific chat room types for realtime gameplay sync.
pub mod network;
/// OSM GeoJSON feature types in raw and world-projected form.
pub mod osm;
/// Pedestrian animation manifest types.
pub mod pedestrian;
/// Map tile GLB fetch types and extracted collider meshes.
pub mod tile;
/// Weapon stat manifest types.
pub mod weapon;

#[cfg(feature = "worker")]
/// Worker-side BVH occluder caches and visibility ray tests.
pub mod visibility;
#[cfg(feature = "worker")]
/// Worker implementations of the `GameLogicApiGroup` fetch handlers.
pub mod worker;
