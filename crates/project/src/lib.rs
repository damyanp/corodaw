use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

mod app;
mod channel;
mod found_plugin;
mod project;

pub use app::make_app;
pub use channel::{
    ChannelAudioView, ChannelControl, ChannelData, ChannelGainControl, ChannelMessage,
    ChannelState, new_channel,
};
pub use found_plugin::AvailablePlugin;
pub use project::{ChannelOrder, Project, SaveEvent};

#[derive(Component, Serialize, Deserialize)]
struct Id(Uuid);
