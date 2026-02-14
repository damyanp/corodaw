use super::*;
use crate::ChannelOrder;

mod commands;
mod systems;

fn setup_world() -> World {
    let mut world = World::new();
    world.spawn(ChannelOrder::default());
    world
}

fn get_channel_order(world: &mut World) -> Vec<Entity> {
    let mut query = world.query::<&ChannelOrder>();
    query.single(world).unwrap().channel_order.clone()
}

fn make_channel_data(plugin_id: &str) -> ChannelData {
    ChannelData {
        plugin_id: plugin_id.to_owned(),
        plugin_state: None,
    }
}
