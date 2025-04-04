use std::{any::Any, fmt, mem, num::NonZeroU32, ops::Range, ptr::NonNull};

use hg_utils::mem::SharedBox;
use smallbox::SmallBox;

// === PassScheduler === //

type QueuedCmd = SmallBox<dyn FnMut(&mut wgpu::RenderPass<'_>), [usize; 2]>;

#[derive(Default)]
pub struct PassScheduler {
    depth_queue: Vec<DepthEpoch>,
    cmd_queue: Vec<QueuedCmd>,
    depth_buckets: Vec<usize>,
    keep_alive: Vec<SharedBox<dyn Any>>,
}

impl fmt::Debug for PassScheduler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DepthScheduler").finish_non_exhaustive()
    }
}

impl PassScheduler {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn push_many<T>(&mut self, ctx: T) -> PassSchedulerMany<'_, T>
    where
        T: 'static,
    {
        PassSchedulerMany {
            scheduler: self,
            keep_alive: KeepAliveState::Stack(ctx),
        }
    }

    pub fn push(
        &mut self,
        depth: DepthEpoch,
        push: impl 'static + FnOnce(&mut wgpu::RenderPass<'_>),
    ) {
        self.depth_queue.push(depth);

        let mut push = Some(push);
        let push = move |pass: &mut wgpu::RenderPass<'_>| push.take().unwrap()(pass);
        self.cmd_queue.push(smallbox::smallbox!(push));

        if self.depth_buckets.len() <= depth.as_index() {
            self.depth_buckets.resize(depth.as_index() + 1, 0);
        }

        self.depth_buckets[depth.as_index()] += 1;
    }

    pub fn exec(mut self, pass: &mut wgpu::RenderPass<'_>) {
        debug_assert_eq!(self.cmd_queue.len(), self.depth_queue.len());

        // Order commands with an in-place counting sort.
        let mut accum = 0;

        for bucket in &mut self.depth_buckets {
            let count = *bucket;
            *bucket += accum;
            accum += count;
        }

        for i in 0..self.cmd_queue.len() {
            loop {
                let target_bucket = self.depth_queue[i].as_index();
                let target_slot = &mut self.depth_buckets[target_bucket];

                if *target_slot == i {
                    break;
                }

                self.depth_queue.swap(i, *target_slot);
                self.cmd_queue.swap(i, *target_slot);
                *target_slot -= 1;
            }
        }

        // Push passes in order.
        for mut cmd in self.cmd_queue {
            cmd(pass);
        }
    }
}

pub struct PassSchedulerMany<'a, T: 'static> {
    scheduler: &'a mut PassScheduler,
    keep_alive: KeepAliveState<T>,
}

enum KeepAliveState<T> {
    Stack(T),
    Boxed(NonNull<T>),
    Placeholder,
}

impl<'a, T: 'static> PassSchedulerMany<'a, T> {
    pub fn push(
        &mut self,
        depth: DepthEpoch,
        push: impl 'static + FnOnce(&mut wgpu::RenderPass<'_>, &mut T),
    ) -> &mut Self {
        let mut ctx = match mem::replace(&mut self.keep_alive, KeepAliveState::Placeholder) {
            KeepAliveState::Stack(value) => {
                let ctx_box = SharedBox::from_box(Box::new(value) as Box<dyn Any>);
                let ctx = ctx_box.get().cast::<T>();
                self.scheduler.keep_alive.push(ctx_box);

                self.keep_alive = KeepAliveState::Boxed(ctx);
                ctx
            }
            KeepAliveState::Boxed(ctx) => ctx,
            KeepAliveState::Placeholder => unreachable!(),
        };

        self.scheduler
            .push(depth, move |pass| push(pass, unsafe { ctx.as_mut() }));

        self
    }
}

// === Generator === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct DepthEpoch(pub NonZeroU32);

impl DepthEpoch {
    fn as_index(self) -> usize {
        (self.0.get() - 1) as usize
    }
}

#[derive(Debug)]
pub struct DepthGenerator {
    epoch: DepthEpoch,
    depth: u32,
}

impl Default for DepthGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl DepthGenerator {
    pub const fn new() -> Self {
        Self {
            epoch: DepthEpoch(match NonZeroU32::new(1) {
                Some(v) => v,
                None => unreachable!(),
            }),
            depth: 0,
        }
    }

    pub fn reset(&mut self) {
        mem::take(self);
    }

    #[must_use]
    pub fn epoch(&self) -> DepthEpoch {
        self.epoch
    }

    #[must_use]
    pub fn depth(&self) -> f32 {
        todo!()
    }

    pub fn next_depth(&mut self) {
        self.depth += 1;
        if self.depth() >= 1.0 {
            self.next_epoch();
        }
    }

    pub fn next_epoch(&mut self) {
        self.depth = 0;
        self.epoch.0 = self.epoch.0.checked_add(1).unwrap();
    }
}

// === DepthRuns === //

#[derive(Debug)]
pub struct DepthRuns<T> {
    points: Vec<DepthRunPoint<T>>,
}

#[derive(Debug)]
pub struct DepthRunPoint<T> {
    pub epoch: Option<DepthEpoch>,
    pub pos: T,
}

impl<T: Copy> DepthRuns<T> {
    pub fn new(start: T) -> Self {
        Self {
            points: vec![DepthRunPoint {
                epoch: None,
                pos: start,
            }],
        }
    }

    pub fn record_pos(&mut self, epoch: DepthEpoch, pos: T) {
        let latest = self.points.last_mut().unwrap();

        if latest.epoch != Some(epoch) {
            self.points.push(DepthRunPoint {
                epoch: Some(epoch),
                pos,
            });
        } else {
            latest.pos = pos;
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (DepthEpoch, Range<T>)> + '_ {
        self.points
            .windows(2)
            .map(|v| (v[1].epoch.unwrap(), v[0].pos..v[1].pos))
    }

    pub fn points(&self) -> &[DepthRunPoint<T>] {
        &self.points
    }

    pub fn last_pos(&self) -> T {
        self.points.last().unwrap().pos
    }

    pub fn last_epoch(&self) -> Option<DepthEpoch> {
        self.points.last().unwrap().epoch
    }

    pub fn reset(&mut self, start: T) {
        self.points.clear();
        self.points.push(DepthRunPoint {
            epoch: None,
            pos: start,
        });
    }
}
