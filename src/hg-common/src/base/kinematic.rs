use std::context::Bundle;

use glam::Vec2;
use hg_ecs::{component, entity::Component, Obj, Query, Resource};

use crate::utils::math::{cancel_normal, HullCastRequest, MoveAndSlide};

use super::collide::bus::{Collider, ColliderLookupCx};

// === Components === //

component!(Pos, Vel, KinematicProps);

#[derive(Debug, Clone, Default)]
pub struct Pos(pub Vec2);

#[derive(Debug, Clone, Default)]
pub struct Vel {
    pub physical: Vec2,
    pub artificial: Vec2,
}

impl Vel {
    pub fn total(&self) -> Vec2 {
        self.physical + self.artificial
    }
}

#[derive(Debug, Clone)]
pub struct KinematicProps {
    pub gravity: Vec2,
    pub friction: f32,
}

// === Systems === //

pub fn sys_kinematic_start_of_frame() {
    <<KinematicProps as Component>::Arena as Resource>::fetch();

    for mut vel in Query::<Obj<Vel>>::new() {
        vel.artificial = Vec2::ZERO;
    }
}

pub fn sys_apply_kinematics(dt: f32) {
    for (mut vel, kine) in Query::<(Obj<Vel>, Obj<KinematicProps>)>::new() {
        vel.physical += kine.gravity * dt;
        vel.physical *= kine.friction;
    }

    for (mut pos, mut vel, collider) in Query::<(Obj<Pos>, Obj<Vel>, Obj<Collider>)>::new() {
        let mut predicate =
            |candidate: Obj<Collider>, _cx: Bundle<ColliderLookupCx<'_>>| candidate != collider;

        let bus = collider.expect_bus();
        let aabb = collider.aabb();

        let mut move_and_slide = MoveAndSlide::new(10, vel.total() * dt);

        while let Some(desired_delta) = move_and_slide.next_delta() {
            let hull_result =
                bus.cast_hull(HullCastRequest::new(aabb, desired_delta), &mut predicate);

            pos.0 += desired_delta * hull_result.percent;
            move_and_slide.update(hull_result);

            if let Some(normal) = hull_result.normal {
                vel.artificial = cancel_normal(vel.artificial, normal);
                vel.physical = cancel_normal(vel.physical, normal);
            }
        }
    }
}
