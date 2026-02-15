use std::{
    mem::swap,
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex},
};

use bevy_ecs::entity::{Entity, EntityHashMap};

pub fn graph_state_tracker() -> (GraphStateReader, GraphStateWriter) {
    let inner: Arc<Mutex<Inner>> = Arc::default();

    let reader = GraphStateReader {
        inner: inner.clone(),
        buffer: GraphStateBuffer::default(),
    };

    let writer = GraphStateWriter {
        inner,
        buffer: GraphStateBuffer::default(),
    };

    (reader, writer)
}

pub struct GraphStateReader {
    inner: Arc<Mutex<Inner>>,
    buffer: GraphStateBuffer,
}

pub struct GraphStateWriter {
    inner: Arc<Mutex<Inner>>,
    buffer: GraphStateBuffer,
}

struct Inner {
    ready_to_read_buffer: Option<GraphStateBuffer>,
    ready_to_write_buffer: Option<GraphStateBuffer>,
}

impl Default for Inner {
    fn default() -> Self {
        // We start with one extra buffer reader for writing to.
        Self {
            ready_to_read_buffer: Default::default(),
            ready_to_write_buffer: Some(GraphStateBuffer::default()),
        }
    }
}

impl GraphStateReader {
    pub fn swap_buffers(&mut self) {
        let mut inner = self.inner.lock().unwrap();

        if let Some(mut buffer) = inner.ready_to_read_buffer.take() {
            assert!(inner.ready_to_write_buffer.is_none());
            swap(&mut self.buffer, &mut buffer);
            inner.ready_to_write_buffer = Some(buffer);
        }
    }
}

impl Deref for GraphStateReader {
    type Target = GraphStateBuffer;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl GraphStateWriter {
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

impl Deref for GraphStateWriter {
    type Target = GraphStateBuffer;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl DerefMut for GraphStateWriter {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GraphStateValue {
    None,
    Mono(f32),
    Stereo(f32, f32),
}

#[derive(Default, Debug)]
pub struct GraphStateBuffer {
    data: EntityHashMap<GraphStateValue>,
}

impl GraphStateBuffer {
    pub fn insert(&mut self, key: Entity, value: GraphStateValue) -> Option<GraphStateValue> {
        self.data.insert(key, value)
    }

    pub fn get(&self, key: &Entity) -> Option<&GraphStateValue> {
        self.data.get(key)
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn entity(id: u32) -> Entity {
        Entity::from_raw_u32(id).expect("test entity id should be valid")
    }

    #[test]
    fn reader_starts_empty() {
        let (mut reader, _writer) = graph_state_tracker();

        reader.swap_buffers();

        assert_eq!(reader.get(&entity(1)), None);
    }

    #[test]
    fn reader_sees_written_values_after_swaps() {
        let (mut reader, mut writer) = graph_state_tracker();

        writer.insert(entity(1), GraphStateValue::Mono(0.5));
        writer.insert(entity(2), GraphStateValue::Stereo(0.1, 0.2));

        writer.swap_buffers();
        reader.swap_buffers();

        assert_eq!(reader.get(&entity(1)), Some(&GraphStateValue::Mono(0.5)));
        assert_eq!(
            reader.get(&entity(2)),
            Some(&GraphStateValue::Stereo(0.1, 0.2))
        );
    }

    #[test]
    fn reader_does_not_see_values_without_writer_swap() {
        let (mut reader, mut writer) = graph_state_tracker();

        writer.insert(entity(3), GraphStateValue::Mono(1.0));

        reader.swap_buffers();

        assert_eq!(reader.get(&entity(3)), None);
    }

    #[test]
    fn latest_write_wins_before_reader_swap() {
        let (mut reader, mut writer) = graph_state_tracker();

        writer.insert(entity(4), GraphStateValue::Mono(0.25));
        writer.swap_buffers();

        writer.insert(entity(4), GraphStateValue::Mono(0.75));
        writer.swap_buffers();

        reader.swap_buffers();

        assert_eq!(reader.get(&entity(4)), Some(&GraphStateValue::Mono(0.75)));
    }
}
