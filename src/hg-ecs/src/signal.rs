use std::{slice, sync::Arc};

// === SimpleSignal === //

#[derive(Debug)]
pub struct SimpleSignal<E> {
    events: Arc<Vec<E>>,
    locked: bool,
}

impl<E> Default for SimpleSignal<E> {
    fn default() -> Self {
        Self {
            events: Arc::default(),
            locked: true,
        }
    }
}

impl<E> SimpleSignal<E> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.locked = false;
        Arc::get_mut(&mut self.events)
            .expect("cannot reset a signal while it's being iterated over")
            .clear();
    }

    pub fn fire(&mut self, event: E) {
        assert!(!self.locked, "cannot push to a locked signal");

        Arc::get_mut(&mut self.events)
            .expect("cannot extend a signal while it's being iterated over")
            .push(event);
    }

    pub fn lock(&mut self) {
        self.locked = true;
    }

    pub fn reader(&self) -> SimpleSignalReader<E> {
        assert!(self.locked, "cannot iterate over an unlocked signal");

        SimpleSignalReader {
            events: self.events.clone(),
        }
    }

    pub fn iter(&self) -> slice::Iter<'_, E> {
        assert!(self.locked, "cannot iterate over an unlocked signal");

        self.events.iter()
    }
}

impl<'a, E> IntoIterator for &'a SimpleSignal<E> {
    type Item = &'a E;
    type IntoIter = slice::Iter<'a, E>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Debug, Clone)]
pub struct SimpleSignalReader<E> {
    events: Arc<Vec<E>>,
}

impl<E> SimpleSignalReader<E> {
    pub fn iter(&self) -> slice::Iter<'_, E> {
        self.events.iter()
    }
}

impl<'a, E> IntoIterator for &'a SimpleSignalReader<E> {
    type Item = &'a E;
    type IntoIter = slice::Iter<'a, E>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
