use clack_extensions::{
    audio_ports::{AudioPortInfoBuffer, PluginAudioPorts},
    gui::{GuiSize, HostGui, HostGuiImpl, PluginGui},
    log::{HostLog, HostLogImpl},
    params::{HostParams, HostParamsImplMainThread, HostParamsImplShared},
    timer::{HostTimer, PluginTimer},
};
use clack_host::{
    host::{self, HostHandlers, HostInfo},
    plugin::{InitializedPluginHandle, InitializingPluginHandle, PluginInstance},
    process::{PluginAudioConfiguration, PluginAudioProcessor},
};
use futures::StreamExt;
use futures_channel::{
    mpsc::{UnboundedReceiver, UnboundedSender, unbounded},
    oneshot,
};
use gpui::{App, AsyncApp, Window};
use std::{
    cell::RefCell,
    ffi::CString,
    rc::{Rc, Weak},
};

use crate::{
    audio_graph,
    plugins::{discovery::FoundPlugin, gui::Gui, timers::Timers},
};

pub mod discovery;
mod gui;
mod timers;

pub struct ClapPlugin {
    plugin: RefCell<PluginInstance<Self>>,
    gui: RefCell<Gui>,
    plugin_audio_ports: RefCell<Option<PluginAudioPorts>>,
}

impl ClapPlugin {
    pub async fn new(plugin: &mut FoundPlugin, app: &AsyncApp) -> Rc<Self> {
        let (sender, receiver) = unbounded();

        let bundle = plugin.load_bundle();
        bundle
            .get_plugin_factory()
            .expect("Only bundles with plugin factories supported");

        let id = plugin.id.clone();

        let shared = ClapPluginShared { channel: sender };
        let plugin_id = CString::new(id.as_str()).unwrap();
        let host =
            HostInfo::new("corodaw", "damyanp", "https://github.com/damyanp", "0.0.1").unwrap();

        let plugin = clack_host::plugin::PluginInstance::new(
            move |_| shared,
            move |shared| ClapPluginMainThread::new(shared),
            &bundle,
            plugin_id.as_c_str(),
            &host,
        )
        .unwrap();

        let (initialized_sender, initialized_receiver) = oneshot::channel();

        let clap_plugin = Rc::new(Self {
            plugin: RefCell::new(plugin),
            gui: RefCell::new(Gui::default()),
            plugin_audio_ports: RefCell::new(None),
        });

        let weak_plugin = Rc::downgrade(&clap_plugin);
        app.spawn(async move |app| {
            println!("[{}] spawn message receiver", id);

            let mut handler = ClapPluginMessageHandler {
                weak_plugin,
                receiver,
                app: app.clone(),
                initialized_sender: Some(initialized_sender),
            };

            handler.run().await;

            println!("[{}] end message receiver", id);
        })
        .detach();

        initialized_receiver.await.unwrap();

        clap_plugin
    }

    pub fn show_gui(self: &Rc<Self>, window: &mut Window, app: &mut App) {
        let mut gui = self.gui.borrow_mut();
        gui.show(self.clone(), window, app);
    }

    pub fn has_gui(&self) -> bool {
        self.gui.borrow().has_gui()
    }

    pub fn get_audio_graph_node_desc(&self, is_generator: bool) -> audio_graph::NodeDesc {
        let processor = Box::new(self.get_audio_processor());

        let audio_ports = self.plugin_audio_ports.borrow_mut();
        let mut plugin = self.plugin.borrow_mut();
        let mut handle = plugin.plugin_handle();

        let mut collect_ports = |is_input| {
            if let Some(audio_ports) = *audio_ports {
                let count = audio_ports.count(&mut handle, is_input);
                (0..count)
                    .map(|index| {
                        let mut buffer = AudioPortInfoBuffer::new();
                        let info = audio_ports
                            .get(&mut handle, index, is_input, &mut buffer)
                            .unwrap();

                        audio_graph::AudioPortDesc {
                            _channel_count: info.channel_count,
                            _sample_format: cpal::SampleFormat::F32,
                        }
                    })
                    .collect()
            } else {
                Vec::default()
            }
        };

        audio_graph::NodeDesc {
            _is_generator: is_generator,
            _processor: processor,
            _audio_inputs: collect_ports(true),
            _audio_outputs: collect_ports(false),
        }
    }

    fn get_audio_processor(&self) -> PluginAudioProcessor<ClapPlugin> {
        let configuration = PluginAudioConfiguration {
            sample_rate: 48_000.0,
            min_frames_count: 1,
            max_frames_count: 100_000,
        };
        PluginAudioProcessor::Started(
            self.plugin
                .borrow_mut()
                .activate(|_, _| (), configuration)
                .unwrap()
                .start_processing()
                .unwrap(),
        )
    }
}

