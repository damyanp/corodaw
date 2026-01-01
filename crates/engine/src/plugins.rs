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
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ffi::CString,
    rc::{Rc, Weak},
};

use crate::plugins::{discovery::FoundPlugin, timers::Timers};

pub mod discovery;
mod timers;

pub trait Gui: Default + 'static {
    type Context;

    fn set_plugin_gui(&mut self, plugin_gui: Option<PluginGui>);
    fn request_resize(&mut self, size: GuiSize, cx: &mut Self::Context);
}

#[derive(Debug, Hash, Copy, Clone, Eq, PartialEq)]
pub struct ClapPluginId(usize);

pub struct ClapPluginManager<GUI>
where
    GUI: Gui,
{
    plugins: RefCell<HashMap<ClapPluginId, Rc<ClapPlugin<GUI>>>>,
    next_id: Cell<ClapPluginId>,
    receiver: Cell<Option<UnboundedReceiver<Message>>>,
    sender: UnboundedSender<Message>,
}

impl<GUI> ClapPluginManager<GUI>
where
    GUI: Gui,
{
    pub fn new() -> Rc<Self> {
        let (sender, receiver) = unbounded();

        Rc::new(Self {
            plugins: RefCell::new(HashMap::new()),
            next_id: Cell::new(ClapPluginId(1)),
            receiver: Cell::new(Some(receiver)),
            sender,
        })
    }

    pub async fn create_plugin(&self, plugin: &mut FoundPlugin) -> Rc<ClapPlugin<GUI>> {
        let id = self.next_id.get();
        self.next_id.set(ClapPluginId(id.0 + 1));

        let (clap_plugin, initialized_receiver) = ClapPlugin::new(id, plugin, self.sender.clone());
        let old_plugin = self.plugins.borrow_mut().insert(id, clap_plugin.clone());
        assert!(old_plugin.is_none());

        initialized_receiver.await.unwrap();

        clap_plugin
    }

    pub fn get_plugin(&self, clap_plugin_id: ClapPluginId) -> Rc<ClapPlugin<GUI>> {
        self.plugins.borrow().get(&clap_plugin_id).unwrap().clone()
    }

    pub async fn message_handler(
        clap_plugin_manager: Weak<ClapPluginManager<GUI>>,
        app: &mut GUI::Context,
    ) {
        println!("[message_handler] start");
        let mut receiver = clap_plugin_manager
            .upgrade()
            .unwrap()
            .receiver
            .take()
            .unwrap();

        while let Some(Message { plugin_id, payload }) = receiver.next().await {
            let plugin = {
                let Some(manager) = clap_plugin_manager.upgrade() else {
                    break;
                };
                manager.get_plugin(plugin_id)
            };

            match payload {
                MessagePayload::Initialized {
                    plugin_gui,
                    plugin_audio_ports,
                } => {
                    plugin.gui.borrow_mut().set_plugin_gui(plugin_gui);
                    *plugin.plugin_audio_ports.borrow_mut() = plugin_audio_ports;

                    let initialized = plugin
                        .initialized
                        .replace(None)
                        .expect("Plugin should only be initialized once");
                    initialized.send(()).unwrap();
                }
                MessagePayload::RunOnMainThread => {
                    plugin.plugin.borrow_mut().call_on_main_thread_callback();
                }
                MessagePayload::ResizeHintsChanged => {
                    println!("Handling changed resize hints not supported");
                }
                MessagePayload::RequestResize(new_size) => {
                    plugin.gui.borrow_mut().request_resize(new_size, app);
                }
            }
        }
        println!("[message_handler] end");
    }
}

pub struct ClapPlugin<GUI>
where
    GUI: Gui,
{
    initialized: Cell<Option<oneshot::Sender<()>>>,
    pub plugin: RefCell<PluginInstance<Self>>,
    pub gui: RefCell<GUI>,
    plugin_audio_ports: RefCell<Option<PluginAudioPorts>>,
}

