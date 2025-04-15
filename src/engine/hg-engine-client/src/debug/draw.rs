use hg_engine_common::{
    debug::{DebugDrawBackend, ErasedBackend, ErasedRenderer},
    utils::math::{Aabb, Circle, LogisticCurve, RgbaColor, Segment},
};
use macroquad::math::Vec2;

use crate::utils::macroquad_ext::{MqAabbExt as _, MqCircleExt as _, MqSegmentExt as _};

pub fn debug_draw_macroquad() -> ErasedBackend {
    Box::new(DebugDrawMacroquad)
}

struct DebugDrawMacroquad;

impl DebugDrawBackend for DebugDrawMacroquad {
    fn segment(&self, segment: Segment, thickness: f32, color: RgbaColor) -> ErasedRenderer {
        Box::new(move |_world| {
            segment.draw(thickness, color);
        })
    }

    fn vector(&self, segment: Segment, thickness: f32, color: RgbaColor) -> ErasedRenderer {
        Box::new(move |_world| {
            Circle::new(segment.start, thickness / 2.).draw(color);
            Circle::new(segment.end, thickness / 2.).draw(color);
            segment.draw(thickness, color);

            let new_segment = segment
                .translated(segment.delta())
                .normalized_or_zero()
                .scaled(50.);
            new_segment
                .rotated_ccw_deg(90. + 45.)
                .draw(thickness, color);
            new_segment.rotated_cw_deg(90. + 45.).draw(thickness, color);
        })
    }

    fn vector_scaled(&self, origin: Vec2, delta: Vec2, color: RgbaColor) -> ErasedRenderer {
        let logistic = LogisticCurve {
            max_value: 500.,
            midpoint: 2000.,
            steepness: 0.001,
        };

        let delta = delta.normalize_or_zero() * logistic.compute(delta.length());

        self.vector(Segment::new_delta(origin, delta), 15., color)
    }

    fn rect(&self, aabb: Aabb, color: RgbaColor) -> ErasedRenderer {
        Box::new(move |_world| {
            aabb.draw_solid(color);
        })
    }

    fn circle(&self, pos: Vec2, radius: f32, color: RgbaColor) -> ErasedRenderer {
        Box::new(move |_world| {
            Circle::new(pos, radius).draw(color);
        })
    }

    fn line_rect(&self, aabb: Aabb, thickness: f32, color: RgbaColor) -> ErasedRenderer {
        Box::new(move |_world| {
            aabb.draw_lines(thickness, color);
        })
    }
}