struct ClapPluginMessageHandler {
    weak_plugin: Weak<ClapPlugin>,
    receiver: UnboundedReceiver<Message>,
    app: AsyncApp,
    initialized_sender: Option<oneshot::Sender<()>>,
}

impl ClapPluginMessageHandler {
    async fn run(&mut self) {
        while let Some(msg) = self.receiver.next().await
            && let Some(clap_plugin) = Weak::upgrade(&self.weak_plugin)
        {
            match msg {
                Message::Initialized {
                    plugin_gui,
                    plugin_audio_ports,
                } => {
                    clap_plugin.gui.borrow_mut().set_plugin_gui(plugin_gui);
                    *clap_plugin.plugin_audio_ports.borrow_mut() = plugin_audio_ports;

                    let _ = self
                        .initialized_sender
                        .take()
                        .expect("Should only be initialized once!")
                        .send(());
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
                        .request_resize(new_size, &mut self.app);
                }
            }
        }
    }
}

enum Message {
    Initialized {
        plugin_gui: Option<PluginGui>,
        plugin_audio_ports: Option<PluginAudioPorts>,
    },
    RunOnMainThread,
    ResizeHintsChanged,
    RequestResize(GuiSize),
}

impl HostHandlers for ClapPlugin {
    type Shared<'a> = ClapPluginShared;
    type MainThread<'a> = ClapPluginMainThread<'a>;
    type AudioProcessor<'a> = ();

    fn declare_extensions(
        builder: &mut clack_host::prelude::HostExtensions<Self>,
        _shared: &Self::Shared<'_>,
    ) {
        builder
            .register::<HostLog>()
            .register::<HostGui>()
            .register::<HostTimer>()
            .register::<HostParams>();
    }
}

pub struct ClapPluginShared {
    channel: UnboundedSender<Message>,
}

impl HostLogImpl for ClapPluginShared {
    fn log(&self, severity: clack_extensions::log::LogSeverity, message: &str) {
        println!("[host log] {}: {}", severity, message);
    }
}

impl HostGuiImpl for ClapPluginShared {
    fn resize_hints_changed(&self) {
        self.channel
            .unbounded_send(Message::ResizeHintsChanged)
            .expect("unbounded_send should always succeed");
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

    fn closed(&self, _was_destroyed: bool) {
        todo!()
    }
}

impl<'a> HostParamsImplMainThread for ClapPluginMainThread<'a> {
    fn rescan(&mut self, _flags: clack_extensions::params::ParamRescanFlags) {
        todo!()
    }

    fn clear(
        &mut self,
        _param_id: clack_host::prelude::ClapId,
        _flags: clack_extensions::params::ParamClearFlags,
    ) {
        todo!()
    }
}

impl HostParamsImplShared for ClapPluginShared {
    fn request_flush(&self) {
        todo!()
    }
}

unsafe impl Send for ClapPluginShared {}

impl<'a> host::SharedHandler<'a> for ClapPluginShared {
    fn initializing(&self, instance: InitializingPluginHandle<'a>) {
        println!("initializing");
        let _ = instance.get_extension::<PluginAudioPorts>();
    }

    fn request_restart(&self) {
        todo!()
    }

    fn request_process(&self) {
        todo!()
    }

    fn request_callback(&self) {
        self.channel
            .unbounded_send(Message::RunOnMainThread)
            .expect("Unbounded send should already succeed");
    }
}

pub struct ClapPluginMainThread<'a> {
    shared: &'a ClapPluginShared,
    plugin: Option<InitializedPluginHandle<'a>>,
    timer_support: Option<PluginTimer>,
    _timers: Rc<Timers>,
}

impl<'a> ClapPluginMainThread<'a> {
    fn new(shared: &'a ClapPluginShared) -> Self {
        Self {
            shared,
            plugin: None,
            timer_support: None,
            _timers: Rc::new(Timers::new()),
        }
    }
}

impl<'a> host::MainThreadHandler<'a> for ClapPluginMainThread<'a> {
    fn initialized(&mut self, instance: InitializedPluginHandle<'a>) {
        println!("Initialized!");
        self.timer_support = instance.get_extension();
        self.shared
            .channel
            .unbounded_send(Message::Initialized {
                plugin_gui: instance.get_extension(),
                plugin_audio_ports: instance.get_extension(),
            })
            .expect("unbounded_send should always succeed");
        self.plugin = Some(instance);
    }
}
