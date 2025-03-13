pub trait Context: Sized {
    fn fill_rect(&mut self, x: f32, y: f32, width: f32, height: f32);
}
