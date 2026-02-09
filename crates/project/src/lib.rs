use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

mod app;
mod channel;
mod commands;
mod found_plugin;
mod project;

pub use app::make_app;
pub use channel::*;
pub use commands::*;
pub use found_plugin::AvailablePlugin;
pub use project::{ChannelOrder, LoadEvent, Project, SaveEvent};

#[derive(Component, Serialize, Deserialize)]
pub struct Id(pub Uuid);
