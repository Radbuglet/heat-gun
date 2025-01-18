use std::cmp::Ordering;

use macroquad::math::Vec2;

use super::{ilerp_f32, Aabb, Axis2, Segment, Sign, Vec2Ext as _};

// === HullCastRequest === //

#[derive(Debug, Copy, Clone)]
pub struct HullCastRequest {
    start: Aabb,
    delta: Vec2,
    delta_norm: Vec2,
    delta_len: f32,
}

impl HullCastRequest {
    pub fn new(start: Aabb, delta: Vec2) -> Self {
        Self {
            start,
            delta,
            delta_norm: delta.normalize_or_zero(),
            delta_len: delta.length(),
        }
    }

    pub fn hull_cast(self, occluder: Aabb) -> HullCastResult {
        // We want to see the minimum distance along `translation`, if any, will cause `self` to
        // intersect with `occluder`.

        let mut result = self.result_clear();

        for axis in Axis2::AXES {
            let sign = Sign::of_biased(self.delta.axis(axis));
            let closest_self = self.start.corner(axis, sign);
            let closest_other = occluder.corner(axis, -sign);

            let achieve_lerp = ilerp_f32(
                closest_self,
                closest_self + self.delta.axis(axis),
                closest_other,
            );

            // `result_obstructed` clamps `achieve_lerp` for us.
            result = result.min(self.result_obstructed(achieve_lerp, Vec2::ZERO));
        }

        result
    }

    pub fn delta(self) -> Vec2 {
        self.delta
    }

    pub fn delta_norm(self) -> Vec2 {
        self.delta_norm
    }

    pub fn delta_len(self) -> f32 {
        self.delta_len
    }

    pub fn start_aabb(self) -> Aabb {
        self.start
    }

    pub fn end_aabb(self) -> Aabb {
        self.start.translated(self.delta)
    }

    pub fn candidate_aabb(self) -> Aabb {
        self.start.translate_extend(self.delta)
    }

    pub fn debug_segment(self) -> Segment {
        Segment::new_delta(self.start.center(), self.delta)
    }

    pub fn result_clear(self) -> HullCastResult {
        HullCastResult {
            percent: 1.0,
            dist: self.delta_len,
            normal: None,
        }
    }

    pub fn result_obstructed(self, percent: f32, normal: Vec2) -> HullCastResult {
        if (0f32..=1f32).contains(&percent) {
            HullCastResult {
                percent,
                dist: self.delta_len * percent,
                normal: Some(normal),
            }
        } else {
            self.result_clear()
        }
    }
}

// === HullCastResult === //

#[derive(Debug, Copy, Clone)]
pub struct HullCastResult {
    pub percent: f32,
    pub dist: f32,
    pub normal: Option<Vec2>,
}

impl Eq for HullCastResult {}

impl PartialEq for HullCastResult {
    fn eq(&self, other: &Self) -> bool {
        self.percent.total_cmp(&other.percent).is_eq()
    }
}

impl Ord for HullCastResult {
    fn cmp(&self, other: &Self) -> Ordering {
        self.percent.total_cmp(&other.percent)
    }
}

impl PartialOrd for HullCastResult {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
