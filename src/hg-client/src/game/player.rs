use hg_ecs::{component, Entity, Obj, Query};
use macroquad::{
    color::RED,
    input::{is_key_down, KeyCode},
    math::{FloatExt, Vec2},
};

use crate::utils::math::Aabb;

use crate::base::{
    collide::{
        bus::{register_collider, Collider, ColliderMask, ColliderMat},
        update::ColliderFollows,
    },
    gfx::{bus::register_gfx, sprite::SolidRenderer},
    kinematic::{KinematicProps, Pos, Vel},
};

#[derive(Debug, Clone, Default)]
pub struct PlayerController {
    last_heading: f32,
}

component!(PlayerController);

pub fn spawn_player(parent: Entity) -> Entity {
    let player = Entity::new(parent)
        .with(Pos::default())
        .with(Vel::default())
        .with(KinematicProps {
            gravity: Vec2::Y * 1000.,
            friction: 0.98,
        })
        .with(PlayerController::default())
        .with(SolidRenderer::new_centered(RED, 50.))
        .with(Collider::new(ColliderMask::ALL, ColliderMat::Solid));

    player.with(ColliderFollows {
        target: player.get(),
        aabb: Aabb::new_centered(Vec2::ZERO, Vec2::splat(50.)),
    });

    register_gfx(player);
    register_collider(player.get());

    player
}

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

        heading *= 250.;

        // Compute actual heading
        player.last_heading = player.last_heading.lerp(heading, 0.9);

        // Apply heading
        vel.artificial += player.last_heading * Vec2::X;
    }
}
