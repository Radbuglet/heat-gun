use std::sync::Arc;

use glam::Vec2;
use hg_ecs::{bind, component, Entity, Obj, Query, World};

use crate::{
    kinematic::Pos,
    utils::math::{Aabb, HullCastRequest, HullCastResult},
};

use super::bus::{register_collider, Collider, ColliderBus, ColliderMask, ColliderMat};

const CONCURRENT_MUTATION_ERR: &str =
    "cannot modify the member set of a collider group while it is being iterated over";

// === Predicates === //

pub fn collide_no_group(
    group: Obj<ColliderGroup>,
) -> impl 'static + Copy + FnMut(Obj<Collider>, &mut World) -> bool {
    move |collider, world| collide_no_group_inner(world, group, collider)
}

fn collide_no_group_inner(
    world: &mut World,
    group: Obj<ColliderGroup>,
    collider: Obj<Collider>,
) -> bool {
    bind!(world);

    let Some(member) = collider.entity().try_get::<ColliderGroupMember>() else {
        return true;
    };

    member.group != group
}

// === Components === //

component!(ColliderGroup, ColliderGroupMember, ColliderFollows);

#[derive(Debug, Default)]
pub struct ColliderGroup {
    colliders: Arc<Vec<Obj<ColliderGroupMember>>>,
}

impl ColliderGroup {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(mut self: Obj<Self>, collider: Obj<Collider>) -> Obj<ColliderGroupMember> {
        let member = collider.entity().add(ColliderGroupMember {
            collider,
            group: self,
            index: self.colliders.len(),
        });
        Arc::get_mut(&mut self.colliders)
            .expect(CONCURRENT_MUTATION_ERR)
            .push(member);

        member
    }

    pub fn members(&self) -> Arc<Vec<Obj<ColliderGroupMember>>> {
        self.colliders.clone()
    }

    pub fn expect_bus(&self) -> Obj<ColliderBus> {
        self.colliders[0].collider.expect_bus()
    }

    pub fn cast_hull(
        self: Obj<Self>,
        delta: Vec2,
        mut predicate: impl FnMut(Obj<Collider>, &mut World) -> bool,
    ) -> HullCastResult {
        let mut result = HullCastResult {
            percent: 1.,
            dist: delta.length(),
            normal: None,
        };

        let Some(&first_member) = self.members().get(0) else {
            return result;
        };

        let bus = first_member.collider.expect_bus();

        let mut predicate = |collider: Obj<Collider>, world: &mut World| {
            collide_no_group(self)(collider, world) && predicate(collider, world)
        };

        for &member in self.members().iter() {
            result = result.min(bus.cast_hull(
                HullCastRequest::new(member.collider.aabb(), delta),
                &mut predicate,
            ));
        }

        result
    }
}

#[derive(Debug)]
pub struct ColliderGroupMember {
    collider: Obj<Collider>,
    group: Obj<ColliderGroup>,
    index: usize,
}

impl ColliderGroupMember {
    pub fn group(&self) -> Obj<ColliderGroup> {
        self.group
    }

    pub fn collider(&self) -> Obj<Collider> {
        self.collider
    }

    pub fn unregister(mut self: Obj<Self>) {
        let colliders = Arc::get_mut(&mut self.group.colliders).expect(CONCURRENT_MUTATION_ERR);

        colliders.swap_remove(self.index);

        if let Some(&(mut moved)) = colliders.get(self.index) {
            moved.index = self.index;
        }
    }
}

#[derive(Debug)]
pub struct ColliderFollows {
    pub target: Obj<Pos>,
    pub aabb: Aabb,
}

// === Prefabs === //

pub fn spawn_collider(
    group: Obj<ColliderGroup>,
    pos: Obj<Pos>,
    aabb: Aabb,
    mask: ColliderMask,
    mat: ColliderMat,
) -> Obj<Collider> {
    let me = Entity::new(group.entity());

    // Create collider
    let collider = me.add(Collider::new(mask, mat));
    register_collider(collider);

    // Add to group
    group.register(collider);

    // Ensure that it follows the parent position
    me.add(ColliderFollows { target: pos, aabb });

    collider
}

// === Systems === //

pub fn sys_update_colliders() {
    for (mut collider, follows) in Query::<(Obj<Collider>, Obj<ColliderFollows>)>::new() {
        collider.set_aabb(follows.aabb.translated(follows.target.0));
    }
}
