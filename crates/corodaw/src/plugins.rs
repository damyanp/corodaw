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

use crate::plugins::{discovery::FoundPlugin, gui::Gui, timers::Timers};

pub mod discovery;
mod gui;
mod timers;

pub struct ClapPlugin {
    plugin: RefCell<PluginInstance<Self>>,
    gui: RefCell<Gui>,
}

impl ClapPlugin {
    pub fn new(plugin: &mut FoundPlugin, app: &App) -> Rc<Self> {
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

        let clap_plugin = Rc::new(Self {
            plugin: RefCell::new(plugin),
            gui: RefCell::new(Gui::default()),
        });

        let weak_plugin = Rc::downgrade(&clap_plugin);
        app.spawn(async move |app| {
            println!("[{}] spawn message receiver", id);
            ClapPlugin::handle_messages(weak_plugin, receiver, app.clone()).await;
            println!("[{}] end message receiver", id);
        })
        .detach();

        clap_plugin
    }

    async fn handle_messages(
        clap_plugin: Weak<ClapPlugin>,
        mut receiver: UnboundedReceiver<Message>,
        mut app: AsyncApp,
    ) {
        while let Some(msg) = receiver.next().await
            && let Some(clap_plugin) = Weak::upgrade(&clap_plugin)
        {
            match msg {
                Message::Initialized { plugin_gui } => {
                    clap_plugin.gui.borrow_mut().plugin_gui = plugin_gui;
                }
                Message::RunOnMainThread => {
                    clap_plugin
                        .plugin
                        .borrow_mut()
                        .call_on_main_thread_callback();
                }
                Message::ResizeHintsChanged => {
                    println!("Handling changed resize hints not supported");
                }
                Message::RequestResize(new_size) => {
                    clap_plugin
                        .gui
                        .borrow_mut()
                        .request_resize(new_size, &mut app);
                }
            }
        }
    }

    pub fn show_gui(self: &Rc<Self>, window: &mut Window, app: &mut App) {
        let mut gui = self.gui.borrow_mut();
        gui.show(self.clone(), window, app);
    }

    pub fn has_gui(&self) -> bool {
        self.gui.borrow().window_handle.is_some()
    }
}

enum Message {
    Initialized { plugin_gui: Option<PluginGui> },
    RunOnMainThread,
    ResizeHintsChanged,
    RequestResize(GuiSize),
}

impl HostHandlers for ClapPlugin {
    type Shared<'a> = SharedHandler;
    type MainThread<'a> = MainThreadHandler<'a>;
    type AudioProcessor<'a> = AudioProcessorHandler;

    fn declare_extensions(
        builder: &mut clack_host::prelude::HostExtensions<Self>,
        shared: &Self::Shared<'_>,
    ) {
        builder
            .register::<HostLog>()
            .register::<HostGui>()
            .register::<HostTimer>()
            .register::<HostParams>();
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
        Ok(self
            .channel
            .unbounded_send(Message::RequestResize(new_size))?)
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

impl<'a> HostParamsImplMainThread for MainThreadHandler<'a> {
    fn rescan(&mut self, flags: clack_extensions::params::ParamRescanFlags) {
        todo!()
    }

    fn clear(
        &mut self,
        param_id: clack_host::prelude::ClapId,
        flags: clack_extensions::params::ParamClearFlags,
    ) {
        todo!()
    }
}

impl HostParamsImplShared for SharedHandler {
    fn request_flush(&self) {
        todo!()
    }
}

unsafe impl Send for SharedHandler {}

impl<'a> host::SharedHandler<'a> for SharedHandler {
    fn initializing(&self, instance: InitializingPluginHandle<'a>) {
        let _ = instance.get_extension::<PluginAudioPorts>();
    }

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
    timer_support: Option<PluginTimer>,
    timers: Rc<Timers>,
}

impl<'a> MainThreadHandler<'a> {
    fn new(shared: &'a SharedHandler) -> Self {
        Self {
            shared,
            plugin: None,
            timer_support: None,
            timers: Rc::new(Timers::new()),
        }
    }
}

impl<'a> host::MainThreadHandler<'a> for MainThreadHandler<'a> {
    fn initialized(&mut self, instance: InitializedPluginHandle<'a>) {
        println!("Initialized!");
        self.timer_support = instance.get_extension();
        self.shared.channel.unbounded_send(Message::Initialized {
            plugin_gui: instance.get_extension(),
        });
        self.plugin = Some(instance);
    }
}

pub struct AudioProcessorHandler;
impl<'a> host::AudioProcessorHandler<'a> for AudioProcessorHandler {}
