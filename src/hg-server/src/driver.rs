use std::context::{infer_bundle, Bundle};

use hg_common::base::{
    rpc::RpcNodeServer,
    time::{tps_to_dt, RunLoop},
};
use hg_ecs::{bind, Entity, Obj, Query, World};

use crate::{
    base::net::{NetManager, Transport},
    game::player::{spawn_player, PlayerRpcKindServer},
};

pub fn world_init(world: &mut World, transport: Transport) {
    bind!(world);

    // Setup engine root
    Entity::root()
        .with_proc(|me, cx: Bundle<infer_bundle!('_)>| {
            let static ..cx;
            NetManager::new(me, transport);
        })
        .with(RunLoop::new(tps_to_dt(60.)));

    // Define RPC kinds
    let nm = Entity::service::<NetManager>();
    nm.rpc().define::<PlayerRpcKindServer>();

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
    net.process();

    for &peer in &net.on_join() {
        for node in Query::<Obj<RpcNodeServer>>::new() {
            net.rpc().replicate(node, peer);
        }
    }
}
