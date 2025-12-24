#![allow(unused)]
use clack_extensions::{
    gui::{GuiApiType, GuiConfiguration, GuiSize, HostGui, HostGuiImpl, PluginGui},
    log::{HostLog, HostLogImpl},
};
use clack_host::{
    host::{self, HostHandlers, HostInfo},
    plugin::{InitializedPluginHandle, PluginMainThreadHandle},
};
use futures::{SinkExt, StreamExt};
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use gpui::{
    AnyWindowHandle, App, AppContext, AsyncApp, Render, SharedString, Size, Window, WindowBounds,
    WindowOptions, div,
};
use raw_window_handle::RawWindowHandle;
use std::{
    cell::RefCell,
    ffi::{CStr, CString},
    rc::{Rc, Weak},
    time::Duration,
};

use crate::plugins::FoundPlugin;

struct Project {
    channels: Vec<Channel>,
}

struct Channel {
    generator: PluginInstance,
}

pub struct PluginInstance {
    plugin: clack_host::plugin::PluginInstance<Self>,
    window_handle: Option<AnyWindowHandle>,
}

impl PluginInstance {
    pub fn new(plugin: &mut FoundPlugin, app: &App) -> Rc<RefCell<Self>> {
        let (sender, mut receiver) = unbounded();

        let bundle = plugin.load_bundle();
        bundle
            .get_plugin_factory()
            .expect("Only bundles with plugin factories supported");

        let id = plugin.id.clone();

        let shared = SharedHandler { channel: sender };
        let plugin_id = CString::new(id.as_str()).unwrap();
        let host =
            HostInfo::new("corodaw", "damyanp", "https://github.com/damyanp", "0.0.1").unwrap();

        let plugin = clack_host::plugin::PluginInstance::new(
            move |_| shared,
            move |shared| MainThreadHandler::new(shared),
            &bundle,
            plugin_id.as_c_str(),
            &host,
        )
        .unwrap();

        let p = Rc::new(RefCell::new(Self {
            plugin,
            window_handle: None,
        }));

        let weak_p = Rc::downgrade(&p);

        app.spawn(async move |app| {
            println!("[{}] spawn message receiver", id);
            PluginInstance::handle_messages(weak_p, receiver, app.clone()).await;
            println!("[{}] end message receiver", id);
        })
        .detach();

        p
    }

    async fn handle_messages(
        mut this: Weak<RefCell<PluginInstance>>,
        mut receiver: UnboundedReceiver<Message>,
        app: AsyncApp,
    ) {
        while let Some(msg) = receiver.next().await
            && let Some(p) = Weak::upgrade(&this)
        {
            println!("Message: {:?}", msg);

            match msg {
                Message::RunOnMainThread => {
                    p.borrow_mut().plugin.call_on_main_thread_callback();
                }
                Message::ResizeHintsChanged => {
                    println!("Handling changed resize hints not supported");
                }
            }
        }
    }

    pub fn show_gui(&mut self, window: &mut Window, app: &mut App) {
        let Some(mut gui) = self.plugin.access_handler(|h| h.gui) else {
            println!("Plugin doesn't have a GUI!");
            return;
        };

        let config = GuiConfiguration {
            api_type: GuiApiType::default_for_current_platform()
                .expect("This platform supports UI"),
            is_floating: false,
        };

        let mut plugin_handle = self.plugin.plugin_handle();

        if !gui.is_api_supported(&mut plugin_handle, config) {
            println!("Plugin doesn't support API");
            return;
        }

        gui.create(&mut plugin_handle, config)
            .expect("create succeeds");

        let initial_size = gui.get_size(&mut plugin_handle).unwrap_or(GuiSize {
            width: 800,
            height: 600,
        });
        let bounds = WindowBounds::centered(
            Size::new(initial_size.width.into(), initial_size.height.into()),
            app,
        );

        let window_handle = app
            .open_window(
                WindowOptions {
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some(SharedString::from("Plugin Window")),
                        ..Default::default()
                    }),
                    window_bounds: Some(bounds),

                    // Cardinal always reports that it isn't resizable, and then
                    // changes its resize hints. When we figure out how to
                    // handle that we can update this.
                    is_resizable: true, // gui.can_resize(&mut plugin_handle),
                    ..Default::default()
                },
                |window, cx| {
                    cx.new(|cx| {
                        cx.observe_window_bounds(window, |_, window, _| {
                            println!("Window bounds changed!");
                        })
                        .detach();
                        gpui::Empty
                    })
                },
            )
            .expect("open_window succeeded");

        let window = app
            .update_window(window_handle.into(), |_, window, _| {
                clack_extensions::gui::Window::from_window(window).unwrap()
            })
            .unwrap();

        unsafe {
            gui.set_parent(&mut plugin_handle, window)
                .expect("set_parent succeeds");
        }

        if let Err(err) = gui.show(&mut plugin_handle) {
            println!("Error: {:?}", err);
        }

        self.window_handle = Some(window_handle.into());
    }

    pub fn has_gui(&self) -> bool {
        self.window_handle.is_some()
    }
}

#[derive(Debug)]
enum Message {
    RunOnMainThread,
    ResizeHintsChanged,
}

impl HostHandlers for PluginInstance {
    type Shared<'a> = SharedHandler;
    type MainThread<'a> = MainThreadHandler<'a>;
    type AudioProcessor<'a> = AudioProcessorHandler;

    fn declare_extensions(
        builder: &mut clack_host::prelude::HostExtensions<Self>,
        shared: &Self::Shared<'_>,
    ) {
        builder.register::<HostLog>().register::<HostGui>();
    }
}

pub struct SharedHandler {
    channel: UnboundedSender<Message>,
}

impl HostLogImpl for SharedHandler {
    fn log(&self, severity: clack_extensions::log::LogSeverity, message: &str) {
        println!("[host log] {}: {}", severity, message);
    }
}

impl HostGuiImpl for SharedHandler {
    fn resize_hints_changed(&self) {
        self.channel.unbounded_send(Message::ResizeHintsChanged);
    }

    fn request_resize(&self, new_size: GuiSize) -> Result<(), clack_host::prelude::HostError> {
        todo!()
    }

    fn request_show(&self) -> Result<(), clack_host::prelude::HostError> {
        todo!()
    }

    fn request_hide(&self) -> Result<(), clack_host::prelude::HostError> {
        todo!()
    }

    fn closed(&self, was_destroyed: bool) {
        todo!()
    }
}

unsafe impl Send for SharedHandler {}

impl<'a> host::SharedHandler<'a> for SharedHandler {
    fn request_restart(&self) {
        todo!()
    }

    fn request_process(&self) {
        todo!()
    }

    fn request_callback(&self) {
        self.channel.unbounded_send(Message::RunOnMainThread);
    }
}

pub struct MainThreadHandler<'a> {
    shared: &'a SharedHandler,
    plugin: Option<InitializedPluginHandle<'a>>,
    gui: Option<PluginGui>,
}

impl<'a> MainThreadHandler<'a> {
    fn new(shared: &'a SharedHandler) -> Self {
        Self {
            shared,
            plugin: None,
            gui: None,
        }
    }
}

impl<'a> host::MainThreadHandler<'a> for MainThreadHandler<'a> {
    fn initialized(&mut self, instance: InitializedPluginHandle<'a>) {
        println!("Initialized!");
        self.gui = instance.get_extension();
        self.plugin = Some(instance);
    }
}

pub struct AudioProcessorHandler;
impl<'a> host::AudioProcessorHandler<'a> for AudioProcessorHandler {}
