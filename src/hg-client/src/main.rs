#![feature(context_injection)]

use hg_ecs::{bind, resource, Resource, World, WORLD};

fn main() {
    let mut world = World::new();

    bind!(world);

    dbg!(MyThing::fetch());
    MyThing::fetch_mut().a += 1;
    dbg!(MyThing::fetch());

    {
        bind!(WORLD);

        MyThing::fetch_mut().a += 1;
    }

    dbg!(MyThing::fetch());
}

#[derive(Debug, Default)]
pub struct MyThing {
    a: u32,
}

resource!(MyThing);
