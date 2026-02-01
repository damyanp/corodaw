use std::{
    collections::VecDeque,
    error::Error,
    mem::swap,
    ops::{Deref, DerefMut},
    sync::{Arc, LockResult, Mutex, MutexGuard},
};

use bevy_ecs::entity::{Entity, EntityHashMap};

pub fn state_tracker() -> (StateReader, StateWriter) {
    let inner: Arc<Mutex<Inner>> = Arc::default();

    let reader = StateReader {
        inner: inner.clone(),
        buffer: StateBuffer::default(),
    };

    let writer = StateWriter {
        inner,
        buffer: StateBuffer::default(),
    };

    (reader, writer)
}

pub struct StateReader {
    inner: Arc<Mutex<Inner>>,
    buffer: StateBuffer,
}

pub struct StateWriter {
    inner: Arc<Mutex<Inner>>,
    buffer: StateBuffer,
}

struct Inner {
    ready_to_read_buffer: Option<StateBuffer>,
    ready_to_write_buffer: Option<StateBuffer>,
}

impl Default for Inner {
    fn default() -> Self {
        // We start with one extra buffer reader for writing to.
        Self {
            ready_to_read_buffer: Default::default(),
            ready_to_write_buffer: Some(StateBuffer::default()),
        }
    }
}

impl StateReader {
    pub fn swap_buffers(&mut self) {
        let mut inner = self.inner.lock().unwrap();

        if let Some(mut buffer) = inner.ready_to_read_buffer.take() {
            assert!(inner.ready_to_write_buffer.is_none());
            swap(&mut self.buffer, &mut buffer);
            inner.ready_to_write_buffer = Some(buffer);
        }
    }
}

impl Deref for StateReader {
    type Target = StateBuffer;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl StateWriter {
    pub fn swap_buffers(&mut self) {
        let mut inner = self.inner.lock().unwrap();

        // ready_to_read_buffer should always contain the most recently written
        // one. If there's one there already, then we must have put it there
        // previously, so we swap our one with that one.
        if let Some(mut buffer) = inner.ready_to_read_buffer.take() {
            swap(&mut self.buffer, &mut buffer);
            inner.ready_to_read_buffer = Some(buffer);
        }
        // if there's no buffer in ready_to_read then that means that something
        // is reading from it. But there might be one for us in ready_to_write,
        // so we can use that
        else if let Some(mut buffer) = inner.ready_to_write_buffer.take() {
            assert!(inner.ready_to_read_buffer.is_none());
            swap(&mut self.buffer, &mut buffer);
            inner.ready_to_read_buffer = Some(buffer);
        }

        // There's nothing for us to swap with, so we'll have to reuse this buffer.
    }
}

impl Deref for StateWriter {
    type Target = StateBuffer;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl DerefMut for StateWriter {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StateValue {
    None,
    Mono(f32),
    Stereo(f32, f32),
}

#[derive(Default, Debug)]
pub struct StateBuffer {
    data: EntityHashMap<StateValue>,
}

impl StateBuffer {
    pub fn insert(&mut self, key: Entity, value: StateValue) -> Option<StateValue> {
        self.data.insert(key, value)
    }

    pub fn get(&self, key: &Entity) -> Option<&StateValue> {
        self.data.get(key)
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_state_tracker() {}
}
