use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ffi::CString,
    io::Cursor,
    pin::Pin,
    rc::Rc,
    sync::{
        Arc, RwLock,
        mpsc::{Receiver, Sender, TryRecvError, channel},
    },
    thread::JoinHandle,
};

use bevy_ecs::prelude::*;
use clack_extensions::{
    audio_ports::{AudioPortInfoBuffer, PluginAudioPorts},
    gui::{GuiSize, HostGui, HostGuiImpl, PluginGui},
    log::{HostLog, HostLogImpl},
    params::{HostParams, HostParamsImplMainThread, HostParamsImplShared},
    state::{HostState, HostStateImpl, PluginState},
    timer::{HostTimer, PluginTimer},
};
use clack_host::{
    host::{self, HostHandlers, HostInfo},
    plugin::{InitializedPluginHandle, InitializingPluginHandle, PluginInstance},
    process::{PluginAudioConfiguration, PluginAudioProcessor},
};
use derivative::Derivative;
use futures_channel::oneshot;
use smol::LocalExecutor;

use clap_adapter::ClapProcessor;

use audio_graph::{GraphNodeDesc, GraphProcessor};
use discovery::PluginDescriptor;
use timers::Timers;
use ui_host::PluginUiHost;

pub use crate::plugins::ui_host::PluginGuiHandle;

mod clap_adapter;
pub mod discovery;
mod timers;
mod ui_host;

#[derive(Debug, Hash, Copy, Clone, Eq, PartialEq)]
pub struct ClapId(usize);

impl ClapId {
    pub fn from_raw(id: usize) -> Self {
        Self(id)
    }
}

pub struct ClapManager {
    sender: Sender<Message>,
    _plugin_host: JoinHandle<()>,
}

impl Default for ClapManager {
    fn default() -> Self {
        let (sender, receiver) = channel();

        let plugin_host = {
            let sender = sender.clone();

            std::thread::spawn(move || {
                let host = PluginHostThread::new(sender, receiver);
                host.run();
            })
        };

        Self {
            sender,
            _plugin_host: plugin_host,
        }
    }
}

pub trait PluginManager {
    type Plugin: Component;

    fn create_plugin_sync(&self, plugin: PluginDescriptor) -> Self::Plugin;

    fn plugin_id(plugin: &Self::Plugin) -> ClapId;

    fn plugin_name(plugin: &Self::Plugin) -> &str;

    fn load_plugin_state(
        &self,
        clap_plugin_id: ClapId,
        data: Vec<u8>,
    ) -> oneshot::Receiver<Result<(), String>>;

    fn set_title(&self, clap_plugin_id: ClapId, title: String);

    fn show_gui(&self, clap_plugin_id: ClapId, title: String)
    -> oneshot::Receiver<PluginGuiHandle>;

    fn save_plugin_state(&self, clap_plugin_id: ClapId) -> oneshot::Receiver<Option<Vec<u8>>>;

    fn create_audio_graph_node(
        &self,
        plugin: &Self::Plugin,
    ) -> (GraphNodeDesc, Box<dyn GraphProcessor>);
}

impl ClapManager {
    pub fn create_plugin(&self, plugin: PluginDescriptor) -> oneshot::Receiver<ClapProxy> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Message::CreatePlugin(plugin, sender))
            .unwrap();
        receiver
    }
}

impl PluginManager for ClapManager {
    type Plugin = ClapProxy;

    fn create_plugin_sync(&self, plugin: PluginDescriptor) -> ClapProxy {
        let receiver = self.create_plugin(plugin);
        futures::executor::block_on(async { receiver.await.unwrap() })
    }

    fn plugin_id(plugin: &ClapProxy) -> ClapId {
        plugin.plugin_id
    }

    fn plugin_name(plugin: &ClapProxy) -> &str {
        &plugin.plugin_name
    }

    fn show_gui(
        &self,
        clap_plugin_id: ClapId,
        title: String,
    ) -> oneshot::Receiver<PluginGuiHandle> {
        let (sender, receiver) = oneshot::channel();

        self.sender
            .send(Message::ShowGui(clap_plugin_id, title, sender))
            .unwrap();

        receiver
    }

    fn set_title(&self, clap_plugin_id: ClapId, title: String) {
        self.sender
            .send(Message::SetTitle(clap_plugin_id, title))
            .unwrap();
    }

