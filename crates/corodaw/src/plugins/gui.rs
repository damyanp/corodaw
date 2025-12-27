#![allow(unused)]
use clack_extensions::{
    audio_ports::PluginAudioPorts,
    gui::{GuiApiType, GuiConfiguration, GuiSize, HostGui, HostGuiImpl, PluginGui},
    log::{HostLog, HostLogImpl},
    params::{HostParams, HostParamsImplMainThread, HostParamsImplShared},
    timer::{HostTimer, HostTimerImpl, PluginTimer},
};
use clack_host::{
    host::{self, HostError, HostHandlers, HostInfo},
    plugin::{
        InitializedPluginHandle, InitializingPluginHandle, PluginInstance, PluginMainThreadHandle,
    },
};
use futures::{SinkExt, StreamExt};
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use gpui::{
    AnyWindowHandle, App, AppContext, AsyncApp, Context, IntoElement, Pixels, Render, SharedString,
    Size, Subscription, Window, WindowBounds, WindowOptions, div,
};
use raw_window_handle::RawWindowHandle;
use std::{
    cell::RefCell,
    ffi::{CStr, CString},
    rc::{Rc, Weak},
    time::Duration,
};

use super::ClapPlugin;
use crate::{plugins::discovery::FoundPlugin, plugins::timers::Timers};

#[derive(Default)]
pub struct Gui {
    pub plugin_gui: Option<PluginGui>,
    pub window_handle: Option<AnyWindowHandle>,
    window_closed_subscription: Option<Subscription>,
}

impl Gui {
    pub fn show(&mut self, clap_plugin: Rc<ClapPlugin>, window: &mut Window, app: &mut App) {
        let Some(mut plugin_gui) = self.plugin_gui else {
            println!("Plugin doesn't have a GUI!");
            return;
        };

        let config = GuiConfiguration {
            api_type: GuiApiType::default_for_current_platform()
                .expect("This platform supports UI"),
            is_floating: false,
        };

        let mut plugin = clap_plugin.plugin.borrow_mut();
        let mut plugin_handle = plugin.plugin_handle();

        if !plugin_gui.is_api_supported(&mut plugin_handle, config) {
            println!("Plugin doesn't support API");
            return;
        }

        plugin_gui
            .create(&mut plugin_handle, config)
            .expect("create succeeds");

        let initial_size = plugin_gui
            .get_size(&mut plugin_handle)
            .unwrap_or(GuiSize {
                width: 800,
                height: 600,
            })
            .to_size(window);

        let bounds = WindowBounds::centered(initial_size, app);

        let clap_plugin_for_view = clap_plugin.clone();
        let window_handle = app
            .open_window(
                WindowOptions {
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some(SharedString::from("Plugin Window")),
                        ..Default::default()
                    }),
                    window_bounds: Some(bounds),
                    is_resizable: plugin_gui.can_resize(&mut plugin_handle),
                    ..Default::default()
                },
                |window, cx| {
                    cx.new(|cx| {
                        cx.observe_window_bounds(window, ClapPluginView::on_window_bounds)
                            .detach();

                        ClapPluginView::new(clap_plugin_for_view)
                    })
                },
            )
            .expect("open_window succeeded");
        let window_handle = window_handle.into();

        let window = app
            .update_window(window_handle, |_, window, _| {
                clack_extensions::gui::Window::from_window(window).unwrap()
            })
            .unwrap();

        unsafe {
            plugin_gui
                .set_parent(&mut plugin_handle, window)
                .expect("set_parent succeeds");
        }

        if let Err(err) = plugin_gui.show(&mut plugin_handle) {
            println!("Error: {:?}", err);
        }

        self.window_handle = Some(window_handle);

        let clap_plugin = clap_plugin.clone();
        let subscription = app.on_window_closed(move |cx| {
            // gpui doesn't seem to have a way to get a notification when a
            // specific window is closed, so instead we have to look at the
            // windows that haven't been closed to determine figure out if it is
            // still there or not!
            if !cx.windows().into_iter().any(|w| w == window_handle) {
                let mut gui = clap_plugin.gui.borrow_mut();

                gui.window_handle = None;
                gui.window_closed_subscription = None;

                if let Some(plugin_gui) = gui.plugin_gui.as_ref() {
                    plugin_gui.destroy(&mut clap_plugin.plugin.borrow_mut().plugin_handle());
                }
            }
        });

        self.window_closed_subscription = Some(subscription);
    }

    pub fn request_resize(&mut self, new_size: GuiSize, app: &mut AsyncApp) {
        if let Some(window_handle) = self.window_handle {
            app.update_window(window_handle, |_, window, _| {
                window.resize(new_size.to_size(window));
            });
        }
    }
}

struct ClapPluginView {
    clap_plugin: Rc<ClapPlugin>,
    last_size: Size<Pixels>,
}

impl ClapPluginView {
    fn new(clap_plugin: Rc<ClapPlugin>) -> Self {
        Self {
            clap_plugin,
            last_size: Size::default(),
        }
    }

    fn on_window_bounds(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let new_size = window.window_bounds().get_bounds().size;
        if new_size != self.last_size {
            self.last_size = new_size;

            let mut plugin_instance = self.clap_plugin.plugin.borrow_mut();
            let Some(plugin_gui) = self.clap_plugin.gui.borrow().plugin_gui else {
                return;
            };

            let mut handle = plugin_instance.plugin_handle();

            if !plugin_gui.can_resize(&mut handle) {
                return;
            }

            plugin_gui.set_size(&mut handle, new_size.to_gui_size(window));
        }
    }
}

impl Render for ClapPluginView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        gpui::Empty
    }
}

trait ToSize {
    fn to_size(&self, window: &Window) -> Size<Pixels>;
}

impl ToSize for GuiSize {
    fn to_size(&self, window: &Window) -> Size<Pixels> {
        let scale = 1.0 / window.scale_factor();
        let s = Size::<Pixels>::new(self.width.into(), self.height.into());
        s.map(|d| d * scale)
    }
}

trait ToGuiSize {
    fn to_gui_size(&self, window: &Window) -> GuiSize;
}

impl ToGuiSize for Size<Pixels> {
    fn to_gui_size(&self, window: &Window) -> GuiSize {
        let s = self.scale(window.scale_factor());
        GuiSize {
            width: s.width.into(),
            height: s.height.into(),
        }
    }
}
