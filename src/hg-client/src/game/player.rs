use hg_common::{
    base::{
        collide::{
            bus::{register_collider, Collider, ColliderMask, ColliderMat},
            update::ColliderFollows,
        },
        kinematic::{KinematicProps, Pos, Vel},
    },
    utils::math::{Aabb, RgbaColor},
};
use hg_ecs::{component, Entity, Obj, Query};
use macroquad::{
    input::{is_key_down, is_key_pressed, KeyCode},
    math::{FloatExt, Vec2},
};

use crate::base::gfx::{bus::register_gfx, sprite::SolidRenderer};

// === Components === //

#[derive(Debug, Clone)]
pub struct PlayerController {
    last_heading: f32,
    camera: Obj<Pos>,
}

component!(PlayerController);

// === Prefabs === //

pub fn spawn_player(parent: Entity, camera: Entity) -> Entity {
    let player = Entity::new(parent)
        .with(Pos::default())
        .with(Vel::default())
        .with(KinematicProps {
            gravity: Vec2::Y * 4000.,
            friction: 0.98,
        })
        .with(PlayerController {
            last_heading: 0.,
            camera: camera.get(),
        })
        .with(SolidRenderer::new_centered(RgbaColor::RED, 50.))
        .with(Collider::new(ColliderMask::ALL, ColliderMat::Solid));

    player.with(ColliderFollows {
        target: player.get(),
        aabb: Aabb::new_centered(Vec2::ZERO, Vec2::splat(50.)),
    });

    register_gfx(player);
    register_collider(player.get());

    player
}

// === Systems === //

pub fn sys_update_players() {
    for (mut vel, mut player) in Query::<(Obj<Vel>, Obj<PlayerController>)>::new() {
        // Determine desired heading
        let mut heading = 0.;

        if is_key_down(KeyCode::A) {
            heading -= 1.;
        }

        if is_key_down(KeyCode::D) {
            heading += 1.;
        }

        if is_key_pressed(KeyCode::Space) {
            vel.physical.y = -2000.;
        }

        heading *= 2000.;

        // Compute actual heading
        player.last_heading = player.last_heading.lerp(heading, 0.9);

        // Apply heading
        vel.artificial += player.last_heading * Vec2::X;
    }
}

pub fn sys_update_player_camera() {
    for (pos, mut player) in Query::<(Obj<Pos>, Obj<PlayerController>)>::new() {
        // Update camera
        player.camera.0 = pos.0;
    }
}
