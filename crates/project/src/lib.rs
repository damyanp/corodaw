use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

mod channel;
mod commands;
mod found_plugin;
mod project;

pub use channel::*;
pub use commands::*;
pub use found_plugin::{AvailablePlugin, add_available_plugins};
pub use project::{ChannelOrder, LoadEvent, Project, ProjectPlugin, SaveEvent};

#[derive(Component, Serialize, Deserialize, Hash, Eq, PartialEq, Clone, Copy, Debug, Reflect)]
#[reflect(opaque)]
pub struct Id(Uuid);

impl Default for Id {
    fn default() -> Self {
        Self::new()
    }
}

impl Id {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn find_entity(&self, world: &mut World) -> Option<Entity> {
        let mut query = world.query::<(Entity, &Id)>();
        query
            .iter(world)
            .find(|(_, id)| **id == *self)
            .map(|(entity, _)| entity)
    }
}
