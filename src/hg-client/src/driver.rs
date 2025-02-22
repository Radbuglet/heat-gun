use bytes::BytesMut;
use hg_common::base::{
    collide::{bus::sys_flush_colliders, update::sys_update_colliders},
    debug::DebugDraw,
    kinematic::{sys_apply_kinematics, sys_kinematic_start_of_frame, Pos},
    net::{back_pressure::ErasedTaskGuard, codec::FrameEncoder},
};
use hg_ecs::{bind, Entity, World};
use macroquad::{math::Vec2, time::get_frame_time};
use tokio_util::codec::Encoder as _;

use crate::{
    base::{
        gfx::{
            bus::find_gfx,
            camera::{sys_update_virtual_cameras, VirtualCameraSelector},
            sprite::SolidRenderer,
            tile::TileRenderer,
        },
        net::{NetManager, TransportEvent},
    },
    game::{
        debug::sys_update_debug,
        level::spawn_level,
        player::{spawn_player, sys_update_player_camera, sys_update_players},
    },
};

pub fn world_init(world: &mut World) {
    bind!(world);

    Entity::root().add(NetManager::new().unwrap());

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

    world_update();
    world_render();
}

pub fn world_update() {
    sys_kinematic_start_of_frame();
    sys_update_players();
    sys_apply_kinematics(get_frame_time());
    sys_update_colliders();
    sys_update_player_camera();
    sys_update_virtual_cameras();
    sys_update_debug();

    let mut nm = Entity::root().get::<NetManager>();

    while let Some(ev) = nm.transport.process_non_blocking() {
        match ev {
            TransportEvent::Connected => {
                tracing::info!("Connected");
            }
            TransportEvent::Disconnected { cause } => {
                tracing::info!("Disconnected: {cause:?}");
            }
            TransportEvent::DataReceived { packet, task } => {
                tracing::info!("DataReceived: {packet:?}");
                drop(task);
            }
        }
    }

    let mut packet = BytesMut::new();
    FrameEncoder.encode(&[0; 64][..], &mut packet).unwrap();
    nm.transport
        .send_reliable(packet.freeze(), ErasedTaskGuard::noop());
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
