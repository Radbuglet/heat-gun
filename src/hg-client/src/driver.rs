use hg_ecs::{bind, Entity, World};
use macroquad::math::Vec2;

use crate::{
    base::{
        collide::{bus::sys_flush_colliders, update::sys_update_colliders},
        debug::DebugDraw,
        gfx::{
            bus::find_gfx,
            camera::{sys_update_virtual_cameras, VirtualCameraSelector},
            sprite::SolidRenderer,
            tile::TileRenderer,
        },
        kinematic::{sys_apply_kinematics, sys_kinematic_start_of_frame, Pos},
    },
    game::{
        debug::sys_update_debug,
        level::spawn_level,
        player::{spawn_player, sys_update_players},
    },
};

pub fn world_init(world: &mut World) {
    bind!(world);

    let level = spawn_level(Entity::root());

    // Spawn the player
    let player = spawn_player(
        level,
        level
            .get::<VirtualCameraSelector>()
            .current()
            .unwrap()
            .entity(),
    );
    player.get::<Pos>().0 = Vec2::new(100., -200.);
}

pub fn world_tick(world: &mut World) {
    bind!(world);

    Entity::flush(world_flush);

    // Update
    sys_update_virtual_cameras();
    sys_kinematic_start_of_frame();
    sys_update_players();
    sys_apply_kinematics();
    sys_update_colliders();
    sys_update_debug();

    // Render
    for camera in &find_gfx::<VirtualCameraSelector>(Entity::root()) {
        let Some(camera_obj) = camera.get::<VirtualCameraSelector>().current() else {
            continue;
        };
        let _guard = camera_obj.bind();

        for layer in &find_gfx::<TileRenderer>(camera) {
            layer.get::<TileRenderer>().render(camera_obj.focus());
        }

        for solid in &find_gfx::<SolidRenderer>(camera) {
            solid.get::<SolidRenderer>().render();
        }

        for dbg in &find_gfx::<DebugDraw>(camera) {
            dbg.get::<DebugDraw>().render();
        }
    }
}

pub fn world_flush(world: &mut World) {
    bind!(world);

    sys_flush_colliders();
}
