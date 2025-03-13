use core::f32;
use std::mem;

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct DepthEpoch(pub u16);

#[derive(Debug, Clone)]
pub struct DepthGenerator {
    pub epoch: DepthEpoch,
    pub value: u32,
}

impl Default for DepthGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl DepthGenerator {
    pub const fn new() -> Self {
        Self {
            epoch: DepthEpoch(0),
            value: 0,
        }
    }

    pub fn reset(&mut self) {
        mem::take(self);
    }

    pub fn curr(&self) -> Depth {
        Depth {
            epoch: self.epoch,
            value: f32::from_bits(self.value),
        }
    }

    pub fn next(&mut self) {
        self.value += 1;

        if self.value > 1.0f32.to_bits() {
            self.value = 0;
            self.epoch.0 += 1;
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Depth {
    pub epoch: DepthEpoch,
    pub value: f32,
}
