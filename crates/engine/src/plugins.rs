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

use clap_adapter::ClapPluginProcessor;

use audio_graph::{Node, Processor};
use discovery::FoundPlugin;
use timers::Timers;
use ui_host::PluginUiHost;

pub use crate::plugins::ui_host::GuiHandle;

mod clap_adapter;
pub mod discovery;
mod timers;
mod ui_host;

#[derive(Debug, Hash, Copy, Clone, Eq, PartialEq)]
pub struct ClapPluginId(usize);

pub struct ClapPluginManager {
    sender: Sender<Message>,
    _plugin_host: JoinHandle<()>,
}

impl Default for ClapPluginManager {
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

pub trait PluginFactory {
    fn create_plugin_sync(&self, plugin: FoundPlugin) -> ClapPluginShared;

    fn load_plugin_state(
        &self,
        clap_plugin_id: ClapPluginId,
        data: Vec<u8>,
    ) -> oneshot::Receiver<Result<(), String>>;

    fn set_title(&self, clap_plugin_id: ClapPluginId, title: String);

    fn show_gui(&self, clap_plugin_id: ClapPluginId, title: String)
    -> oneshot::Receiver<GuiHandle>;

    fn save_plugin_state(&self, clap_plugin_id: ClapPluginId)
    -> oneshot::Receiver<Option<Vec<u8>>>;

    fn create_audio_graph_node(&self, clap_plugin: &ClapPluginShared)
    -> (Node, Box<dyn Processor>);
}

impl ClapPluginManager {
    pub fn create_plugin(&self, plugin: FoundPlugin) -> oneshot::Receiver<ClapPluginShared> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Message::CreatePlugin(plugin, sender))
            .unwrap();
        receiver
    }
}

impl PluginFactory for ClapPluginManager {
    fn create_plugin_sync(&self, plugin: FoundPlugin) -> ClapPluginShared {
        let receiver = self.create_plugin(plugin);
        futures::executor::block_on(async { receiver.await.unwrap() })
    }

    fn show_gui(
        &self,
        clap_plugin_id: ClapPluginId,
        title: String,
    ) -> oneshot::Receiver<GuiHandle> {
        let (sender, receiver) = oneshot::channel();

        self.sender
            .send(Message::ShowGui(clap_plugin_id, title, sender))
            .unwrap();

        receiver
    }

    fn set_title(&self, clap_plugin_id: ClapPluginId, title: String) {
        self.sender
            .send(Message::SetTitle(clap_plugin_id, title))
            .unwrap();
    }

    fn save_plugin_state(
        &self,
        clap_plugin_id: ClapPluginId,
    ) -> oneshot::Receiver<Option<Vec<u8>>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Message::SaveState(clap_plugin_id, sender))
            .unwrap();
        receiver
    }

    fn load_plugin_state(
        &self,
        clap_plugin_id: ClapPluginId,
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
        clap_plugin: &ClapPluginShared,
    ) -> (Node, Box<dyn Processor>) {
        clap_plugin.create_audio_graph_node_sync()
    }
}

struct PluginHostThread {
    executor: LocalExecutor<'static>,
    plugins: RefCell<HashMap<ClapPluginId, Rc<ClapPlugin>>>,
    next_id: Cell<ClapPluginId>,
    sender: Sender<Message>,
    receiver: Receiver<Message>,
    plugin_ui_host: Pin<Box<PluginUiHost>>,
}

