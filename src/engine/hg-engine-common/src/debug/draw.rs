use std::{
    cell::Cell,
    fmt,
    time::{Duration, Instant},
};

use glam::Vec2;
use hg_ecs::{component, Obj, World};

use crate::{
    field,
    utils::{
        lang::{steal_from_ecs, Steal},
        math::{Aabb, RgbaColor, Segment},
    },
};

// === DebugDraw === //

pub type ErasedRenderer = Box<dyn 'static + FnMut(&mut World)>;
pub type ErasedBackend = Box<dyn DebugDrawBackend>;

pub struct DebugDraw {
    backend: ErasedBackend,
    inner: Steal<DebugDrawInner>,
}

#[derive(Default)]
struct DebugDrawInner {
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
    pub fn new(backend: ErasedBackend) -> Self {
        Self {
            backend,
            inner: Steal::Present(DebugDrawInner::default()),
        }
    }

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

    pub fn clear_keyed(mut self: Obj<Self>) {
        self.inner.keyed.clear();
    }

    pub fn render(self: Obj<Self>) {
        let now = Instant::now();

        // Swap `me` with a dummy `DebugContext` so we can mutate ourself without needing to borrow
        // the `DebugContext` component. This is swapped back
        let mut guard = steal_from_ecs(self, field!(Self, inner));
        let (world, inner) = &mut *guard;

        // Do the actual drawing!
        for mut target in inner.ephemeral.drain(..) {
            target(world);
        }

        inner.timed.retain_mut(|(expires_at, target)| {
            if now > *expires_at {
                return false;
            }

            target(world);
            true
        });

        for target in &mut inner.keyed {
            target(world);
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
    pub fn push_erased(self, renderer: ErasedRenderer) {
        let mut ctx = self.ctx;

        match self.mode {
            DebugClearMode::NextFrame => ctx.inner.ephemeral.push(renderer),
            DebugClearMode::Timed(instant) => ctx.inner.timed.push((instant, renderer)),
            DebugClearMode::UntilKey => ctx.inner.keyed.push(renderer),
        }
    }

    pub fn push(self, renderer: impl 'static + FnMut(&mut World)) {
        self.push_erased(Box::new(renderer));
    }
}

// === Debug Draw Methods === //

macro_rules! debug_methods {
    ($(
        fn $name:ident($($arg:ident: $ty:ty),*$(,)?);
    )*) => {
        pub trait DebugDrawBackend {
            $(fn $name(&self, $($arg:$ty),*) -> ErasedRenderer;)*
        }

        impl DebugDrawBound {
            $(pub fn $name(self, $($arg:$ty),*) {
                self.push_erased(self.ctx.backend.$name($($arg),*));
            })*
        }
    };
}

debug_methods! {
    fn segment(segment: Segment, thickness: f32, color: RgbaColor);

    fn vector(segment: Segment, thickness: f32, color: RgbaColor);

    fn vector_scaled(origin: Vec2, delta: Vec2, color: RgbaColor);

    fn rect(aabb: Aabb, color: RgbaColor);

    fn circle(pos: Vec2, radius: f32, color: RgbaColor);

    fn line_rect(aabb: Aabb, thickness: f32, color: RgbaColor);
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