    fn save_plugin_state(&self, clap_plugin_id: ClapId) -> oneshot::Receiver<Option<Vec<u8>>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Message::SaveState(clap_plugin_id, sender))
            .unwrap();
        receiver
    }

    fn load_plugin_state(
        &self,
        clap_plugin_id: ClapId,
        data: Vec<u8>,
    ) -> oneshot::Receiver<Result<(), String>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Message::LoadState(clap_plugin_id, data, sender))
            .unwrap();
        receiver
    }

    fn create_audio_graph_node(
        &self,
        plugin: &ClapProxy,
    ) -> (GraphNodeDesc, Box<dyn GraphProcessor>) {
        plugin.create_audio_graph_node_sync()
    }
}

struct PluginHostThread {
    executor: LocalExecutor<'static>,
    plugins: RefCell<HashMap<ClapId, Rc<ClapInstance>>>,
    next_id: Cell<ClapId>,
    sender: Sender<Message>,
    receiver: Receiver<Message>,
    plugin_ui_host: Pin<Box<PluginUiHost>>,
}

impl PluginHostThread {
    fn new(sender: Sender<Message>, receiver: Receiver<Message>) -> Rc<Self> {
        Rc::new(Self {
            executor: LocalExecutor::new(),
            plugins: RefCell::new(HashMap::new()),
            next_id: Cell::new(ClapId(1)),
            sender,
            receiver,
            plugin_ui_host: Pin::new(Box::new(PluginUiHost::new())),
        })
    }

    fn run(self: Rc<Self>) {
        loop {
            self.message_handler();
            self.plugin_ui_host.run_message_handlers();
            while self.executor.try_tick() {}
        }
    }

    fn message_handler(self: &Rc<Self>) -> bool {
        loop {
            match self.receiver.try_recv() {
                Err(TryRecvError::Empty) => return true,
                Err(TryRecvError::Disconnected) => return false,
                Ok(message) => match message {
                    Message::CreatePlugin(found_plugin, sender) => {
                        let this = self.clone();
                        self.executor
                            .spawn(async move {
                                let id = this.next_id.get();
                                this.next_id.set(ClapId(id.0 + 1));

                                let (clap_plugin, clap_plugin_shared) =
                                    ClapInstance::new(id, &found_plugin, this.sender.clone()).await;
                                let old_plugin =
                                    this.plugins.borrow_mut().insert(id, clap_plugin.clone());
                                assert!(old_plugin.is_none());

                                sender.send(clap_plugin_shared).unwrap();
                            })
                            .detach();
                    }
                    Message::RunOnMainThread(clap_plugin_id) => {
                        let clap_plugin = self.get_plugin(clap_plugin_id);
                        clap_plugin
                            .plugin
                            .borrow_mut()
                            .call_on_main_thread_callback();
                    }
                    Message::ShowGui(clap_plugin_id, title, sender) => {
                        let clap_plugin = self.get_plugin(clap_plugin_id);

                        let this = self.clone();
                        self.executor
                            .spawn(async move {
                                println!("show gui!");
                                let handle =
                                    this.plugin_ui_host.show_gui(&clap_plugin, &title).await;
                                sender.send(handle).unwrap();
                            })
                            .detach();
                    }
                    Message::SetTitle(clap_plugin_id, title) => {
                        self.plugin_ui_host.set_title(clap_plugin_id, &title);
                    }
                    Message::ResizeHintsChanged(clap_plugin_id) => {
                        self.plugin_ui_host.resize_hints_changed(clap_plugin_id);
                    }
                    Message::RequestResize(clap_plugin_id, gui_size) => {
                        self.plugin_ui_host.request_resize(clap_plugin_id, gui_size);
                    }
                    Message::CreateProcessor(clap_plugin_id, sender) => {
                        let clap_plugin = self.get_plugin(clap_plugin_id);
                        let processor = Box::new(ClapProcessor::new(&clap_plugin));

                        let num_inputs = 0; // TODO: support inputs!
                        let num_outputs = processor.get_total_output_channels() as u16;

                        sender.send((num_inputs, num_outputs, processor)).unwrap();
                    }
                    Message::SaveState(clap_plugin_id, sender) => {
                        let clap_plugin = self.get_plugin(clap_plugin_id);
                        let state_ext = {
                            let plugin = clap_plugin.plugin.borrow();
                            plugin.access_shared_handler(|h: &ClapProxy| {
                                h.extensions.read().unwrap().plugin_state
                            })
                        };
                        let result = match state_ext {
                            Some(state_ext) => {
                                let mut buffer = Vec::new();
                                let mut plugin = clap_plugin.plugin.borrow_mut();
                                match state_ext.save(&mut plugin.plugin_handle(), &mut buffer) {
                                    Ok(()) => Some(buffer),
                                    Err(e) => {
                                        eprintln!("Failed to save plugin state: {e}");
                                        None
                                    }
                                }
                            }
                            None => {
                                eprintln!(
                                    "Warning: plugin {:?} does not support state extension",
                                    clap_plugin_id
                                );
                                None
                            }
                        };
                        sender.send(result).unwrap();
                    }
                    Message::LoadState(clap_plugin_id, data, sender) => {
                        let clap_plugin = self.get_plugin(clap_plugin_id);
                        let state_ext = {
                            let plugin = clap_plugin.plugin.borrow();
                            plugin.access_shared_handler(|h: &ClapProxy| {
                                h.extensions.read().unwrap().plugin_state
                            })
                        };
                        let result = match state_ext {
                            Some(state_ext) => {
                                let mut reader = Cursor::new(data);
                                let mut plugin = clap_plugin.plugin.borrow_mut();
                                state_ext
                                    .load(&mut plugin.plugin_handle(), &mut reader)
                                    .map_err(|e| format!("{e}"))
                            }
                            None => Err("Plugin does not support state extension".to_string()),
                        };
                        sender.send(result).unwrap();
                    }
                },
            }
        }
    }

