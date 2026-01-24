use bevy_ecs::prelude::*;
use engine::plugins::discovery::{FoundPlugin, get_plugins};

#[derive(Component)]
pub struct AvailablePlugin(pub FoundPlugin);

pub fn add_available_plugins(world: &mut World) {
    let plugins = get_plugins();

    for plugin in plugins {
        world.spawn(AvailablePlugin(plugin));
    }
}
