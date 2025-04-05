use std::mem;

use crevice::std430::AsStd430;
use derive_where::derive_where;

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
pub(super) struct PositionedVecWriter<'a> {
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

#[derive(Debug, Copy, Clone)]
pub struct SliceStream<'a>(pub &'a [u8]);

impl StreamWrite for SliceStream<'_> {
    fn write_to(&self, out: &mut (impl ?Sized + StreamWriter)) {
        out.write(self.0);
    }
}

impl StreamWriteSized for SliceStream<'_> {
    fn len(&self) -> usize {
        self.0.len()
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
