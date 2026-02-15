use bevy_ecs::{name::Name, prelude::*};

use crate::commands::EditCommand;
use crate::{ChannelOrder, StableId};

use super::components::{ChannelButton, ChannelMixerState, ChannelPluginBinding, ChannelSnapshot};

#[derive(Debug)]
pub struct RenameChannelEdit {
    channel: StableId,
    name: String,
}

impl RenameChannelEdit {
    pub fn new(channel: StableId, name: String) -> Self {
        Self { channel, name }
    }
}

impl EditCommand for RenameChannelEdit {
    fn execute(&self, world: &mut World) -> Option<Box<dyn EditCommand>> {
        let entity = self.channel.find_entity(world)?;
        let mut name = world.get_mut::<Name>(entity)?;
        let old_name = name.as_str().to_owned();
        name.set(self.name.clone());
        Some(Box::new(RenameChannelEdit::new(self.channel, old_name)))
    }
}

#[derive(Debug)]
pub struct ChannelButtonEdit {
    channel: StableId,
    button: ChannelButton,
    value: bool,
}

impl ChannelButtonEdit {
    pub fn new(channel: StableId, button: ChannelButton, value: bool) -> Self {
        Self {
            channel,
            button,
            value,
        }
    }
}

impl EditCommand for ChannelButtonEdit {
    fn execute(&self, world: &mut World) -> Option<Box<dyn EditCommand>> {
        let entity = self.channel.find_entity(world)?;
        let mut state = world.get_mut::<ChannelMixerState>(entity)?;
        let old_value = state.get_button(self.button);
        state.set_button(self.button, self.value);
        Some(Box::new(ChannelButtonEdit::new(
            self.channel,
            self.button,
            old_value,
        )))
    }
}

#[derive(Debug)]
pub struct AddChannelEdit {
    index: usize,
    snapshot: ChannelSnapshot,
}

impl AddChannelEdit {
    pub fn new(index: usize, snapshot: ChannelSnapshot) -> Self {
        Self { index, snapshot }
    }
}

impl EditCommand for AddChannelEdit {
    fn execute(&self, world: &mut World) -> Option<Box<dyn EditCommand>> {
        let mut entity = world.spawn((
            self.snapshot.state.clone(),
            self.snapshot.name.clone(),
            self.snapshot.id,
        ));
        if let Some(data) = &self.snapshot.data {
            entity.insert(data.clone());
        }
        let entity_id = entity.id();

        let mut query = world.query::<&mut ChannelOrder>();
        let mut channel_order = query.single_mut(world).ok()?;
        channel_order.channel_order.insert(self.index, entity_id);

        Some(Box::new(DeleteChannelEdit::new(
            self.snapshot.id,
            self.index,
        )))
    }
}

#[derive(Debug)]
pub struct DeleteChannelEdit {
    channel: StableId,
    index: usize,
}

impl DeleteChannelEdit {
    pub fn new(channel: StableId, index: usize) -> Self {
        Self { channel, index }
    }
}

impl EditCommand for DeleteChannelEdit {
    fn execute(&self, world: &mut World) -> Option<Box<dyn EditCommand>> {
        let entity = self.channel.find_entity(world)?;

        let name = world.get::<Name>(entity)?.clone();
        let state = world.get::<ChannelMixerState>(entity)?.clone();
        let data = world.get::<ChannelPluginBinding>(entity).cloned();
        let id = *world.get::<StableId>(entity)?;

        let mut query = world.query::<&mut ChannelOrder>();
        let mut channel_order = query.single_mut(world).ok()?;
        channel_order.channel_order.retain(|&e| e != entity);

        world.despawn(entity);

        let snapshot = ChannelSnapshot {
            name,
            state,
            data,
            id,
        };

        Some(Box::new(AddChannelEdit::new(self.index, snapshot)))
    }
}

#[derive(Debug)]
pub struct MoveChannelEdit {
    from: usize,
    to: usize,
}

impl MoveChannelEdit {
    pub fn new(from: usize, to: usize) -> Self {
        Self { from, to }
    }

    pub fn apply(&self, channel_order: &mut ChannelOrder) -> Box<dyn EditCommand> {
        channel_order.move_channel(self.from, self.to);
        let undo = if self.from < self.to {
            Self::new(self.to - 1, self.from)
        } else if self.from > self.to {
            Self::new(self.to, self.from + 1)
        } else {
            Self::new(self.from, self.to)
        };
        Box::new(undo)
    }
}

impl EditCommand for MoveChannelEdit {
    fn execute(&self, world: &mut World) -> Option<Box<dyn EditCommand>> {
        let mut query = world.query::<&mut ChannelOrder>();
        let mut channel_order = query.single_mut(world).ok()?;
        let undo = self.apply(&mut channel_order);
        Some(undo)
    }
}

#[derive(Debug)]
pub struct SetPluginEdit {
    channel: StableId,
    data: Option<ChannelPluginBinding>,
}

impl SetPluginEdit {
    pub fn new(channel: StableId, data: Option<ChannelPluginBinding>) -> Self {
        Self { channel, data }
    }
}

impl EditCommand for SetPluginEdit {
    fn execute(&self, world: &mut World) -> Option<Box<dyn EditCommand>> {
        let entity = self.channel.find_entity(world)?;
        let old_data = world.entity_mut(entity).take::<ChannelPluginBinding>();
        if let Some(data) = &self.data {
            world.entity_mut(entity).insert(data.clone());
        }
        Some(Box::new(SetPluginEdit::new(self.channel, old_data)))
    }
}

#[derive(Debug)]
pub struct SetGainEdit {
    channel: StableId,
    gain_value: f32,
}

impl SetGainEdit {
    pub fn new(channel: StableId, gain_value: f32) -> Self {
        Self {
            channel,
            gain_value,
        }
    }
}

impl EditCommand for SetGainEdit {
    fn execute(&self, world: &mut World) -> Option<Box<dyn EditCommand>> {
        let entity = self.channel.find_entity(world)?;
        let mut state = world.get_mut::<ChannelMixerState>(entity)?;
        let old_value = state.gain_value;
        state.gain_value = self.gain_value;
        Some(Box::new(SetGainEdit::new(self.channel, old_value)))
    }
}