impl<GUI: Gui> ClapPlugin<GUI> {
    fn new(
        clap_plugin_id: ClapPluginId,
        plugin: &mut FoundPlugin,
        sender: UnboundedSender<Message>,
    ) -> (Rc<Self>, oneshot::Receiver<()>) {
        let bundle = plugin.load_bundle();
        bundle
            .get_plugin_factory()
            .expect("Only bundles with plugin factories supported");

        let id = plugin.id.clone();

        let plugin_id = CString::new(id.as_str()).unwrap();
        let host =
            HostInfo::new("corodaw", "damyanp", "https://github.com/damyanp", "0.0.1").unwrap();

        let shared = ClapPluginShared {
            channel: sender,
            plugin_id: clap_plugin_id,
        };

        let plugin = clack_host::plugin::PluginInstance::new(
            move |_| shared,
            move |shared| ClapPluginMainThread::new(shared),
            &bundle,
            plugin_id.as_c_str(),
            &host,
        )
        .unwrap();

        let (initialized_sender, initialized_receiver) = oneshot::channel::<()>();

        let clap_plugin = Rc::new(Self {
            initialized: Cell::new(Some(initialized_sender)),
            plugin: RefCell::new(plugin),
            gui: RefCell::default(),
            plugin_audio_ports: RefCell::new(None),
        });

        (clap_plugin, initialized_receiver)
    }

    pub fn get_audio_ports(&self, is_input: bool) -> Vec<u32> {
        let audio_ports = self.plugin_audio_ports.borrow_mut();
        let mut plugin = self.plugin.borrow_mut();
        let mut handle = plugin.plugin_handle();

        audio_ports
            .map(|audio_ports| {
                let count = audio_ports.count(&mut handle, is_input);
                (0..count)
                    .map(|index| {
                        let mut buffer = AudioPortInfoBuffer::new();
                        audio_ports
                            .get(&mut handle, index, is_input, &mut buffer)
                            .unwrap()
                            .channel_count
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn get_audio_processor(&self) -> PluginAudioProcessor<ClapPlugin<GUI>> {
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

struct Message {
    plugin_id: ClapPluginId,
    payload: MessagePayload,
}

enum MessagePayload {
    Initialized {
        plugin_gui: Option<PluginGui>,
        plugin_audio_ports: Option<PluginAudioPorts>,
    },
    RunOnMainThread,
    ResizeHintsChanged,
    RequestResize(GuiSize),
}

impl<GUI> HostHandlers for ClapPlugin<GUI>
where
    GUI: Gui,
{
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
    plugin_id: ClapPluginId,
}

impl ClapPluginShared {
    fn send_message(&self, payload: MessagePayload) {
        self.channel
            .unbounded_send(Message {
                plugin_id: self.plugin_id,
                payload,
            })
            .expect("unbounded_send should always succeed");
    }
}

impl HostLogImpl for ClapPluginShared {
    fn log(&self, severity: clack_extensions::log::LogSeverity, message: &str) {
        println!("[host log] {}: {}", severity, message);
    }
}

impl HostGuiImpl for ClapPluginShared {
    fn resize_hints_changed(&self) {
        self.send_message(MessagePayload::ResizeHintsChanged);
    }

    fn request_resize(&self, new_size: GuiSize) -> Result<(), clack_host::prelude::HostError> {
        self.send_message(MessagePayload::RequestResize(new_size));
        Ok(())
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
        let _ = instance.get_extension::<PluginAudioPorts>();
    }

    fn request_restart(&self) {
        todo!()
    }

    fn request_process(&self) {
        todo!()
    }

    fn request_callback(&self) {
        self.send_message(MessagePayload::RunOnMainThread);
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
        self.timer_support = instance.get_extension();
        self.shared.send_message(MessagePayload::Initialized {
            plugin_gui: instance.get_extension(),
            plugin_audio_ports: instance.get_extension(),
        });
        self.plugin = Some(instance);
    }
}
