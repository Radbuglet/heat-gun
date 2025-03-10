use glam::Vec2;
use hg_ecs::{component, Entity, Obj, Query};

use crate::utils::math::{cancel_normal, MoveAndSlide};

use super::collide::{bus::collide_everything, group::ColliderGroup};

// === Components === //

component!(Pos, Vel, KinematicProps, CollisionChecker);

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

#[derive(Debug, Clone)]
pub struct CollisionChecker {
    pub collider: Obj<ColliderGroup>,
    pub delta: Vec2,
    pub is_touching: bool,
}

impl CollisionChecker {
    pub fn new(collider: Obj<ColliderGroup>, direction: Vec2) -> Self {
        Self {
            collider,
            delta: direction,
            is_touching: false,
        }
    }
}

// === Prefabs === //

pub fn spawn_collision_checker(
    collider: Obj<ColliderGroup>,
    direction: Vec2,
) -> Obj<CollisionChecker> {
    Entity::new(collider.entity()).add(CollisionChecker::new(collider, direction))
}

// === Systems === //

pub fn sys_kinematic_start_of_frame() {
    for mut vel in Query::<Obj<Vel>>::new() {
        vel.artificial = Vec2::ZERO;
    }
}

pub fn sys_apply_kinematics(dt: f32) {
    for (mut vel, kine) in Query::<(Obj<Vel>, Obj<KinematicProps>)>::new() {
        vel.physical += kine.gravity * dt;
        vel.physical *= kine.friction;
    }

    for (mut pos, mut vel, collider) in Query::<(Obj<Pos>, Obj<Vel>, Obj<ColliderGroup>)>::new() {
        let mut move_and_slide = MoveAndSlide::new(10, vel.total() * dt);

        while let Some(desired_delta) = move_and_slide.next_delta() {
            let hull_result = collider.cast_hull(desired_delta, &mut collide_everything());

            pos.0 += desired_delta * hull_result.percent;
            move_and_slide.update(hull_result);

            if let Some(normal) = hull_result.normal {
                vel.artificial = cancel_normal(vel.artificial, normal);
                vel.physical = cancel_normal(vel.physical, normal);
            }
        }
    }

    for mut checker in Query::<Obj<CollisionChecker>>::new() {
        let hull = checker
            .collider
            .cast_hull(checker.delta, &mut collide_everything());

        checker.is_touching = !hull.is_full();
    }
}
