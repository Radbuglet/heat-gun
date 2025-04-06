use std::{mem, num::NonZeroU32, ops::Range};

// === Generator === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct DepthEpoch(pub NonZeroU32);

#[derive(Debug)]
pub(super) struct DepthGenerator {
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
        f32::MIN_POSITIVE + f32::from_bits(self.depth)
    }

    pub fn advance_depth(&mut self) {
        self.depth += 1;
        if self.depth() >= 1.0 {
            self.advance_epoch();
        }
    }

    pub fn advance_epoch(&mut self) {
        self.depth = 0;
        self.epoch.0 = self.epoch.0.checked_add(1).unwrap();
    }
}

// === InstanceRuns === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub struct RunIndex(u32);

#[derive(Debug, Default)]
pub(crate) struct InstanceRuns {
    starts: Vec<u32>,
    last_epoch: Option<DepthEpoch>,
    len: u32,
}

impl InstanceRuns {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.starts.clear();
        self.last_epoch = None;
        self.len = 0;
    }

    #[must_use]
    pub fn push(&mut self, epoch: DepthEpoch, count: u32) -> Option<RunIndex> {
        if self.last_epoch == Some(epoch) {
            self.len += count;
            return None;
        }

        debug_assert!(epoch.0.get() > self.last_epoch.map_or(0, |v| v.0.get()));

        let idx = RunIndex(self.starts.len() as u32);
        self.starts.push(self.len);
        self.len += count;

        Some(idx)
    }

    pub fn len(&self) -> u32 {
        self.len
    }

    pub fn range(&self, idx: RunIndex) -> Range<u32> {
        let start = self.starts[idx.0 as usize];
        let end = self
            .starts
            .get(idx.0 as usize + 1)
            .copied()
            .unwrap_or(self.len);

        start..end
    }
}
