use hg_common::base::{
    net::quic_server::QuicServerTransport,
    rpc::{sys_flush_rpc_groups, sys_flush_rpc_server},
    time::{tps_to_dt, RunLoop},
};
use hg_ecs::{bind, Entity, World};

use crate::{base::net::NetManager, game::player::spawn_player};

pub fn world_init(world: &mut World, transport: QuicServerTransport) {
    bind!(world);

    // Setup engine root
    NetManager::attach(Entity::root(), transport);
    Entity::root().add(RunLoop::new(tps_to_dt(60.)));

    // Spawn a player
    spawn_player(Entity::root());
}

pub fn world_main_loop(world: &mut World) {
    bind!(world);

    let mut rl = Entity::service::<RunLoop>();

    loop {
        // Process tick
        world_tick();
        Entity::flush(|world| {
            bind!(world);
            world_flush();
        });

        // Wait for next tick
        if rl.should_exit() {
            break;
        }

        rl.wait_for_tick();
    }
}

fn world_tick() {
    let nm = Entity::service::<NetManager>();
    nm.process();
}

fn world_flush() {
    sys_flush_rpc_server();
    sys_flush_rpc_groups();
}