    fn get_plugin(&self, clap_plugin_id: ClapId) -> Rc<ClapInstance> {
        self.plugins.borrow().get(&clap_plugin_id).unwrap().clone()
    }
}

pub struct ClapInstance {
    clap_plugin_id: ClapId,
    pub plugin: RefCell<PluginInstance<Self>>,
    plugin_audio_ports: RefCell<Option<PluginAudioPorts>>,
}

impl ClapInstance {
    async fn new(
        clap_plugin_id: ClapId,
        plugin: &PluginDescriptor,
        sender: Sender<Message>,
    ) -> (Rc<Self>, ClapProxy) {
        let bundle = plugin.load_bundle();
        bundle
            .get_plugin_factory()
            .expect("Only bundles with plugin factories supported");

        let id = plugin.id.clone();

        let plugin_id = CString::new(id.as_str()).unwrap();
        let host =
            HostInfo::new("corodaw", "damyanp", "https://github.com/damyanp", "0.0.1").unwrap();

        let shared = ClapProxy {
            channel: sender,
            plugin_id: clap_plugin_id,
            plugin_name: plugin.name.clone(),
            extensions: Arc::default(),
        };

        let (initialized_sender, initialized_receiver) = oneshot::channel::<()>();

        let shared_clone = shared.clone();
        let plugin = clack_host::plugin::PluginInstance::new(
            move |_| shared_clone,
            move |_| ClapMainThread::new(initialized_sender),
            &bundle,
            plugin_id.as_c_str(),
            &host,
        )
        .unwrap();

        initialized_receiver.await.unwrap();

        let audio_ports =
            plugin.access_shared_handler(|h: &ClapProxy| h.extensions.read().unwrap().audio_ports);

        let clap_plugin = Rc::new(Self {
            clap_plugin_id,
            plugin: RefCell::new(plugin),
            plugin_audio_ports: RefCell::new(audio_ports),
        });

        (clap_plugin, shared)
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

    pub fn get_audio_processor(&self, sample_rate: f64) -> PluginAudioProcessor<ClapInstance> {
        let configuration = PluginAudioConfiguration {
            sample_rate,
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

    pub fn get_id(&self) -> ClapId {
        self.clap_plugin_id
    }
}

enum Message {
    CreatePlugin(PluginDescriptor, oneshot::Sender<ClapProxy>),
    ShowGui(ClapId, String, oneshot::Sender<PluginGuiHandle>),
    SetTitle(ClapId, String),
    RunOnMainThread(ClapId),
    ResizeHintsChanged(ClapId),
    RequestResize(ClapId, GuiSize),
    CreateProcessor(ClapId, oneshot::Sender<(u16, u16, Box<dyn GraphProcessor>)>),
    SaveState(ClapId, oneshot::Sender<Option<Vec<u8>>>),
    LoadState(ClapId, Vec<u8>, oneshot::Sender<Result<(), String>>),
}

impl HostHandlers for ClapInstance {
    type Shared<'a> = ClapProxy;
    type MainThread<'a> = ClapMainThread<'a>;
    type AudioProcessor<'a> = ();

    fn declare_extensions(
        builder: &mut clack_host::prelude::HostExtensions<Self>,
        _shared: &Self::Shared<'_>,
    ) {
        builder
            .register::<HostLog>()
            .register::<HostGui>()
            .register::<HostTimer>()
            .register::<HostParams>()
            .register::<HostState>();
    }
}

#[derive(Clone, Component, Derivative)]
#[derivative(Debug)]
pub struct ClapProxy {
    channel: Sender<Message>,
    pub plugin_id: ClapId,
    pub plugin_name: String,
    #[derivative(Debug = "ignore")]
    pub extensions: Arc<RwLock<ClapExtensions>>,
}

#[derive(Default)]
pub struct ClapExtensions {
    pub plugin_gui: Option<PluginGui>,
    pub audio_ports: Option<PluginAudioPorts>,
    pub plugin_state: Option<PluginState>,
}

impl ClapProxy {
    pub async fn create_audio_graph_node(&self) -> (GraphNodeDesc, Box<dyn GraphProcessor>) {
        let (sender, receiver) = oneshot::channel();
        self.channel
            .send(Message::CreateProcessor(self.plugin_id, sender))
            .unwrap();
        let (num_inputs, num_outputs, processor) = receiver.await.unwrap();

        let node = GraphNodeDesc::default()
            .audio(num_inputs, num_outputs)
            .event(1, 0);

        (node, processor)
    }

    pub fn create_audio_graph_node_sync(&self) -> (GraphNodeDesc, Box<dyn GraphProcessor>) {
        futures::executor::block_on(async { self.create_audio_graph_node().await })
    }
}

impl HostLogImpl for ClapProxy {
    fn log(&self, severity: clack_extensions::log::LogSeverity, message: &str) {
        println!("[host log] {}: {}", severity, message);
    }
}

impl HostGuiImpl for ClapProxy {
    fn resize_hints_changed(&self) {
        self.channel
            .send(Message::ResizeHintsChanged(self.plugin_id))
            .unwrap();
    }

    fn request_resize(&self, new_size: GuiSize) -> Result<(), clack_host::prelude::HostError> {
        self.channel
            .send(Message::RequestResize(self.plugin_id, new_size))?;
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

impl<'a> HostParamsImplMainThread for ClapMainThread<'a> {
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

impl HostParamsImplShared for ClapProxy {
    fn request_flush(&self) {
        todo!()
    }
}

impl<'a> HostStateImpl for ClapMainThread<'a> {
    fn mark_dirty(&mut self) {
        println!("[host state] Plugin marked state as dirty");
    }
}

unsafe impl Send for ClapProxy {}
unsafe impl Sync for ClapProxy {}

impl<'a> host::SharedHandler<'a> for ClapProxy {
    fn initializing(&self, instance: InitializingPluginHandle<'a>) {
        let mut extensions = self.extensions.write().unwrap();
        extensions.audio_ports = instance.get_extension();
        extensions.plugin_gui = instance.get_extension();
        extensions.plugin_state = instance.get_extension();
    }

    fn request_restart(&self) {
        todo!()
    }

    fn request_process(&self) {
        todo!()
    }

    fn request_callback(&self) {
        self.channel
            .send(Message::RunOnMainThread(self.plugin_id))
            .unwrap();
    }
}

pub struct ClapMainThread<'a> {
    plugin: Option<InitializedPluginHandle<'a>>,
    initialized: Cell<Option<oneshot::Sender<()>>>,
    timer_support: Option<PluginTimer>,
    _timers: Rc<Timers>,
}

impl<'a> ClapMainThread<'a> {
    fn new(initialized: oneshot::Sender<()>) -> Self {
        Self {
            plugin: None,
            initialized: Cell::new(Some(initialized)),
            timer_support: None,
            _timers: Rc::new(Timers::new()),
        }
    }
}

impl<'a> host::MainThreadHandler<'a> for ClapMainThread<'a> {
    fn initialized(&mut self, instance: InitializedPluginHandle<'a>) {
        self.timer_support = instance.get_extension();
        self.initialized
            .replace(None)
            .expect("Plugin should only be initialized once")
            .send(())
            .unwrap();
        self.plugin = Some(instance);
    }
}
