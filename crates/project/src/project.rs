use std::{fs::File, path::PathBuf};

use bevy_app::prelude::*;
use bevy_ecs::{prelude::*, system::RunSystemOnce};
use serde::Serialize;
use serde_json::json;

use crate::{ChannelData, ChannelState, Id, new_channel};

#[derive(Component, Default)]
pub struct Project {
    pub path: Option<PathBuf>,
}

#[derive(Component, Default, Serialize)]
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

    pub fn delete_channel(&self, commands: &mut Commands, index: usize) {
        let channel = self.channel_order[index];
        if let Ok(mut entity) = commands.get_spawned_entity(channel) {
            entity.despawn();
        }
    }
}

pub struct ProjectPlugin;
impl Plugin for ProjectPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut()
            .spawn((Project::default(), ChannelOrder::default()));

        app.world_mut()
            .run_system_once(
                |mut commands: Commands, mut channel_order: Single<&mut ChannelOrder>| {
                    channel_order.as_mut().add_channel(&mut commands, 0);
                },
            )
            .unwrap();

        app.add_observer(on_save_event);
    }
}

#[derive(Event)]
pub struct SaveEvent {
    path: PathBuf,
}

impl SaveEvent {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

fn on_save_event(
    save_event: On<SaveEvent>,
    project_query: Single<(&mut Project, &ChannelOrder)>,
    channels_query: Query<(&Name, Option<&ChannelData>, &ChannelState, &Id)>,
) {
    println!("Save: {:?}", save_event.path);

    let (mut project, channel_order) = project_query.into_inner();

    let channels: Vec<_> = channels_query
        .iter()
        .map(
            |(name, data, state, id)| json!({"name": name, "data": data, "state": state, "id": id}),
        )
        .collect();

    let channel_order: Vec<_> = channel_order
        .channel_order
        .iter()
        .map(|e| channels_query.get(*e).ok().map(|r| r.3))
        .collect();

    let document = &json!(
        {"channels": channels, "channel_order": channel_order}
    );

    let file = File::create(&save_event.path).unwrap();
    serde_json::to_writer_pretty(file, document).unwrap();

    project.path = Some(save_event.path.clone());
}
