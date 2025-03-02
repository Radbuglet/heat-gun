use hg_common::base::{
    mp::MpServer,
    net::quic_server::QuicServerTransport,
    rpc::{sys_flush_rpc_groups, sys_flush_rpc_server, RpcGroup, RpcServer},
    time::{tps_to_dt, RunLoop},
};
use hg_ecs::{bind, Entity, World};

use crate::game::player::spawn_player;

pub fn world_init(world: &mut World, transport: QuicServerTransport) {
    bind!(world);

    // Setup engine root
    let rpc = Entity::root().add(RpcServer::new());
    let group = Entity::root().add(RpcGroup::new());
    Entity::root().add(MpServer::new(Box::new(transport), rpc, group));
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
    Entity::service::<MpServer>().process();
}

fn world_flush() {
    sys_flush_rpc_server();
    sys_flush_rpc_groups();
}
