use std::{
    cell::Cell,
    context::{infer_bundle, pack, Bundle},
    fmt,
    time::{Duration, Instant},
};

use glam::Vec2;
use hg_ecs::{component, Obj, World, WORLD};

use crate::utils::math::{Aabb, RgbaColor, Segment};

const REENTRANCY_MSG: &str = "cannot reentrantly call `DebugDraw` methods while rendering";

// === DebugDraw === //

pub type ErasedRenderer = Box<dyn 'static + FnMut(&mut World)>;
pub type ErasedBackend = Box<dyn DebugDrawBackend>;

pub struct DebugDraw {
    backend: ErasedBackend,
    inner: Option<DebugDrawInner>,
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
            inner: Some(DebugDrawInner::default()),
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

    fn inner_mut(&mut self) -> &mut DebugDrawInner {
        self.inner.as_mut().expect(REENTRANCY_MSG)
    }

    pub fn clear_keyed(mut self: Obj<Self>) {
        self.inner_mut().keyed.clear();
    }

    pub fn render(mut self: Obj<Self>) {
        let now = Instant::now();

        // Swap `me` with a dummy `DebugContext` so we can mutate ourself without needing to borrow
        // the `DebugContext` component. This is swapped back
        let taken = self.inner.take().expect(REENTRANCY_MSG);
        let cx = pack!(@env => Bundle<infer_bundle!('_)>);
        let mut guard = scopeguard::guard((cx, taken), |(cx, taken)| {
            let static ..cx;
            self.inner = Some(taken);
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
            DebugClearMode::NextFrame => ctx.inner_mut().ephemeral.push(renderer),
            DebugClearMode::Timed(instant) => ctx.inner_mut().timed.push((instant, renderer)),
            DebugClearMode::UntilKey => ctx.inner_mut().keyed.push(renderer),
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
