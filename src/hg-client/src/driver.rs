use std::time::Instant;

use hg_common::base::{
    collide::{bus::sys_flush_colliders, group::sys_update_colliders},
    debug::DebugDraw,
    kinematic::{sys_apply_kinematics, sys_kinematic_start_of_frame},
    mp::sys_update_mp_clients,
};
use hg_ecs::{bind, Entity, World};
use macroquad::time::get_frame_time;

use crate::{
    base::gfx::{
        bus::find_gfx,
        camera::{sys_update_virtual_cameras, VirtualCameraSelector},
        sprite::SolidRenderer,
        tile::TileRenderer,
    },
    game::{
        bullet::BulletTrailRenderer,
        debug::sys_update_debug,
        level::spawn_level,
        player::{sys_update_player_camera, sys_update_players},
    },
};

pub fn world_init(world: &mut World) {
    bind!(world);

    spawn_level(Entity::root());
}

pub fn world_tick(world: &mut World) {
    bind!(world);

    Entity::flush(world_flush);

    world_update();
    world_render();
}

pub fn world_update() {
    sys_kinematic_start_of_frame();
    sys_update_mp_clients();
    sys_update_players();
    sys_apply_kinematics(get_frame_time());
    sys_update_colliders();
    sys_update_player_camera();
    sys_update_virtual_cameras();
    sys_update_debug();
}

pub fn world_render() {
    for camera in &find_gfx::<VirtualCameraSelector>(Entity::root()) {
        let Some(camera_obj) = camera.get::<VirtualCameraSelector>().current() else {
            continue;
        };
        let _guard = camera_obj.bind();

        for layer in &find_gfx::<TileRenderer>(camera) {
            layer.get::<TileRenderer>().render(camera_obj.focus());
        }

        for trails in &find_gfx::<BulletTrailRenderer>(camera) {
            trails.get::<BulletTrailRenderer>().render(Instant::now());
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
