use bevy_ecs::prelude::*;

use crate::new_channel;

#[derive(Component, Default)]
pub struct Project;

#[derive(Component, Default)]
pub struct ChannelOrder {
    pub channel_order: Vec<Entity>,
}

impl ChannelOrder {
    pub fn add_channel(&mut self, commands: &mut Commands) {
        let entity = commands.spawn(new_channel()).id();
        self.channel_order.push(entity);
    }
}