impl PluginHostThread {
    fn new(sender: Sender<Message>, receiver: Receiver<Message>) -> Rc<Self> {
        Rc::new(Self {
            executor: LocalExecutor::new(),
            plugins: RefCell::new(HashMap::new()),
            next_id: Cell::new(ClapPluginId(1)),
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
                                this.next_id.set(ClapPluginId(id.0 + 1));

                                let (clap_plugin, clap_plugin_shared) =
                                    ClapPlugin::new(id, &found_plugin, this.sender.clone()).await;
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
                        let processor = Box::new(ClapPluginProcessor::new(&clap_plugin));

                        let num_inputs = 0; // TODO: support inputs!
                        let num_outputs = processor.get_total_output_channels() as u16;

                        sender.send((num_inputs, num_outputs, processor)).unwrap();
                    }
                    Message::SaveState(clap_plugin_id, sender) => {
                        let clap_plugin = self.get_plugin(clap_plugin_id);
                        let state_ext = {
                            let plugin = clap_plugin.plugin.borrow();
                            plugin.access_shared_handler(|h: &ClapPluginShared| {
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
                            plugin.access_shared_handler(|h: &ClapPluginShared| {
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

    fn get_plugin(&self, clap_plugin_id: ClapPluginId) -> Rc<ClapPlugin> {
        self.plugins.borrow().get(&clap_plugin_id).unwrap().clone()
    }
}

pub struct ClapPlugin {
    clap_plugin_id: ClapPluginId,
    pub plugin: RefCell<PluginInstance<Self>>,
    plugin_audio_ports: RefCell<Option<PluginAudioPorts>>,
}

impl ClapPlugin {
    async fn new(
        clap_plugin_id: ClapPluginId,
        plugin: &FoundPlugin,
        sender: Sender<Message>,
    ) -> (Rc<Self>, ClapPluginShared) {
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
            plugin_name: plugin.name.clone(),
            extensions: Arc::default(),
        };

        let (initialized_sender, initialized_receiver) = oneshot::channel::<()>();

        let shared_clone = shared.clone();
        let plugin = clack_host::plugin::PluginInstance::new(
            move |_| shared_clone,
            move |_| ClapPluginMainThread::new(initialized_sender),
            &bundle,
            plugin_id.as_c_str(),
            &host,
        )
        .unwrap();

        initialized_receiver.await.unwrap();

        let audio_ports = plugin
            .access_shared_handler(|h: &ClapPluginShared| h.extensions.read().unwrap().audio_ports);

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

    pub fn get_audio_processor(&self, sample_rate: f64) -> PluginAudioProcessor<ClapPlugin> {
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

    pub fn get_id(&self) -> ClapPluginId {
        self.clap_plugin_id
    }
}

enum Message {
    CreatePlugin(FoundPlugin, oneshot::Sender<ClapPluginShared>),
    ShowGui(ClapPluginId, String, oneshot::Sender<GuiHandle>),
    SetTitle(ClapPluginId, String),
    RunOnMainThread(ClapPluginId),
    ResizeHintsChanged(ClapPluginId),
    RequestResize(ClapPluginId, GuiSize),
    CreateProcessor(
        ClapPluginId,
        oneshot::Sender<(u16, u16, Box<dyn Processor>)>,
    ),
    SaveState(ClapPluginId, oneshot::Sender<Option<Vec<u8>>>),
    LoadState(ClapPluginId, Vec<u8>, oneshot::Sender<Result<(), String>>),
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
            .register::<HostParams>()
            .register::<HostState>();
    }
}

#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub struct ClapPluginShared {
    channel: Sender<Message>,
    pub plugin_id: ClapPluginId,
    pub plugin_name: String,
    #[derivative(Debug = "ignore")]
    pub extensions: Arc<RwLock<Extensions>>,
}

#[derive(Default)]
pub struct Extensions {
    pub plugin_gui: Option<PluginGui>,
    pub audio_ports: Option<PluginAudioPorts>,
    pub plugin_state: Option<PluginState>,
}

impl ClapPluginShared {
    pub async fn create_audio_graph_node(&self) -> (Node, Box<dyn Processor>) {
        let (sender, receiver) = oneshot::channel();
        self.channel
            .send(Message::CreateProcessor(self.plugin_id, sender))
            .unwrap();
        let (num_inputs, num_outputs, processor) = receiver.await.unwrap();

        let node = Node::default().audio(num_inputs, num_outputs).event(1, 0);

        (node, processor)
    }

    pub fn create_audio_graph_node_sync(&self) -> (Node, Box<dyn Processor>) {
        futures::executor::block_on(async { self.create_audio_graph_node().await })
    }
}

impl HostLogImpl for ClapPluginShared {
    fn log(&self, severity: clack_extensions::log::LogSeverity, message: &str) {
        println!("[host log] {}: {}", severity, message);
    }
}

impl HostGuiImpl for ClapPluginShared {
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

impl<'a> HostStateImpl for ClapPluginMainThread<'a> {
    fn mark_dirty(&mut self) {
        println!("[host state] Plugin marked state as dirty");
    }
}

unsafe impl Send for ClapPluginShared {}

impl<'a> host::SharedHandler<'a> for ClapPluginShared {
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

pub struct ClapPluginMainThread<'a> {
    plugin: Option<InitializedPluginHandle<'a>>,
    initialized: Cell<Option<oneshot::Sender<()>>>,
    timer_support: Option<PluginTimer>,
    _timers: Rc<Timers>,
}

impl<'a> ClapPluginMainThread<'a> {
    fn new(initialized: oneshot::Sender<()>) -> Self {
        Self {
            plugin: None,
            initialized: Cell::new(Some(initialized)),
            timer_support: None,
            _timers: Rc::new(Timers::new()),
        }
    }
}

impl<'a> host::MainThreadHandler<'a> for ClapPluginMainThread<'a> {
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
