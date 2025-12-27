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

use crate::{plugins::FoundPlugin, project::timers::Timers};

mod timers;

pub struct ClapPlugin {
    plugin: RefCell<PluginInstance<Self>>,
    gui: RefCell<Gui>,
}

#[derive(Default)]
struct Gui {
    plugin_gui: Option<PluginGui>,
    window_handle: Option<AnyWindowHandle>,
    window_closed_subscription: Option<Subscription>,
}

impl Gui {
    fn request_resize(&mut self, new_size: GuiSize, app: &mut AsyncApp) {
        if let Some(window_handle) = self.window_handle {
            app.update_window(window_handle, |_, window, _| {
                window.resize(new_size.to_size(window));
            });
        }
    }
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

        let Some(mut plugin_gui) = gui.plugin_gui else {
            println!("Plugin doesn't have a GUI!");
            return;
        };

        let config = GuiConfiguration {
            api_type: GuiApiType::default_for_current_platform()
                .expect("This platform supports UI"),
            is_floating: false,
        };

        let mut plugin = self.plugin.borrow_mut();
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

        let clap_plugin_for_view = self.clone();

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

        gui.window_handle = Some(window_handle);

        let plugin_rc = self.clone();
        let subscription = app.on_window_closed(move |cx| {
            // gpui doesn't seem to have a way to get a notification when a
            // specific window is closed, so instead we have to look at the
            // windows that haven't been closed to determine figure out if it is
            // still there or not!
            if !cx.windows().into_iter().any(|w| w == window_handle) {
                let mut gui = plugin_rc.gui.borrow_mut();

                gui.window_handle = None;
                gui.window_closed_subscription = None;

                if let Some(plugin_gui) = gui.plugin_gui.as_ref() {
                    plugin_gui.destroy(&mut plugin_rc.plugin.borrow_mut().plugin_handle());
                }
            }
        });

        gui.window_closed_subscription = Some(subscription);
    }

    pub fn has_gui(&self) -> bool {
        self.gui.borrow().window_handle.is_some()
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
