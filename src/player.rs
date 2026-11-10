use bevy::prelude::*;

use crate::scene::WorldFlags;
use crate::world::{WorldFlag, WorldMap, WORLD_HALF_SIZE, WORLD_SIZE};

pub(crate) const PLAYER_SPEED: f32 = 5.0;
pub(crate) const PLAYER_HALF_SIZE: f32 = 0.4;
pub(crate) const PLAYER_GROUND_Y: f32 = 0.6;
const CAMERA_DISTANCE: f32 = 8.0;
const CAMERA_HEIGHT: f32 = 3.2;
const CAMERA_ROTATION_SPEED: f32 = 1.8;
const GRAVITY: f32 = -15.0;
const JUMP_FORCE: f32 = 7.0;

#[derive(Component)]
pub(crate) struct Player {
    velocity_y: f32,
    is_grounded: bool,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            velocity_y: 0.0,
            is_grounded: true,
        }
    }
}

#[derive(Component)]
pub(crate) struct FollowCamera;

#[derive(Resource, Default)]
pub(crate) struct CameraOrbit {
    yaw: f32,
}

pub(crate) fn orbit_camera(// カメラをプレイヤーの背中に回転させる
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut orbit: ResMut<CameraOrbit>,
    mut players: Query<&mut Transform, (With<Player>, Without<FollowCamera>)>,
    mut cameras: Query<&mut Transform, (With<FollowCamera>, Without<Player>)>,
) {
    let Ok(mut player_transform) = players.single_mut() else {
        return;
    };
    let Ok(mut camera_transform) = cameras.single_mut() else {
        return;
    };

    let mut turn = 0.0;
    if keyboard.pressed(KeyCode::ArrowLeft) {
        turn += 1.0;
    }
    if keyboard.pressed(KeyCode::ArrowRight) {
        turn -= 1.0;
    }
    orbit.yaw += turn * CAMERA_ROTATION_SPEED * time.delta_secs();

    // 常にカメラと反対向きにし、旋回中もプレイヤーの背中をカメラへ向ける。
    player_transform.rotation = Quat::from_rotation_y(orbit.yaw);

    let camera_offset = Vec3::new(orbit.yaw.sin(), 0.0, orbit.yaw.cos()) * CAMERA_DISTANCE;
    camera_transform.translation =
        player_transform.translation + camera_offset + Vec3::Y * CAMERA_HEIGHT;
    camera_transform.look_at(player_transform.translation + Vec3::Y * 0.6, Vec3::Y);
}

/// WASD、矢印キー、または接続中ゲームパッドの左スティックでXZ平面を移動する。
pub(crate) fn move_player(// プレイヤーを移動させる
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    world_map: Res<WorldMap>,
    flags: Res<WorldFlags>,
    mut players: Query<(&mut Transform, &mut Player)>,
) {
    let mut direction = Vec2::ZERO;

    if keyboard.pressed(KeyCode::KeyA) {
        direction.x -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        direction.x += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        direction.y += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        direction.y -= 1.0;
    }

    // キーボード入力がない場合は、最初に見つかったゲームパッドの左スティックを使う。
    if direction == Vec2::ZERO {
        if let Some(gamepad) = gamepads.iter().next() {
            direction.x = gamepad.get(GamepadAxis::LeftStickX).unwrap_or(0.0);
            direction.y = -gamepad.get(GamepadAxis::LeftStickY).unwrap_or(0.0);
        }
    }

    let direction = direction.clamp_length_max(1.0);
    let bridge_built = flags.is_set(WorldFlag::BridgeBuilt);
    let jump_pressed = keyboard.just_pressed(KeyCode::Space)
        || gamepads
            .iter()
            .any(|gamepad| gamepad.just_pressed(GamepadButton::East));
    if let Ok((mut transform, mut player)) = players.single_mut() {
        // 上下入力はプレイヤーの前後、左右入力はプレイヤーの左右として扱う。
        let world_direction = transform.rotation * Vec3::new(direction.x, 0.0, -direction.y);
        let movement = world_direction * PLAYER_SPEED * time.delta_secs();
        let candidate_x = wrap_world_coordinate(transform.translation.x + movement.x);
        if !world_map.blocks_movement(
            Vec3::new(
                candidate_x,
                transform.translation.y,
                transform.translation.z,
            ),
            bridge_built,
            PLAYER_HALF_SIZE,
        ) {
            transform.translation.x = candidate_x;
        }

        let candidate_z = wrap_world_coordinate(transform.translation.z + movement.z);
        if !world_map.blocks_movement(
            Vec3::new(
                transform.translation.x,
                transform.translation.y,
                candidate_z,
            ),
            bridge_built,
            PLAYER_HALF_SIZE,
        ) {
            transform.translation.z = candidate_z;
        }

        let ground_y = world_map.ground_height(transform.translation.x, transform.translation.z)
            + PLAYER_GROUND_Y;
        apply_vertical_motion(
            &mut player,
            &mut transform,
            jump_pressed,
            ground_y,
            time.delta_secs(),
        );
    }
}

