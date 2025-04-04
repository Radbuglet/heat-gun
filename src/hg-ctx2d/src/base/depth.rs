use std::{fmt, mem, num::NonZeroU32};

use smallbox::SmallBox;

// === PassScheduler === //

type QueuedCmd = SmallBox<dyn FnMut(&mut wgpu::RenderPass<'_>), [usize; 2]>;

#[derive(Default)]
pub struct PassScheduler {
    depth_queue: Vec<DepthEpoch>,
    cmd_queue: Vec<QueuedCmd>,
    depth_buckets: Vec<usize>,
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
