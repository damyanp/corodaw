use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use engine::plugins::discovery::{FoundPlugin, get_plugins};

#[derive(Component, Reflect)]
#[reflect(from_reflect = false)]
pub struct AvailablePlugin(#[reflect(ignore)] pub FoundPlugin);

pub fn add_available_plugins(world: &mut World) {
    let plugins = get_plugins();

    for plugin in plugins {
        world.spawn(AvailablePlugin(plugin));
    }
}
