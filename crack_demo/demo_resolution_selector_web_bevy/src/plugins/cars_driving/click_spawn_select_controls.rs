use crate::plugins::{
    cars_driving::{
        car_info::{get_car_asset, get_random_car_type},
        driving_plugin::{CarDriveState, GamePhysicsLayer},
    },
    states::GameControlState,
};
use bevy::prelude::*;
use bevy_egui::EguiContexts;

pub fn handle_click_raycast_spawn_car(
    mut commands: Commands,
    mouse_button: Res<ButtonInput<MouseButton>>,
    window_query: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    spatial_query: avian3d::prelude::SpatialQuery,
    mut contexts: EguiContexts,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui() {
        return;
    }

    if mouse_button.just_pressed(MouseButton::Right) {
        let Ok(window) = window_query.single() else {
            return;
        };
        if let Some(cursor_pos) = window.cursor_position() {
            let Ok((camera, camera_transform)) = camera_query.single() else {
                return;
            };

            if let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) {
                if let Some(hit) = spatial_query.cast_ray(
                    ray.origin,
                    ray.direction,
                    10000.0,
                    true,
                    &avian3d::prelude::SpatialQueryFilter::default(),
                ) {
                    let hit_point = ray.origin + *ray.direction * hit.distance;
                    // lod_state.reference_points.push(hit_point);
                    info!("Spawn car at {:?}", hit_point);
                    commands.trigger(SpawnCarRequestEvent {
                        position: hit_point,
                        car_type: get_random_car_type().to_string(),
                    });
                }
            }
        }
    }
}

#[derive(Event)]
pub struct SpawnCarRequestEvent {
    pub position: Vec3,
    pub car_type: String,
}

#[derive(Component)]
pub struct Car {
    pub _car_type: String,
}

pub fn spawn_car_request_event_observer(
    spawn_car_event: On<SpawnCarRequestEvent>,
    asset_server: Res<AssetServer>,
    mut commands: Commands,
    current_state: Res<State<GameControlState>>,
    mut next_state: ResMut<NextState<GameControlState>>,
    spatial_query: avian3d::prelude::SpatialQuery,
) {
    if current_state.get() != &GameControlState::MapFreecam {
        return;
    }
    let mut pos = spawn_car_event.position;

    // Raycast down from pos.y + 100.0 to find exact ground height
    let start_y = pos.y + 100.0;
    let ray_origin = Vec3::new(pos.x, start_y, pos.z);
    let filter = avian3d::prelude::SpatialQueryFilter::default();

    if let Some(hit) = spatial_query.cast_ray(ray_origin, bevy::prelude::Dir3::NEG_Y, 1000.0, true, &filter) {
        let ground_y = start_y - hit.distance;
        pos.y = ground_y + 9.0;
    } else {
        pos.y += 9.0;
    }

    let handle = get_car_asset(&spawn_car_event.car_type, &asset_server);

    commands.spawn((
        WorldAssetRoot(handle.clone()),
        Transform::from_translation(pos),
        Car {
            _car_type: spawn_car_event.car_type.clone(),
        },
        avian3d::prelude::RigidBody::Dynamic,
        avian3d::prelude::ColliderConstructorHierarchy::new(
            avian3d::prelude::ColliderConstructor::TrimeshFromMesh,
        ),
        avian3d::prelude::Restitution::ZERO
            .with_combine_rule(avian3d::prelude::CoefficientCombine::Min),
        avian3d::prelude::Mass(1200.0),
        avian3d::prelude::CollisionLayers::new(
            [GamePhysicsLayer::Car],
            [GamePhysicsLayer::Map],
        ),
        CarDriveState::default(),
    ));
    next_state.set(GameControlState::DrivingCar);
}
