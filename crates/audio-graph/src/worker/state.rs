use std::{
    collections::VecDeque,
    error::Error,
    ops::{Deref, DerefMut},
    sync::{Arc, LockResult, Mutex, MutexGuard},
};

use bevy_ecs::entity::{Entity, EntityHashMap};

#[derive(Default, Clone)]
pub struct StateTracker {
    inner: Arc<Mutex<Inner>>,
}

// free_buffers - front: next to write to, back: next to read from

impl StateTracker {
    pub fn get_buffer(&mut self) -> StateBufferGuard {
        let mut inner = self.inner.lock().unwrap();
        let buffer = inner.free_buffers.pop_back().unwrap();

        StateBufferGuard {
            inner: self.inner.clone(),
            buffer: Some(buffer),
        }
    }

    pub fn get_buffer_mut(&mut self) -> StateBufferGuardMut {
        let mut inner = self.inner.lock().unwrap();
        let buffer = inner.free_buffers.pop_front().unwrap();

        StateBufferGuardMut {
            inner: self.inner.clone(),
            buffer: Some(buffer),
        }
    }
}

struct Inner {
    free_buffers: VecDeque<StateBuffer>,
}

impl Default for Inner {
    fn default() -> Self {
        let mut free_buffers = VecDeque::default();
        free_buffers.resize_with(2, StateBuffer::default);

        Self { free_buffers }
    }
}

pub struct StateBufferGuard {
    inner: Arc<Mutex<Inner>>,
    buffer: Option<StateBuffer>,
}

impl Deref for StateBufferGuard {
    type Target = StateBuffer;

    fn deref(&self) -> &Self::Target {
        self.buffer.as_ref().unwrap()
    }
}

impl Drop for StateBufferGuard {
    fn drop(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            let mut inner = self.inner.lock().unwrap();
            inner.free_buffers.push_front(buffer);
        }
    }
}

pub struct StateBufferGuardMut {
    inner: Arc<Mutex<Inner>>,
    buffer: Option<StateBuffer>,
}

impl Deref for StateBufferGuardMut {
    type Target = StateBuffer;

    fn deref(&self) -> &Self::Target {
        self.buffer.as_ref().unwrap()
    }
}

impl DerefMut for StateBufferGuardMut {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.buffer.as_mut().unwrap()
    }
}

impl Drop for StateBufferGuardMut {
    fn drop(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            let mut inner = self.inner.lock().unwrap();
            inner.free_buffers.push_back(buffer);
        }
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
    fn test_state_tracker() {
        let mut s = StateTracker::default();
        let entity_a = Entity::from_raw_u32(1).unwrap();

        let mut a = s.get_buffer_mut();
        let mut b = s.get_buffer_mut();

        a.insert(entity_a, StateValue::Mono(1.0));

        assert_eq!(b.get(&entity_a), None);
        b.insert(entity_a, StateValue::Stereo(1.0, 1.0));

        drop(a);
        drop(b);
        let b = s.get_buffer();
        assert_eq!(b.get(&entity_a), Some(&StateValue::Mono(1.0)));

        drop(b);
        let a = s.get_buffer();
        assert_eq!(a.get(&entity_a), Some(&StateValue::Stereo(1.0, 1.0)));
        let b = s.get_buffer();
        assert_eq!(b.get(&entity_a), Some(&StateValue::Mono(1.0)));
    }
}
