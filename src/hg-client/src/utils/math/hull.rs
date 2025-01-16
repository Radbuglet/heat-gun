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

    pub fn hull_cast_percent(self, occluder: Aabb) -> f32 {
        // We want to see the minimum distance along `translation`, if any, will cause `self` to
        // intersect with `occluder`.

        let mut max_lerp = 1f32;

        for axis in Axis2::AXES {
            let sign = Sign::of_biased(self.delta.axis(axis));
            let closest_self = self.start.corner(axis, sign);
            let closest_other = occluder.corner(axis, -sign);

            let achieve_lerp = ilerp_f32(
                closest_self,
                closest_self + self.delta.axis(axis),
                closest_other,
            );

            if achieve_lerp >= 0.0 && achieve_lerp < 1.0 {
                max_lerp = max_lerp.min(achieve_lerp);
            }
        }

        max_lerp
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

    pub fn hull_cast(self, occluder: Aabb) -> f32 {
        self.hull_cast_percent(occluder) * self.delta.length()
    }
}
