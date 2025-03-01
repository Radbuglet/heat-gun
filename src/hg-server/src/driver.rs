use hg_common::base::{
    net::quic_server::QuicServerTransport,
    rpc::{RpcNodeServer, RpcServer},
    time::{tps_to_dt, RunLoop},
};
use hg_ecs::{bind, Entity, Obj, Query, World};

use crate::{
    base::net::NetManager,
    game::player::{spawn_player, PlayerRpcKindServer},
};

pub fn world_init(world: &mut World, transport: QuicServerTransport) {
    bind!(world);

    // Setup engine root
    let mut rpc = Entity::root().add(RpcServer::new());
    Entity::root()
        .with(NetManager::new(transport, rpc))
        .with(RunLoop::new(tps_to_dt(60.)));

    // Define RPC kinds
    rpc.define::<PlayerRpcKindServer>();

    // Spawn a player
    spawn_player(Entity::root());
}

pub fn world_main_loop(world: &mut World) {
    bind!(world);

    let mut rl = Entity::service::<RunLoop>();

    loop {
        world_tick();
        Entity::flush(|_world| {});

        if rl.should_exit() {
            break;
        }

        rl.wait_for_tick();
    }
}

fn world_tick() {
    let net = Entity::service::<NetManager>();
    let rpc = Entity::service::<RpcServer>();
    net.process();

    for &peer in &net.on_join() {
        for node in Query::<Obj<RpcNodeServer>>::new() {
            rpc.replicate(node, peer);
        }
    }
}
