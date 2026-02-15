use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

mod app;
mod channel;
mod commands;
mod found_plugin;
mod project;

pub use app::build_app;
pub use channel::*;
pub use commands::*;
pub use found_plugin::AvailablePlugin;
pub use project::{ChannelOrder, LoadEvent, ProjectInfo, SaveEvent};

#[derive(Component, Serialize, Deserialize, Hash, Eq, PartialEq, Clone, Copy, Debug, Reflect)]
#[reflect(opaque)]
pub struct StableId(Uuid);

impl Default for StableId {
    fn default() -> Self {
        Self::new()
    }
}

impl StableId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn find_entity(&self, world: &mut World) -> Option<Entity> {
        let mut query = world.query::<(Entity, &StableId)>();
        query
            .iter(world)
            .find(|(_, id)| **id == *self)
            .map(|(entity, _)| entity)
    }
}
