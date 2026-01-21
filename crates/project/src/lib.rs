use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

mod channel;
mod project;

#[derive(Component, Serialize, Deserialize)]
struct Id(Uuid);

pub use channel::{ChannelAudioView, ChannelControl, ChannelData, ChannelMessage, ChannelState};
pub use project::Project;
