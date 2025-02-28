use std::context::{infer_bundle, Bundle};

use hg_common::base::time::{tps_to_dt, RunLoop};
use hg_ecs::{bind, Entity, World};

use crate::{
    base::net::{NetManager, Transport},
    game::player::PlayerRpcKindServer,
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

    // TODO
}

pub fn world_main_loop(world: &mut World) {
    bind!(world);

    let mut rl = Entity::service::<RunLoop>();

    loop {
        world_tick();

        if rl.should_exit() {
            break;
        }

        rl.wait_for_tick();
    }
}

fn world_tick() {
    Entity::service::<NetManager>().process();
}
