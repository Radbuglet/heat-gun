use std::cmp::Ordering;

use macroquad::{
    color::{BLUE, RED},
    math::Vec2,
};

use crate::base::debug::debug_draw;

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

    /// Hull casts into a "padding" occluder.
    ///
    /// A padding occluder is an axis-aligned `aabb` which allows motion into it so long as that
    /// motion is not traveling in the direction of the `face`. If has no safety threshold
    /// of its own.
    pub fn hull_cast_padding(self, aabb: Aabb, face: TileFace) -> HullCastResult {
        debug_draw().frame().line_rect(aabb, 15., BLUE);

        // If the delta is not pointing in the same direction of the face, we can quickly tell that
        // the padding will have no effect in this hull-cast.
        if self.delta.dot(face.as_vec()) <= 0.0 {
            return self.result_clear();
        }

        // Find the edge of the starting hull which is moving towards the padding.
        let hull_edge = self.start.edge_segment(face);

        // Find the edge of the occluder that is closest to that moving edge.
        let aabb_edge = aabb.edge_segment(face.invert());

        // Find the opposite edge too.
        let aabb_edge_other = aabb.edge_segment(face);

        // Determine how far we must move the hull along its delta before `hull_edge` and `aabb_edge`
        // are coincident.
        let hull_start = hull_edge.start.axis(face.axis());
        let hull_end = (hull_edge.start + self.delta).axis(face.axis());
        let aabb_pos = aabb_edge.start.axis(face.axis());

        let mut lerp_to_flush = ilerp_f32(hull_start, hull_end, aabb_pos);

        if lerp_to_flush < 0. {
            // If the lerp factor is negative, the hull is either...
            //
            // 1. ...in the padding itself, in which case we should prevent further motion into the
            //    AABB's safety zone but allow motion away from it.
            //
            // 2. ...past the padding and the occluder winnowing step was too liberal and
            //    considered this a candidate occluder.
            //

            // Let's start by detecting the second case.
            if ilerp_f32(
                hull_start,
                hull_end,
                aabb_edge_other.start.axis(face.axis()),
            ) < 0.
            {
                return self.result_clear();
            }

            // Since we already checked that the second case did not occur and since we already
            // ensured that the hull's delta is not pointing in the direction of the `face`, we know
            // that the hull must be trying to penetrate further into the padding occluder. Hence,
            // we should limit the motion permitted by this cast to `0`.
            lerp_to_flush = 0.;
        }

        if lerp_to_flush > 1.0 {
            // The hull couldn't possibly clip this edge—it's too far away!
            return self.result_clear();
        }

        // Our collision math assumes that our edges extend forever but we only care about blocking
        // the hull-cast if the hull actually ends up entering the padding aabb.
        let hull_end_edge = hull_edge.translated(self.delta * lerp_to_flush);

        let perp_axis = face.axis().invert();
        let hull_end_min = hull_end_edge.start.axis(perp_axis);
        let hull_end_max = hull_end_edge.end.axis(perp_axis);

        let aabb_end_min = aabb_edge.start.axis(perp_axis);
        let aabb_end_max = aabb_edge.end.axis(perp_axis);

        // start_2 > end_1 || start_1 > end_2
        if aabb_end_min > hull_end_max || hull_end_min > aabb_end_max {
            return self.result_clear();
        }

        self.result_obstructed(lerp_to_flush, face.invert().as_vec())
    }

    pub fn hull_cast(self, occluder: Aabb) -> HullCastResult {
        let dbg = debug_draw().frame();

        dbg.line_rect(occluder, 5., RED);
        dbg.line_rect(occluder.grow(Vec2::splat(SAFETY_THRESHOLD * 2.)), 5., RED);

        let mut result = self.result_clear();

        for axis in Axis2::AXES {
            let sign = Sign::of_biased(self.delta.axis(axis));
            let face = TileFace::compose(axis, sign);

            let padding = occluder.translate_extend(face.invert().as_vec() * SAFETY_THRESHOLD);
            result = result.min(self.hull_cast_padding(padding, face));

            if padding.intersects(self.start) {
                // Since the hull is starting in the padding, there is no way it could penetrate
                // further into the occluder `aabb` since we deny all motion towards it in the
                // current hull-cast step. By limiting our hull cast to just this padding object,
                // we prevent the scenario where an object is intersecting two adjacent paddings
                // simultaneously, making it act, e.g., as if it were blocked by both the side of a
                // collider and its top.
                break;
            }
        }

        return result;
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
            .grow(Vec2::splat(SAFETY_THRESHOLD * 2.))
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
