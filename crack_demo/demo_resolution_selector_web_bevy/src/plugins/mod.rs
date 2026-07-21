/// Spatial audio effects and playback helpers.
pub mod audio;
/// Vehicle spawn, driving controls, and camera follow.
pub mod cars_driving;
/// Procedural sky, clouds, and precipitation rendering.
pub mod cloud_sky;
/// Crack map loading, LOD, and manifest orchestration.
pub mod crack_plugin;
/// Entity inspection and debug picking overlay.
pub mod debug_picker;
/// Free-fly camera controls for sandbox scenes.
pub mod game_freecam;
/// GeoJSON layer import and map overlay hooks.
pub mod geojson;
/// Default scene lighting, ground, and environment setup.
pub mod main_scene_plugin;
/// Terrain mesh, materials, minimap, and LOD editing.
pub mod map_plugin;
/// Multiplayer sync, chat, and network session glue.
pub mod network;
/// Transient tooltip and HUD notification UI.
pub mod notifications;
/// Pedestrian AI: factions, combat, and locomotion brains.
pub mod pedestrian_ai;
/// Pedestrian rigs, animation, and player controller.
pub mod pedestrians;
/// Avian3D physics world and collision layers.
pub mod physics_plugin;
/// High-level game state machine and transitions.
pub mod states;
/// Road graph traffic, drivers, and pedestrian crossings.
pub mod traffic;
/// Gun smoke, explosions, and billboard particle FX.
pub mod visual_fx;
/// Weapon manifests, attachment, and shooting systems.
pub mod weapons;
