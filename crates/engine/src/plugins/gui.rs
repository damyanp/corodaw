use clack_extensions::gui::{GuiApiType, GuiConfiguration, GuiSize, PluginGui};
use gpui::{
    AnyWindowHandle, App, AppContext, AsyncApp, Context, IntoElement, Pixels, Render, SharedString,
    Size, Subscription, Window, WindowBounds, WindowOptions,
};
use std::rc::Rc;

use super::ClapPlugin;

#[derive(Default)]
pub struct Gui {
    plugin_gui: Option<PluginGui>,
    window_handle: Option<AnyWindowHandle>,
    window_closed_subscription: Option<Subscription>,
}

impl Gui {
    pub fn set_plugin_gui(&mut self, plugin_gui: Option<PluginGui>) {
        assert!(self.plugin_gui.is_none());
        self.plugin_gui = plugin_gui;
    }

    pub fn has_gui(&self) -> bool {
        self.window_handle.is_some()
    }

    pub fn show(&mut self, clap_plugin: Rc<ClapPlugin>, window: &mut Window, app: &mut App) {}

    pub fn request_resize(&mut self, new_size: GuiSize, app: &mut AsyncApp) {
        if let Some(window_handle) = self.window_handle {
            app.update_window(window_handle, |_, window, _| {
                window.resize(new_size.to_size(window));
            })
            .expect("update_window should succeed");
        }
    }
}

