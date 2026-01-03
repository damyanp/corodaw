use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
};

use clack_extensions::gui::{GuiApiType, GuiConfiguration, GuiSize, PluginGui};
use engine::plugins::{ClapPlugin, ClapPluginId, ClapPluginManager, GuiMessage, GuiMessagePayload};
use futures::StreamExt;
use futures_channel::mpsc::{UnboundedReceiver, unbounded};
use smol::LocalExecutor;
use winit::{
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    window::{Window, WindowId},
};

pub struct EguiClapPluginManager {
    pub inner: Rc<ClapPluginManager>,
    guis: RefCell<HashMap<ClapPluginId, Rc<EguiPluginGui>>>,
    windows: RefCell<HashMap<WindowId, ClapPluginId>>,
}

impl EguiClapPluginManager {
    pub fn new(executor: &LocalExecutor) -> Rc<Self> {
        let (gui_sender, gui_receiver) = unbounded();

        let inner = ClapPluginManager::new(gui_sender);
        Self::spawn_message_handler(executor, Rc::downgrade(&inner));

        let manager = Rc::new(Self {
            inner,
            guis: RefCell::default(),
            windows: RefCell::default(),
        });
        Self::spawn_gui_message_handler(executor, Rc::downgrade(&manager), gui_receiver);

        manager
    }

    fn spawn_message_handler(executor: &LocalExecutor, manager: Weak<ClapPluginManager>) {
        executor
            .spawn(async move {
                ClapPluginManager::message_handler(manager).await;
            })
            .detach();
    }

    fn spawn_gui_message_handler(
        executor: &LocalExecutor,
        manager: Weak<Self>,
        mut receiver: UnboundedReceiver<GuiMessage>,
    ) {
        executor
            .spawn(async move {
                println!("[gui_message_handler] start");
                while let Some(GuiMessage { plugin_id, payload }) = receiver.next().await {
                    let plugin = {
                        let Some(manager) = manager.upgrade() else {
                            break;
                        };
                        manager.guis.borrow().get(&plugin_id).unwrap().clone()
                    };

                    match payload {
                        GuiMessagePayload::ResizeHintsChanged => {
                            println!("Handling changed resize hints not supported");
                        }
                        GuiMessagePayload::RequestResize(size) => {
                            plugin.request_resize(size);
                        }
                    }
                }
                println!("[gui_message_handler] end");
            })
            .detach();
    }

    pub fn show_plugin_gui(&self, event_loop: &ActiveEventLoop, clap_plugin: Rc<ClapPlugin>) {
        let mut guis = self.guis.borrow_mut();

        let plugin_id = clap_plugin.get_id();

        if guis.contains_key(&plugin_id) {
            println!("Asked to show a plugin that is already shown!");
            return;
        }

        let mut plugin = clap_plugin.plugin.borrow_mut();
        let mut plugin_handle = plugin.plugin_handle();

        let Some(plugin_gui) = plugin_handle.get_extension::<PluginGui>() else {
            println!("No GUI for plugin!");
            return;
        };

        let config = GuiConfiguration {
            api_type: GuiApiType::default_for_current_platform()
                .expect("This platform supports UI"),
            is_floating: false,
        };

        if !plugin_gui.is_api_supported(&mut plugin_handle, config) {
            println!("Plugin doesn't support API");
            return;
        }

        plugin_gui
            .create(&mut plugin_handle, config)
            .expect("create succeeds");

        let initial_size = plugin_gui.get_size(&mut plugin_handle).unwrap_or(GuiSize {
            width: 800,
            height: 600,
        });

        let is_resizeable = plugin_gui
            .get_resize_hints(&mut plugin_handle)
            .map(|h| h.can_resize_horizontally && h.can_resize_vertically)
            .unwrap_or(false);

        let window = event_loop
            .create_window(
                Window::default_attributes()
                    .with_inner_size(PhysicalSize {
                        width: initial_size.width,
                        height: initial_size.height,
                    })
                    .with_resizable(is_resizeable),
            )
            .expect("Window creation to succeed");

        unsafe {
            let window = clack_extensions::gui::Window::from_window(&window).unwrap();
            plugin_gui
                .set_parent(&mut plugin_handle, window)
                .expect("set_parent succeeds");
        }

        let _ = plugin_gui.show(&mut plugin_handle);

        drop(plugin);

        let window_id = window.id();
        let gui = Rc::new(EguiPluginGui {
            clap_plugin,
            plugin_gui,
            window,
        });

        guis.insert(plugin_id, gui);
        self.windows.borrow_mut().insert(window_id, plugin_id);
    }

    pub fn window_event(&self, window_id: WindowId, event: &WindowEvent) -> bool {
        let mut windows = self.windows.borrow_mut();

        if let Some(id) = windows.get(&window_id) {
            match event {
                WindowEvent::CloseRequested => {
                    self.guis.borrow_mut().remove(id);
                    windows.remove(&window_id);
                }
                _ => (),
            }
            return true;
        }
        false
    }
}

struct EguiPluginGui {
    clap_plugin: Rc<ClapPlugin>,
    plugin_gui: PluginGui,
    window: Window,
}

impl Drop for EguiPluginGui {
    fn drop(&mut self) {
        self.plugin_gui
            .destroy(&mut self.clap_plugin.plugin.borrow_mut().plugin_handle());
    }
}

impl EguiPluginGui {
    fn request_resize(self: &Rc<EguiPluginGui>, size: GuiSize) {
        let _ = self.window.request_inner_size(PhysicalSize {
            width: size.width,
            height: size.height,
        });
    }
}
