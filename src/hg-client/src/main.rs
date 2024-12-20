#![feature(context_injection)]

use hg_ecs::{bind, component, obj::Obj, World};

fn main() {
    let mut world = World::new();

    bind!(world);

    let mut root = Obj::new(Player {
        name: "whee".to_string(),
        child: None,
    });

    root.child = Some(Obj::new(Player {
        name: "woo".to_string(),
        child: None,
    }));

    dbg!(root.debug());
    dbg!(root);

    root.child.unwrap().destroy();

    dbg!(root.debug());
}

#[derive(Debug)]
pub struct Player {
    name: String,
    child: Option<Obj<Player>>,
}

component!(Player);
