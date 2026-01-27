use bevy_ecs::prelude::*;

use crate::new_channel;

#[derive(Component, Default)]
pub struct Project;

#[derive(Component, Default)]
pub struct ChannelOrder {
    pub channel_order: Vec<Entity>,
}

impl ChannelOrder {
    pub fn add_channel(&mut self, commands: &mut Commands, index: usize) {
        let entity = commands.spawn(new_channel()).id();
        self.channel_order.insert(index, entity);
    }

    pub fn move_channel(&mut self, index: usize, destination: usize) {
        let channel = self.channel_order.remove(index);
        let destination = if destination > index {
            destination - 1
        } else {
            destination
        };
        self.channel_order.insert(destination, channel);
    }
}
