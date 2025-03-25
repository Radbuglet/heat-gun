use std::{f32, mem};

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

// === StreamWrites === //

pub trait StreamWrite: Sized {
    fn write_to(&self, out: &mut (impl ?Sized + StreamWriter));
}

pub trait StreamWriteSized: StreamWrite {
    fn len(&self) -> usize;
}

pub trait StreamWriter {
    fn write(&mut self, data: &[u8]);
}

#[derive(Debug)]
pub struct PositionedVecWriter<'a> {
    pub target: &'a mut Vec<u8>,
    pub start: usize,
}

impl StreamWriter for PositionedVecWriter<'_> {
    fn write(&mut self, data: &[u8]) {
        let (data_overlapped, data_extend) = data.split_at(self.target.len() - self.start);

        self.target[self.start..].copy_from_slice(data_overlapped);
        self.target.extend_from_slice(data_extend);
    }
}

#[derive(Debug)]
#[derive_where(Copy, Clone)]
pub struct Bytemuck<'a, T: bytemuck::Pod>(pub &'a T);

impl<T: bytemuck::Pod> StreamWrite for Bytemuck<'_, T> {
    fn write_to(&self, out: &mut (impl ?Sized + StreamWriter)) {
        out.write(bytemuck::bytes_of(self.0));
    }
}

impl<T: bytemuck::Pod> StreamWriteSized for Bytemuck<'_, T> {
    fn len(&self) -> usize {
        mem::size_of::<T>()
    }
}

#[derive(Debug)]
#[derive_where(Copy, Clone)]
pub struct Crevice<'a, T: AsStd430>(pub &'a T);

impl<T: AsStd430> StreamWrite for Crevice<'_, T> {
    fn write_to(&self, out: &mut (impl ?Sized + StreamWriter)) {
        Bytemuck(&self.0.as_std430()).write_to(out);
    }
}

impl<T: AsStd430> StreamWriteSized for Crevice<'_, T> {
    fn len(&self) -> usize {
        T::std430_size_static()
    }
}
