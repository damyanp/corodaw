use bevy_ecs::{name::Name, prelude::*};
use bevy_reflect::Reflect;
use serde::{Deserialize, Serialize};

use engine::builtin::GainNodeOwner;
use engine::plugins::{ClapId, ClapProxy, PluginGuiHandle, PluginManager};

use crate::StableId;

#[derive(Component, Reflect)]
pub(crate) struct ChannelSourceNode(pub Entity);

#[derive(Component, Debug, Clone, Serialize, Deserialize, Reflect)]
#[require(ChannelMixerState)]
pub struct ChannelPluginBinding {
    pub plugin_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_state: Option<String>,
}

#[derive(Component, Debug, Clone, Serialize, Deserialize, Reflect)]
#[require(StableId=StableId::new(), Name)]
pub struct ChannelMixerState {
    pub gain_value: f32,
    pub muted: bool,
    pub soloed: bool,
    pub armed: bool,
}

impl Default for ChannelMixerState {
    fn default() -> Self {
        Self {
            gain_value: 1.0,
            muted: false,
            soloed: false,
            armed: false,
        }
    }
}

impl ChannelMixerState {
    pub fn get_button(&self, button: ChannelButton) -> bool {
        match button {
            ChannelButton::Mute => self.muted,
            ChannelButton::Solo => self.soloed,
            ChannelButton::Arm => self.armed,
        }
    }

    pub fn set_button(&mut self, button: ChannelButton, value: bool) {
        match button {
            ChannelButton::Mute => self.muted = value,
            ChannelButton::Solo => self.soloed = value,
            ChannelButton::Arm => self.armed = value,
        }
    }
}

#[derive(Component)]
pub struct ChannelPluginInstance<P: Component = ClapProxy> {
    pub(crate) plugin: P,
    pub(crate) plugin_node: Entity,
    pub(crate) gui_handle: Option<PluginGuiHandle>,
}

#[derive(Component, Debug, Reflect)]
#[reflect(from_reflect = false)]
#[require(ChannelMixerState)]
pub struct ChannelGain(#[reflect(ignore)] pub GainNodeOwner);

impl<P: Component> ChannelPluginInstance<P> {
    pub fn has_gui(&self) -> bool {
        self.gui_handle
            .as_ref()
            .map(|h| h.is_visible())
            .unwrap_or(false)
    }

    pub fn plugin_id<T: PluginManager<Plugin = P>>(&self) -> ClapId {
        T::plugin_id(&self.plugin)
    }

    pub fn window_title<T: PluginManager<Plugin = P>>(&self, channel_name: &str) -> String {
        format!("{}: {channel_name}", T::plugin_name(&self.plugin))
    }

    pub fn set_gui_handle(&mut self, gui_handle: PluginGuiHandle) {
        self.gui_handle = Some(gui_handle);
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ChannelButton {
    Mute,
    Solo,
    Arm,
}

#[derive(Debug, Clone)]
pub struct ChannelSnapshot {
    pub name: Name,
    pub state: ChannelMixerState,
    pub data: Option<ChannelPluginBinding>,
    pub id: StableId,
}

impl Default for ChannelSnapshot {
    fn default() -> Self {
        Self {
            name: Name::new("unnamed channel"),
            state: ChannelMixerState::default(),
            data: None,
            id: StableId::new(),
        }
    }
}
