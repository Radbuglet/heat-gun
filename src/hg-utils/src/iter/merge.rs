use std::{cmp::Ordering, iter::Peekable};

// === MergeIter === //

#[derive(Debug, Clone)]
pub struct MergeIter<T, IL, IR>
where
    IL: Iterator<Item = T>,
    IR: Iterator<Item = T>,
{
    left: Peekable<IL>,
    right: Peekable<IR>,
}

impl<T, IL, IR> MergeIter<T, IL, IR>
where
    IL: Iterator<Item = T>,
    IR: Iterator<Item = T>,
{
    pub fn new(
        left: impl IntoIterator<IntoIter = IL>,
        right: impl IntoIterator<IntoIter = IR>,
    ) -> Self {
        Self {
            left: left.into_iter().peekable(),
            right: right.into_iter().peekable(),
        }
    }
}

impl<T, IL, IR> Iterator for MergeIter<T, IL, IR>
where
    T: Ord,
    IL: Iterator<Item = T>,
    IR: Iterator<Item = T>,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        match (self.left.peek(), self.right.peek()) {
            (None, None) => None,
            (None, Some(_)) => Some(self.right.next().unwrap()),
            (Some(_), None) => Some(self.left.next().unwrap()),
            (Some(left), Some(right)) => match left.cmp(right) {
                Ordering::Less => Some(self.left.next().unwrap()),
                Ordering::Equal => {
                    let _discard = self.right.next().unwrap();
                    Some(self.left.next().unwrap())
                }
                Ordering::Greater => Some(self.right.next().unwrap()),
            },
        }
    }
}

// === RemoveIter === //

#[derive(Debug, Clone)]
pub struct RemoveIter<T, IL, IR>
where
    IL: Iterator<Item = T>,
    IR: Iterator<Item = T>,
{
    left: Peekable<IL>,
    right: Peekable<IR>,
}

impl<T, IL, IR> RemoveIter<T, IL, IR>
where
    IL: Iterator<Item = T>,
    IR: Iterator<Item = T>,
{
    pub fn new(
        left: impl IntoIterator<IntoIter = IL>,
        right: impl IntoIterator<IntoIter = IR>,
    ) -> Self {
        Self {
            left: left.into_iter().peekable(),
            right: right.into_iter().peekable(),
        }
    }
}

impl<T, IL, IR> Iterator for RemoveIter<T, IL, IR>
where
    T: Ord,
    IL: Iterator<Item = T>,
    IR: Iterator<Item = T>,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let next = self.left.peek()?;

            match self.right.peek() {
                Some(right) => match next.cmp(right) {
                    Ordering::Less => {
                        let _discard = self.right.next();
                    }
                    Ordering::Equal => {
                        let _discard = self.left.next();
                        continue;
                    }
                    Ordering::Greater => {
                        // (fallthrough)
                    }
                },
                None => {
                    // (fallthrough)
                }
            }

            return Some(self.left.next().unwrap());
        }
    }
}
