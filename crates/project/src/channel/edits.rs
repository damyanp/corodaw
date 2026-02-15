use bevy_ecs::{name::Name, prelude::*};

use crate::commands::Command;
use crate::{ChannelOrder, Id};

use super::{ChannelButton, ChannelData, ChannelSnapshot, ChannelState};

#[derive(Debug)]
pub struct RenameChannelCommand {
    channel: Id,
    name: String,
}

impl RenameChannelCommand {
    pub fn new(channel: Id, name: String) -> Self {
        Self { channel, name }
    }
}

impl Command for RenameChannelCommand {
    fn execute(&self, world: &mut World) -> Option<Box<dyn Command>> {
        let entity = self.channel.find_entity(world)?;
        let mut name = world.get_mut::<Name>(entity)?;
        let old_name = name.as_str().to_owned();
        name.set(self.name.clone());
        Some(Box::new(RenameChannelCommand::new(self.channel, old_name)))
    }
}

#[derive(Debug)]
pub struct ChannelButtonCommand {
    channel: Id,
    button: ChannelButton,
    value: bool,
}

impl ChannelButtonCommand {
    pub fn new(channel: Id, button: ChannelButton, value: bool) -> Self {
        Self {
            channel,
            button,
            value,
        }
    }
}

impl Command for ChannelButtonCommand {
    fn execute(&self, world: &mut World) -> Option<Box<dyn Command>> {
        let entity = self.channel.find_entity(world)?;
        let mut state = world.get_mut::<ChannelState>(entity)?;
        let old_value = state.get_button(self.button);
        state.set_button(self.button, self.value);
        Some(Box::new(ChannelButtonCommand::new(
            self.channel,
            self.button,
            old_value,
        )))
    }
}

#[derive(Debug)]
pub struct AddChannelCommand {
    index: usize,
    snapshot: ChannelSnapshot,
}

impl AddChannelCommand {
    pub fn new(index: usize, snapshot: ChannelSnapshot) -> Self {
        Self { index, snapshot }
    }
}

impl Command for AddChannelCommand {
    fn execute(&self, world: &mut World) -> Option<Box<dyn Command>> {
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

        Some(Box::new(DeleteChannelCommand::new(
            self.snapshot.id,
            self.index,
        )))
    }
}

#[derive(Debug)]
pub struct DeleteChannelCommand {
    channel: Id,
    index: usize,
}

impl DeleteChannelCommand {
    pub fn new(channel: Id, index: usize) -> Self {
        Self { channel, index }
    }
}

impl Command for DeleteChannelCommand {
    fn execute(&self, world: &mut World) -> Option<Box<dyn Command>> {
        let entity = self.channel.find_entity(world)?;

        let name = world.get::<Name>(entity)?.clone();
        let state = world.get::<ChannelState>(entity)?.clone();
        let data = world.get::<ChannelData>(entity).cloned();
        let id = *world.get::<Id>(entity)?;

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

        Some(Box::new(AddChannelCommand::new(self.index, snapshot)))
    }
}

#[derive(Debug)]
pub struct MoveChannelCommand {
    from: usize,
    to: usize,
}

impl MoveChannelCommand {
    pub fn new(from: usize, to: usize) -> Self {
        Self { from, to }
    }

    pub fn apply(&self, channel_order: &mut ChannelOrder) -> Box<dyn Command> {
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

impl Command for MoveChannelCommand {
    fn execute(&self, world: &mut World) -> Option<Box<dyn Command>> {
        let mut query = world.query::<&mut ChannelOrder>();
        let mut channel_order = query.single_mut(world).ok()?;
        let undo = self.apply(&mut channel_order);
        Some(undo)
    }
}

#[derive(Debug)]
pub struct SetPluginCommand {
    channel: Id,
    data: Option<ChannelData>,
}

impl SetPluginCommand {
    pub fn new(channel: Id, data: Option<ChannelData>) -> Self {
        Self { channel, data }
    }
}

impl Command for SetPluginCommand {
    fn execute(&self, world: &mut World) -> Option<Box<dyn Command>> {
        let entity = self.channel.find_entity(world)?;
        let old_data = world.entity_mut(entity).take::<ChannelData>();
        if let Some(data) = &self.data {
            world.entity_mut(entity).insert(data.clone());
        }
        Some(Box::new(SetPluginCommand::new(self.channel, old_data)))
    }
}

#[derive(Debug)]
pub struct SetGainCommand {
    channel: Id,
    gain_value: f32,
}

impl SetGainCommand {
    pub fn new(channel: Id, gain_value: f32) -> Self {
        Self {
            channel,
            gain_value,
        }
    }
}

impl Command for SetGainCommand {
    fn execute(&self, world: &mut World) -> Option<Box<dyn Command>> {
        let entity = self.channel.find_entity(world)?;
        let mut state = world.get_mut::<ChannelState>(entity)?;
        let old_value = state.gain_value;
        state.gain_value = self.gain_value;
        Some(Box::new(SetGainCommand::new(self.channel, old_value)))
    }
}
