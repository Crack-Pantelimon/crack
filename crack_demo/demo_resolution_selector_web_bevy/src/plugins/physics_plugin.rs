use avian3d::prelude::*;
use bevy::prelude::*;

pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PhysicsPlugins::default())
            .insert_resource(Time::<Fixed>::from_hz(40.0))
            .add_systems(Update, (spawn_ball, update_ball_lifetimes));
    }
}

#[derive(Component)]
pub struct BallLifetime {
    pub timer: bevy::time::Timer,
}

fn spawn_ball(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    camera_query: Query<&Transform, With<Camera>>,
) {
    if keyboard_input.just_pressed(KeyCode::Space) {
        let camera_transform = camera_query.single().unwrap();
        let spawn_pos = camera_transform.translation;
        let forward = camera_transform.forward();
        let speed = 40.0;
        let velocity = forward * speed;

        commands.spawn((
            Mesh3d(meshes.add(Sphere::new(0.5).mesh().ico(5).unwrap())),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.2, 0.2),
                ..default()
            })),
            Transform::from_translation(spawn_pos),
            RigidBody::Dynamic,
            Collider::sphere(0.5),
            LinearVelocity(velocity),
            BallLifetime {
                timer: bevy::time::Timer::from_seconds(10.0, TimerMode::Once),
            },
        ));
    }
}

fn update_ball_lifetimes(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut BallLifetime)>,
) {
    for (entity, mut lifetime) in &mut query {
        lifetime.timer.tick(time.delta());
        if lifetime.timer.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}
