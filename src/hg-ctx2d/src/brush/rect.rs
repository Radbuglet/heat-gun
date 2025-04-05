use crate::base::{RawBrushHandle, RawCanvas};

#[derive(Debug)]
pub struct RectBrush {
    handle: RawBrushHandle,
}

impl RectBrush {
    pub fn new() -> Self {
        todo!()
    }

    pub fn push(&mut self, ctx: &mut RawCanvas) {
        // TODO
    }
}
