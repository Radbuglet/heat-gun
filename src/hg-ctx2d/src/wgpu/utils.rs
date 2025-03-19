use core::f32;
use std::{marker::PhantomData, mem};

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

pub trait StreamWritesSized: Sized {
    fn size(&self) -> usize;
}

pub trait StreamWrites<T>: Sized {
    fn write(value: &T, out: &mut impl Extend<u8>);
}

pub trait StreamWritable<S>: Sized {
    fn write_to(&self, out: &mut impl Extend<u8>);
}

impl<T, S> StreamWritable<S> for T
where
    S: StreamWrites<T>,
{
    fn write_to(&self, out: &mut impl Extend<u8>) {
        S::write(self, out);
    }
}

#[derive_where(Debug, Copy, Clone)]
pub struct Bytemuck<T: bytemuck::Pod>(PhantomData<fn(T) -> T>);

impl<T: bytemuck::Pod> Default for Bytemuck<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: bytemuck::Pod> Bytemuck<T> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T: bytemuck::Pod> StreamWritesSized for Bytemuck<T> {
    fn size(&self) -> usize {
        mem::size_of::<T>()
    }
}

impl<T: bytemuck::Pod> StreamWrites<T> for Bytemuck<T> {
    fn write(value: &T, out: &mut impl Extend<u8>) {
        out.extend(bytemuck::bytes_of(value).iter().copied());
    }
}

#[derive_where(Debug, Copy, Clone)]
pub struct Crevice<T: AsStd430>(PhantomData<fn(T) -> T>);

impl<T: AsStd430> Default for Crevice<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: AsStd430> Crevice<T> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T: AsStd430> StreamWritesSized for Crevice<T> {
    fn size(&self) -> usize {
        T::std430_size_static()
    }
}

impl<T: AsStd430> StreamWrites<T> for Crevice<T> {
    fn write(value: &T, out: &mut impl Extend<u8>) {
        Bytemuck::<T::Output>::write(&value.as_std430(), out);
    }
}
