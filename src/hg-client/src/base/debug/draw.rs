use std::{
    cell::Cell,
    context::{infer_bundle, pack, Bundle},
    fmt, mem,
    time::{Duration, Instant},
};

use hg_ecs::{component, Obj, World, WORLD};
use macroquad::{
    color::Color,
    input::{is_key_pressed, KeyCode},
    math::Vec2,
    shapes::draw_circle,
};

use crate::utils::math::{
    Aabb, Circle, LogisticCurve, MqAabbExt as _, MqCircleExt as _, MqSegmentExt as _, Segment,
};

// === DebugDraw === //

type ErasedRenderer = Box<dyn 'static + FnMut(&mut World)>;

#[derive(Default)]
pub struct DebugDraw {
    ephemeral: Vec<ErasedRenderer>,
    timed: Vec<(Instant, ErasedRenderer)>,
    keyed: Vec<ErasedRenderer>,
}

component!(DebugDraw);

impl fmt::Debug for DebugDraw {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DebugContext").finish_non_exhaustive()
    }
}

impl DebugDraw {
    pub fn bind(self: Obj<Self>, mode: DebugClearMode) -> DebugDrawBound {
        DebugDrawBound { ctx: self, mode }
    }

    pub fn frame(self: Obj<Self>) -> DebugDrawBound {
        self.bind(DebugClearMode::NextFrame)
    }

    pub fn until_key(self: Obj<Self>) -> DebugDrawBound {
        self.bind(DebugClearMode::UntilKey)
    }

    pub fn for_time(self: Obj<Self>, time: Duration) -> DebugDrawBound {
        self.bind(DebugClearMode::Timed(Instant::now() + time))
    }

    pub fn render(mut self: Obj<Self>) {
        let now = Instant::now();

        // Swap `me` with a dummy `DebugContext` so we can mutate ourself without needing to borrow
        // the `DebugContext` component. This is swapped back
        let taken = mem::take(&mut *self);
        let cx = pack!(@env => Bundle<infer_bundle!('_)>);
        let mut guard = scopeguard::guard((cx, taken), |(cx, taken)| {
            let static ..cx;
            *self = taken;
        });

        let (cx, me) = &mut *guard;
        let static ..*cx;

        // Do the actual drawing!
        for mut target in me.ephemeral.drain(..) {
            target(&mut WORLD);
        }

        let world = &mut WORLD;

        me.timed.retain_mut(|(expires_at, target)| {
            if now > *expires_at {
                return false;
            }

            target(world);
            true
        });

        for target in &mut me.keyed {
            target(&mut WORLD);
        }

        if is_key_pressed(KeyCode::L) {
            me.keyed.clear();
        }
    }
}

// === DebugDrawBound === //

#[derive(Debug, Copy, Clone)]
pub struct DebugDrawBound {
    pub ctx: Obj<DebugDraw>,
    pub mode: DebugClearMode,
}

#[derive(Debug, Copy, Clone)]
pub enum DebugClearMode {
    NextFrame,
    Timed(Instant),
    UntilKey,
}

impl DebugDrawBound {
    pub fn push(self, renderer: impl 'static + FnMut(&mut World)) {
        let mut ctx = self.ctx;
        let renderer = Box::new(renderer);

        match self.mode {
            DebugClearMode::NextFrame => ctx.ephemeral.push(renderer),
            DebugClearMode::Timed(instant) => ctx.timed.push((instant, renderer)),
            DebugClearMode::UntilKey => ctx.keyed.push(renderer),
        }
    }

    pub fn segment(self, segment: Segment, thickness: f32, color: Color) {
        self.push(move |_world| {
            segment.draw(thickness, color);
        });
    }

    pub fn vector(self, segment: Segment, thickness: f32, color: Color) {
        self.push(move |_world| {
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
        });
    }

    pub fn vector_scaled(self, origin: Vec2, delta: Vec2, color: Color) {
        let logistic = LogisticCurve {
            max_value: 500.,
            midpoint: 2000.,
            steepness: 0.001,
        };

        let delta = delta.normalize_or_zero() * logistic.compute(delta.length());

        self.vector(Segment::new_delta(origin, delta), 15., color);
    }

    pub fn rect(self, aabb: Aabb, color: Color) {
        self.push(move |_world| {
            aabb.draw_solid(color);
        });
    }

    pub fn circle(self, pos: Vec2, radius: f32, color: Color) {
        self.push(move |_world| {
            draw_circle(pos.x, pos.y, radius, color);
        });
    }

    pub fn line_rect(self, aabb: Aabb, thickness: f32, color: Color) {
        self.push(move |_world| {
            aabb.draw_lines(thickness, color);
        });
    }
}

// === Implicit API === //

thread_local! {
    static CURR_CTX: Cell<Obj<DebugDraw>> = const { Cell::new(Obj::DANGLING) };
}

pub fn set_debug_draw(ctx: Obj<DebugDraw>) {
    CURR_CTX.set(ctx);
}

pub fn debug_draw() -> Obj<DebugDraw> {
    CURR_CTX.get()
}
