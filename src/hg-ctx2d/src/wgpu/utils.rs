use core::f32;
use std::mem;

use crevice::std430::AsStd430;
use derive_where::derive_where;

// === Depth === //

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

// === StreamWritable === //

pub trait StreamWritable {
    fn write_to(&self, out: &mut impl Extend<u8>);
}

impl StreamWritable for [u8] {
    fn write_to(&self, out: &mut impl Extend<u8>) {
        out.extend(self.iter().copied());
    }
}

#[derive(Debug)]
#[derive_where(Copy, Clone)]
pub struct Bytemuck<'a, T>(pub &'a T);

impl<T: bytemuck::Pod> StreamWritable for Bytemuck<'_, T> {
    fn write_to(&self, out: &mut impl Extend<u8>) {
        bytemuck::bytes_of(self.0).write_to(out);
    }
}

#[derive(Debug)]
#[derive_where(Copy, Clone)]
pub struct Crevice<'a, T>(pub &'a T);

impl<T: AsStd430> StreamWritable for Crevice<'_, T> {
    fn write_to(&self, out: &mut impl Extend<u8>) {
        Bytemuck(&self.0.as_std430()).write_to(out);
    }
}
