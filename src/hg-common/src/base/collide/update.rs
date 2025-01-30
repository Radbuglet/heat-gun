use hg_ecs::{component, Obj, Query};

use crate::{base::kinematic::Pos, utils::math::Aabb};

use super::bus::Collider;

#[derive(Debug)]
pub struct ColliderFollows {
    pub target: Obj<Pos>,
    pub aabb: Aabb,
}

component!(ColliderFollows);

pub fn sys_update_colliders() {
    for (mut collider, follows) in Query::<(Obj<Collider>, Obj<ColliderFollows>)>::new() {
        collider.set_aabb(follows.aabb.translated(follows.target.0));
    }
}
