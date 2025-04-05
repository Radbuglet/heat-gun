use std::{mem, num::NonZeroU32};

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
