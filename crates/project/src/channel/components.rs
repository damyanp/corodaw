use bevy_ecs::{name::Name, prelude::*};
use bevy_reflect::Reflect;
use engine::builtin::GainControl;
use engine::plugins::{ClapPluginId, GuiHandle, PluginFactory};
use serde::{Deserialize, Serialize};

#[derive(Component, Reflect)]
pub(super) struct InputNode(pub Entity);

#[derive(Component, Debug, Clone, Serialize, Deserialize, Reflect)]
#[require(ChannelState)]
pub struct ChannelData {
    pub plugin_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_state: Option<String>,
}

#[derive(Component, Debug, Clone, Serialize, Deserialize, Reflect)]
#[require(Id=crate::Id::new(), Name)]
pub struct ChannelState {
    pub gain_value: f32,
    pub muted: bool,
    pub soloed: bool,
    pub armed: bool,
}

impl Default for ChannelState {
    fn default() -> Self {
        Self {
            gain_value: 1.0,
            muted: false,
            soloed: false,
            armed: false,
        }
    }
}

impl ChannelState {
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
pub struct ChannelAudioView<P: Component> {
    pub(super) plugin: P,
    pub(super) plugin_node: Entity,
    pub(super) gui_handle: Option<GuiHandle>,
}

#[derive(Component, Debug, Reflect)]
#[reflect(from_reflect = false)]
#[require(ChannelState)]
pub struct ChannelGainControl(#[reflect(ignore)] pub GainControl);

impl<P: Component> ChannelAudioView<P> {
    pub fn has_gui(&self) -> bool {
        self.gui_handle
            .as_ref()
            .map(|h| h.is_visible())
            .unwrap_or(false)
    }

    pub fn plugin_id<T: PluginFactory<Plugin = P>>(&self) -> ClapPluginId {
        T::plugin_id(&self.plugin)
    }

    pub fn window_title<T: PluginFactory<Plugin = P>>(&self, channel_name: &str) -> String {
        format!("{}: {channel_name}", T::plugin_name(&self.plugin))
    }

    pub fn set_gui_handle(&mut self, gui_handle: GuiHandle) {
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
    pub state: ChannelState,
    pub data: Option<ChannelData>,
    pub id: crate::Id,
}

impl Default for ChannelSnapshot {
    fn default() -> Self {
        Self {
            name: Name::new("unnamed channel"),
            state: ChannelState::default(),
            data: None,
            id: crate::Id::new(),
        }
    }
}
