use std::{slice, sync::Arc};

// === DeferSignal === //

#[derive(Debug)]
pub struct DeferSignal<E> {
    events: Arc<Vec<E>>,
    frozen: bool,
}

impl<E> Default for DeferSignal<E> {
    fn default() -> Self {
        Self {
            events: Arc::default(),
            frozen: true,
        }
    }
}

impl<E> DeferSignal<E> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.frozen = false;
        Arc::get_mut(&mut self.events)
            .expect("cannot reset a signal while it's being iterated over")
            .clear();
    }

    pub fn fire(&mut self, event: E) {
        assert!(!self.frozen, "cannot push to a frozen signal");

        Arc::get_mut(&mut self.events)
            .expect("cannot extend a signal while it's being iterated over")
            .push(event);
    }

    pub fn freeze(&mut self) {
        self.frozen = true;
    }

    pub fn reader(&self) -> DeferSignalReader<E> {
        assert!(self.frozen, "cannot iterate over an unfrozen signal");

        DeferSignalReader {
            events: self.events.clone(),
        }
    }

    pub fn iter(&self) -> slice::Iter<'_, E> {
        assert!(self.frozen, "cannot iterate over an unfrozen signal");

        self.events.iter()
    }
}

impl<'a, E> IntoIterator for &'a DeferSignal<E> {
    type Item = &'a E;
    type IntoIter = slice::Iter<'a, E>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Debug, Clone)]
#[must_use]
pub struct DeferSignalReader<E> {
    events: Arc<Vec<E>>,
}

impl<E> DeferSignalReader<E> {
    pub fn iter(&self) -> slice::Iter<'_, E> {
        self.events.iter()
    }
}

impl<'a, E> IntoIterator for &'a DeferSignalReader<E> {
    type Item = &'a E;
    type IntoIter = slice::Iter<'a, E>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
