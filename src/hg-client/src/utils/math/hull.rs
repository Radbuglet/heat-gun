use std::cmp::Ordering;

use macroquad::math::Vec2;

use super::{ilerp_f32, Aabb, Axis2, Segment, Sign, TileFace, Vec2Ext as _, SAFETY_THRESHOLD};

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
        // If the hull-cast starts inside the occluder (not the safety margin!), treat the occluder
        // as being intangible to allow the hull to escape.
        if self.start.intersects(occluder) {
            return self.result_clear();
        }

        // We want to see the minimum distance along `translation`, if any, will cause `self` to
        // intersect with `occluder`.
        let mut result = self.result_clear();

        for axis in Axis2::AXES {
            let sign = Sign::of_biased(self.delta.axis(axis));

            // Compute the axis-aligned position of the edge of the starting AABB that's in the
            // direction of motion.
            let closest_self = self.start.corner(axis, sign);

            // Compute the axis-aligned position of the edge of the occluding AABB that's in the
            // opposite direction of motion.
            let closest_other_real = occluder.corner(axis, sign.invert());

            // Adjust `closest_other` to make it closer to the object by `SAFE_THRESHOLD`
            let closest_other_safety = closest_other_real + sign.unit_mag(-SAFETY_THRESHOLD);

            // Figure out how far we'd have to lerp the hull to become flush with a line extending
            // along the safety edge's threshold.
            let mut lerp_to_flush = ilerp_f32(
                closest_self,
                closest_self + self.delta.axis(axis),
                closest_other_safety,
            );

            if lerp_to_flush < 0. {
                // If the lerp factor is negative, the hull is either...
                //
                // 1. ...in the safety zone but not the actual occluder, in which case we should
                //    prevent further motion into the AABB's safety zone but allow motion away from
                //    it.
                //
                // 2. ...past the occluder and the occluder winnowing step was too liberal and
                //    considered this a candidate occluder.
                //

                // Let's start by detecting the second case.
                if ilerp_f32(
                    closest_self,
                    closest_self + self.delta.axis(axis),
                    closest_other_real,
                ) < 0.
                {
                    // We're definitely past the safety threshold.
                    continue;
                }

                // The first case, meanwhile, sort-of handles itself. Recall that `closest_other_xx`
                // is computed with respect to the direction of the hull's travel. Hence, if the
                // hull traveling is from inside the safe zone and into the occluded zone, one of
                // the hull AABB's edges will be sandwiched between `closest_other_safety` and
                // `closest_other_real` and this next line will run, blocking the offending motion.
                // If, instead, the hull is traveling away from the occlusion zone, the occluding
                // edge pair will be on the other side of the AABB and the logic detecting the
                // second case will reject the occlusion, allowing the AABB to travel freely as we
                // had intended.
                //
                // We can't have a scenario where the hull's starting AABB has edges on both sides
                // of the occluder AABB because, otherwise, the `self.start.intersects(occluder)`
                // check at the start of the function would trigger.
                lerp_to_flush = 0.;
            }

            if lerp_to_flush > 1.0 {
                // The hull couldn't possibly clip this edge—it's too far away!
                continue;
            }

            // Produce a tentative result for this occlusion.
            let normal = TileFace::compose(axis, sign).invert().as_vec();
            let candidate_result = self.result_obstructed(lerp_to_flush, normal);

            // This could be a false positive because `lerp_to_flush` assumes that the occluding
            // edge is a line extending out forever. To reject that false positive, we check our
            // candidate result to ensure that it actually places the hull's AABB flush with the
            // occluding AABB's safety threshold.
            if !self
                .transform_percent(candidate_result.percent)
                // We have to grow our AABB a little bit to account for the way we're trying to stay
                // flush with the occluder.
                .grow(Vec2::splat(SAFETY_THRESHOLD * 2.))
                // We want to check against the occluder's safety zone—not the occluder itself.
                .intersects(occluder.grow(Vec2::splat(SAFETY_THRESHOLD * 2.)))
            {
                continue;
            }

            result = result.min(candidate_result);
        }

        result
    }

    pub fn transform_percent(self, percent: f32) -> Aabb {
        self.start.translated(self.delta * percent)
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
        self.start
            .translate_extend(self.delta)
            .grow(Vec2::splat(SAFETY_THRESHOLD))
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
        HullCastResult {
            percent,
            dist: self.delta_len * percent,
            normal: Some(normal),
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

impl HullCastResult {
    pub fn is_full(self) -> bool {
        self.normal.is_none()
    }

    pub fn is_obstructed(self) -> bool {
        self.normal.is_some()
    }
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