fn apply_vertical_motion(
    player: &mut Player,
    transform: &mut Transform,
    jump_pressed: bool,
    ground_y: f32,
    delta_secs: f32,
) {
    if player.is_grounded && jump_pressed {
        player.velocity_y = JUMP_FORCE;
        player.is_grounded = false;
    } else if player.is_grounded {
        transform.translation.y = ground_y;
        player.velocity_y = 0.0;
        return;
    }

    player.velocity_y += GRAVITY * delta_secs;
    let candidate_y = transform.translation.y + player.velocity_y * delta_secs;
    if candidate_y <= ground_y {
        transform.translation.y = ground_y;
        player.velocity_y = 0.0;
        player.is_grounded = true;
    } else {
        transform.translation.y = candidate_y;
        player.is_grounded = false;
    }
}

pub(crate) fn wrap_world_coordinate(coordinate: f32) -> f32 {
    // ワールドの座標がワールドの境界を超えた場合に反対側の座標にラップする
    (coordinate + WORLD_HALF_SIZE).rem_euclid(WORLD_SIZE) - WORLD_HALF_SIZE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jump_applies_upward_velocity_and_gravity() {
        let mut player = Player::default();
        let mut transform = Transform::from_xyz(0.0, PLAYER_GROUND_Y, 0.0);

        apply_vertical_motion(&mut player, &mut transform, true, PLAYER_GROUND_Y, 0.1);

        assert!(!player.is_grounded);
        assert!((player.velocity_y - 5.5).abs() < 0.001);
        assert!((transform.translation.y - (PLAYER_GROUND_Y + 0.55)).abs() < 0.001);
    }

    #[test]
    fn airborne_player_cannot_jump_again() {
        let mut player = Player {
            velocity_y: 2.0,
            is_grounded: false,
        };
        let mut transform = Transform::from_xyz(0.0, PLAYER_GROUND_Y + 1.0, 0.0);

        apply_vertical_motion(&mut player, &mut transform, true, PLAYER_GROUND_Y, 0.1);

        assert!(!player.is_grounded);
        assert!((player.velocity_y - 0.5).abs() < 0.001);
        assert!((transform.translation.y - (PLAYER_GROUND_Y + 1.05)).abs() < 0.001);
    }

    #[test]
    fn falling_player_lands_on_the_ground_and_resets_velocity() {
        let mut player = Player {
            velocity_y: -3.0,
            is_grounded: false,
        };
        let mut transform = Transform::from_xyz(0.0, PLAYER_GROUND_Y + 0.1, 0.0);

        apply_vertical_motion(&mut player, &mut transform, false, PLAYER_GROUND_Y, 0.1);

        assert!(player.is_grounded);
        assert_eq!(player.velocity_y, 0.0);
        assert_eq!(transform.translation.y, PLAYER_GROUND_Y);
    }

    #[test]
    fn grounded_player_tracks_a_rising_terrain_surface() {
        let mut player = Player::default();
        let mut transform = Transform::from_xyz(0.0, PLAYER_GROUND_Y, 0.0);

        apply_vertical_motion(
            &mut player,
            &mut transform,
            false,
            PLAYER_GROUND_Y + 1.2,
            0.1,
        );

        assert!(player.is_grounded);
        assert_eq!(transform.translation.y, PLAYER_GROUND_Y + 1.2);
    }
}
