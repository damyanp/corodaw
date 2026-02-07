use std::{collections::HashMap, fs::File, path::PathBuf};

use base64::{Engine, engine::general_purpose};
use bevy_app::prelude::*;
use bevy_ecs::{name::Name, prelude::*, system::RunSystemOnce};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{ChannelAudioView, ChannelData, ChannelState, Id, new_channel};

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
        app.add_observer(on_load_event);
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

#[derive(Event)]
pub struct LoadEvent {
    path: PathBuf,
}

impl LoadEvent {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[derive(Deserialize)]
struct Document {
    channels: Vec<ChannelDocument>,
    channel_order: Vec<Option<Uuid>>,
}

#[derive(Deserialize)]
struct ChannelDocument {
    name: Name,
    data: Option<ChannelData>,
    state: ChannelState,
    id: Id,
}

fn on_load_event(
    load_event: On<LoadEvent>,
    mut commands: Commands,
    project_query: Single<Entity, With<Project>>,
    channels_query: Query<Entity, With<ChannelState>>,
) {
    println!("Load: {:?}", load_event.path);

    let file = File::open(&load_event.path).unwrap();
    let document: Document = serde_json::from_reader(file).unwrap();

    for entity in &channels_query {
        commands.entity(entity).despawn();
    }

    let project_entity = project_query.into_inner();
    commands.entity(project_entity).despawn();

    let channel_entities: HashMap<_, _> = document
        .channels
        .into_iter()
        .map(|channel| {
            let state = channel.state;

            let id = channel.id.0;
            let mut entity = commands.spawn((state, channel.name, channel.id));
            if let Some(data) = channel.data {
                entity.insert(data);
            }
            (id, entity.id())
        })
        .collect();

    let ordered: Vec<_> = document
        .channel_order
        .into_iter()
        .flatten()
        .filter_map(|id| channel_entities.get(&id).copied())
        .collect();

    commands.spawn((
        Project {
            path: Some(load_event.path.clone()),
        },
        ChannelOrder {
            channel_order: ordered,
        },
    ));
}

#[allow(clippy::type_complexity)]
fn on_save_event(
    save_event: On<SaveEvent>,
    project_query: Single<(&mut Project, &ChannelOrder)>,
    channels_query: Query<(
        &Name,
        Option<&ChannelData>,
        &ChannelState,
        &Id,
        Option<&ChannelAudioView>,
    )>,
    clap_plugin_manager: NonSend<engine::plugins::ClapPluginManager>,
) {
    println!("Save: {:?}", save_event.path);

    let (mut project, channel_order) = project_query.into_inner();

    let channels: Vec<_> = channels_query
        .iter()
        .map(|(name, data, state, id, view)| {
            let data = match (data, view) {
                (Some(data), Some(view)) => {
                    let plugin_state = futures::executor::block_on(async {
                        clap_plugin_manager
                            .save_plugin_state(view.plugin_id())
                            .await
                            .ok()
                            .flatten()
                    });
                    let mut data = data.clone();
                    data.plugin_state =
                        plugin_state.map(|bytes| general_purpose::STANDARD.encode(&bytes));
                    Some(data)
                }
                (data, _) => data.cloned(),
            };
            json!({"name": name, "data": data, "state": state, "id": id})
        })
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
