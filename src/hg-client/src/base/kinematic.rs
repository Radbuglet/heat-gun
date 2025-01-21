use std::context::Bundle;

use hg_ecs::{component, Obj, Query};
use macroquad::{
    color::{BLUE, GREEN, RED, YELLOW},
    math::Vec2,
    time::get_frame_time,
};

use crate::utils::math::{cancel_normal, HullCastRequest};

use super::{
    collide::bus::{Collider, ColliderLookupCx},
    debug::debug_draw,
};

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
    for mut vel in Query::<Obj<Vel>>::new() {
        vel.artificial = Vec2::ZERO;
    }
}

pub fn sys_apply_kinematics() {
    let dt = get_frame_time();
    let dbg = debug_draw().frame();

    for (mut vel, kine) in Query::<(Obj<Vel>, Obj<KinematicProps>)>::new() {
        vel.physical += kine.gravity * dt;
        vel.physical *= kine.friction;
    }

    for (mut pos, mut vel, collider) in Query::<(Obj<Pos>, Obj<Vel>, Obj<Collider>)>::new() {
        let mut predicate =
            |candidate: Obj<Collider>, _cx: Bundle<ColliderLookupCx<'_>>| candidate != collider;

        let bus = collider.expect_bus();
        let aabb = collider.aabb();

        let mut desired_delta = vel.total() * dt;
        let mut iter = 0;

        while desired_delta.length() > 0.001 && iter < 10 {
            iter += 1;

            dbg.vector_scaled(pos.0, vel.artificial, RED);
            dbg.vector_scaled(pos.0, vel.physical, GREEN);
            dbg.vector_scaled(pos.0, vel.total(), BLUE);

            let hull_result =
                bus.cast_hull(HullCastRequest::new(aabb, desired_delta), &mut predicate);

            pos.0 += desired_delta * hull_result.percent;
            desired_delta *= 1.0 - hull_result.percent;

            if let Some(normal) = hull_result.normal {
                dbg.vector_scaled(pos.0, normal, YELLOW);

                vel.artificial = cancel_normal(vel.artificial, normal);
                vel.physical = cancel_normal(vel.physical, normal);
                desired_delta = cancel_normal(desired_delta, normal);
            }
        }
    }
}
