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
    Size, Window, WindowBounds, WindowOptions, div,
};
use raw_window_handle::RawWindowHandle;
use std::{
    cell::RefCell,
    ffi::{CStr, CString},
    rc::{Rc, Weak},
    time::Duration,
};

use crate::{plugins::FoundPlugin, project::timers::Timers};

mod timers;

pub struct ClapPlugin {
    plugin: Rc<RefCell<PluginInstance<Self>>>,
}

impl ClapPlugin {
    pub fn new(plugin: &mut FoundPlugin, app: &App) -> Self {
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

        let clap_plugin = Self {
            plugin: Rc::new(RefCell::new(plugin)),
        };

        let weak_plugin = Rc::downgrade(&clap_plugin.plugin);
        app.spawn(async move |app| {
            println!("[{}] spawn message receiver", id);
            ClapPlugin::handle_messages(weak_plugin, receiver, app.clone()).await;
            println!("[{}] end message receiver", id);
        })
        .detach();

        clap_plugin
    }

    async fn handle_messages(
        mut plugin: Weak<RefCell<PluginInstance<ClapPlugin>>>,
        mut receiver: UnboundedReceiver<Message>,
        mut app: AsyncApp,
    ) {
        while let Some(msg) = receiver.next().await
            && let Some(plugin) = Weak::upgrade(&plugin)
        {
            match msg {
                Message::RunOnMainThread => {
                    plugin.borrow_mut().call_on_main_thread_callback();
                }
                Message::ResizeHintsChanged => {
                    println!("Handling changed resize hints not supported");
                }
                Message::RequestResize(new_size) => {
                    let window_handle = plugin.borrow().access_handler(|m| m.window_handle);
                    if let Some(window_handle) = window_handle {
                        app.update_window(window_handle, |v, w, a| {
                            let scale = 1.0 / w.scale_factor();
                            let new_size: Size<Pixels> =
                                Size::new(new_size.width.into(), new_size.height.into());
                            let new_size =
                                Size::new(new_size.width * scale, new_size.height * scale);

                            w.resize(new_size);
                        });
                    }
                }
            }
        }
    }

    pub fn show_gui(&mut self, window: &mut Window, app: &mut App) {
        let mut plugin = self.plugin.borrow_mut();

        let Some(mut gui) = plugin.access_handler(|h| h.plugin_gui) else {
            println!("Plugin doesn't have a GUI!");
            return;
        };

        let config = GuiConfiguration {
            api_type: GuiApiType::default_for_current_platform()
                .expect("This platform supports UI"),
            is_floating: false,
        };

        let mut plugin_handle = plugin.plugin_handle();

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

        let plugin_for_view = self.plugin.clone();

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
                    is_resizable: gui.can_resize(&mut plugin_handle),
                    ..Default::default()
                },
                |window, cx| {
                    cx.new(|cx| {
                        cx.observe_window_bounds(window, ClapPluginView::on_window_bounds)
                            .detach();

                        ClapPluginView::new(plugin_for_view)
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

        plugin.access_handler_mut(|m| m.window_handle = Some(window_handle.into()));
    }

    pub fn has_gui(&self) -> bool {
        self.plugin
            .borrow()
            .access_handler(|m| m.window_handle.is_some())
    }
}

struct ClapPluginView {
    plugin: Rc<RefCell<PluginInstance<ClapPlugin>>>,
    last_size: Size<Pixels>,
}

impl ClapPluginView {
    fn new(plugin: Rc<RefCell<PluginInstance<ClapPlugin>>>) -> Self {
        Self {
            plugin,
            last_size: Size::default(),
        }
    }

    fn on_window_bounds(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let new_size = window.window_bounds().get_bounds().size;
        if new_size != self.last_size {
            self.last_size = new_size;

            let mut plugin_instance = self.plugin.borrow_mut();

            let plugin_gui = plugin_instance.access_handler(|m| m.plugin_gui.clone());
            let Some(plugin_gui) = plugin_gui else {
                return;
            };

            let mut handle = plugin_instance.plugin_handle();

            if !plugin_gui.can_resize(&mut handle) {
                return;
            }

            let new_size = new_size.scale(window.scale_factor());

            let new_size = GuiSize {
                width: new_size.width.into(),
                height: new_size.height.into(),
            };

            plugin_gui.set_size(&mut handle, new_size);
        }
    }
}

impl Render for ClapPluginView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        gpui::Empty
    }
}

#[derive(Debug)]
enum Message {
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
    plugin_gui: Option<PluginGui>,
    window_handle: Option<AnyWindowHandle>,
}

impl<'a> MainThreadHandler<'a> {
    fn new(shared: &'a SharedHandler) -> Self {
        Self {
            shared,
            plugin: None,
            plugin_gui: None,
            timer_support: None,
            timers: Rc::new(Timers::new()),
            window_handle: None,
        }
    }
}

impl<'a> host::MainThreadHandler<'a> for MainThreadHandler<'a> {
    fn initialized(&mut self, instance: InitializedPluginHandle<'a>) {
        println!("Initialized!");
        self.plugin_gui = instance.get_extension();
        self.timer_support = instance.get_extension();
        self.plugin = Some(instance);
    }
}

pub struct AudioProcessorHandler;
impl<'a> host::AudioProcessorHandler<'a> for AudioProcessorHandler {}
