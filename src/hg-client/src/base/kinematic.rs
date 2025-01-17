use std::context::Bundle;

use hg_ecs::{component, Obj, Query};
use macroquad::{math::Vec2, time::get_frame_time};

use crate::utils::math::{Axis2, HullCastRequest, Vec2Ext};

use super::collide::bus::{Collider, ColliderBus, ColliderLookupCx};

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

component!(Pos, Vel, KinematicProps);

pub fn sys_kinematic_start_of_frame() {
    for mut vel in Query::<Obj<Vel>>::new() {
        vel.artificial = Vec2::ZERO;
    }
}

pub fn sys_apply_kinematics() {
    let dt = get_frame_time();

    for (mut pos, mut vel, collider) in Query::<(Obj<Pos>, Obj<Vel>, Obj<Collider>)>::new() {
        let mut predicate = |candidate: Obj<Collider>, cx: Bundle<ColliderLookupCx<'_>>| {
            let static ..cx;
            candidate != collider
        };

        let aabb = collider.aabb();
        let bus = collider.entity().deep_get::<ColliderBus>();

        for axis in Axis2::AXES {
            let percent = bus.check_hull_percent(
                HullCastRequest::new(
                    aabb.grow(Vec2::splat(1.)),
                    vel.physical.mask_in_axis(axis) * dt,
                ),
                &mut predicate,
            );
            *vel.physical.axis_mut(axis) *= percent;
        }

        let desired_delta = vel.total() * dt;

        let percent_moved =
            bus.check_hull_percent(HullCastRequest::new(aabb, desired_delta), &mut predicate);

        pos.0 += desired_delta * percent_moved;
    }

    for (mut vel, kine) in Query::<(Obj<Vel>, Obj<KinematicProps>)>::new() {
        vel.physical += kine.gravity * dt;
        vel.physical *= kine.friction;
    }
}
