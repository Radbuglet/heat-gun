use std::ops::Range;

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Default)]
pub struct CopyRange<T> {
    pub start: T,
    pub end: T,
}

impl<T> CopyRange<T> {
    pub fn new(range: Range<T>) -> Self {
        Self {
            start: range.start,
            end: range.end,
        }
    }

    pub fn range(self) -> Range<T> {
        self.start..self.end
    }
}

impl<T> From<Range<T>> for CopyRange<T> {
    fn from(value: Range<T>) -> Self {
        Self {
            start: value.start,
            end: value.end,
        }
    }
}

impl<T> Into<Range<T>> for CopyRange<T> {
    fn into(self) -> Range<T> {
        self.start..self.end
    }
}
