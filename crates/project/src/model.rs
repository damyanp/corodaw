use std::marker::PhantomData;

use audio_graph::{AudioGraph, NodeId};
use derivative::Derivative;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use engine::{
    audio::Audio,
    builtin::{GainControl, MidiInputNode, Summer},
    plugins::{ClapPluginManager, ClapPluginShared, discovery::FoundPlugin},
};

#[derive(Serialize)]
pub struct Project {
    modules: Vec<Module>,

    #[serde(skip)]
    audio_graph: AudioGraph,

    #[serde(skip)]
    clap_plugin_manager: ClapPluginManager,

    #[serde(skip)]
    _midi_input: MidiInputNode,

    #[serde(skip)]
    summer: NodeId,

    #[serde(skip)]
    _audio: Audio,
}

impl Default for Project {
    fn default() -> Self {
        let (audio_graph, audio_graph_worker) = AudioGraph::new();
        let audio = Audio::new(audio_graph_worker).unwrap();

        let midi_input = MidiInputNode::new(&audio_graph);

        let summer = audio_graph.add_node(0, 2, Box::new(Summer));
        audio_graph.set_output_node(summer);

        audio_graph.add_input_node(summer, midi_input.node_id);

        audio_graph.update();

        let clap_plugin_manager = ClapPluginManager::default();

        Self {
            modules: Vec::default(),
            audio_graph,
            clap_plugin_manager,
            _midi_input: midi_input,
            summer,
            _audio: audio,
        }
    }
}

impl Project {
    pub fn num_modules(&self) -> usize {
        self.modules.len()
    }

    pub fn audio_graph(&self) -> AudioGraph {
        self.audio_graph.clone()
    }

    pub fn clap_plugin_manager(&self) -> ClapPluginManager {
        self.clap_plugin_manager.clone()
    }

    pub fn add_module(&mut self, module: Module) -> Id<Module> {
        let module_id = module.id();

        for port in 0..2 {
            self.audio_graph.connect_grow_inputs(
                self.summer,
                self.num_modules() * 2 + port,
                module.output_node(),
                port,
            );
        }

        self.modules.push(module);

        self.audio_graph.update();

        module_id
    }

    pub fn module(&self, id: &Id<Module>) -> Option<&Module> {
        self.modules.iter().find(|m| m.id == *id)
    }

    pub fn module_mut(&mut self, id: &Id<Module>) -> Option<&mut Module> {
        self.modules.iter_mut().find(|m| m.id == *id)
    }

    pub fn module_control(&mut self, id: &Id<Module>, control: ModuleControl) {
        self.module_mut(id).unwrap().control(control);

        let has_soloed = self.modules.iter().any(|m| m.is_soloed());
        for module in self.modules.iter_mut() {
            let muted = module.is_muted() || (has_soloed && !module.is_soloed());
            module.update_gain(muted);
        }
    }

    pub fn show_gui(&self, id: Id<Module>) -> impl Future<Output = ()> + 'static {
        self.module(&id)
            .unwrap()
            .show_gui(self.clap_plugin_manager.clone())
    }

    pub fn has_gui(&self, id: &Id<Module>) -> bool {
        self.module(id).unwrap().has_gui(&self.clap_plugin_manager)
    }
}

pub enum ModuleControl {
    SetGain(f32),
    ToggleMute,
    ToggleSolo,
    ToggleArmed,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Module {
    id: Id<Module>,
    name: String,
    plugin_id: String,
    gain_value: f32,
    muted: bool,
    soloed: bool,
    armed: bool,

    #[serde(skip)]
    clap_plugin: Option<ClapPluginShared>,

    #[serde(skip)]
    gain_control: Option<GainControl>,
}

impl Module {
    pub async fn new(
        name: String,
        audio_graph: &AudioGraph,
        clap_plugin_manager: &ClapPluginManager,
        found_plugin: &FoundPlugin,
        gain_value: f32,
    ) -> Module {
        let clap_plugin = clap_plugin_manager
            .create_plugin(found_plugin.clone())
            .await;

        let gain_control = GainControl::new(audio_graph, gain_value);
        let plugin_node_id = clap_plugin.create_audio_graph_node(audio_graph).await;

        // TODO: this assumes ports 0 & 1 are the right ones to connect!
        for port in 0..2 {
            audio_graph.connect(gain_control.node_id, port, plugin_node_id, port);
        }

        Self {
            id: Id::new(),
            name,
            plugin_id: found_plugin.id.clone(),
            gain_value,
            muted: false,
            soloed: false,
            armed: false,
            clap_plugin: Some(clap_plugin),
            gain_control: Some(gain_control),
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn id(&self) -> Id<Module> {
        self.id
    }

    fn control(&mut self, control: ModuleControl) {
        match control {
            ModuleControl::SetGain(gain) => self.gain_value = gain,
            ModuleControl::ToggleMute => self.muted = !self.muted,
            ModuleControl::ToggleSolo => self.soloed = !self.soloed,
            ModuleControl::ToggleArmed => self.armed = !self.armed,
        }
    }

    fn update_gain(&self, muted: bool) {
        if let Some(gain_control) = &self.gain_control {
            gain_control.set_gain(if muted { 0.0 } else { self.gain_value });
        }
    }

    pub fn gain(&self) -> f32 {
        self.gain_value
    }

    pub fn output_node(&self) -> NodeId {
        self.gain_control.as_ref().unwrap().node_id
    }

    fn show_gui(
        &self,
        clap_plugin_manager: ClapPluginManager,
    ) -> impl Future<Output = ()> + 'static {
        let clap_plugin_id = self.clap_plugin.as_ref().unwrap().plugin_id;
        async move {
            clap_plugin_manager.show_gui(clap_plugin_id).await;
        }
    }

    fn has_gui(&self, clap_plugin_manager: &ClapPluginManager) -> bool {
        clap_plugin_manager.has_gui(&self.clap_plugin.as_ref().unwrap().plugin_id)
    }

    pub fn is_muted(&self) -> bool {
        self.muted
    }

    pub fn is_soloed(&self) -> bool {
        self.soloed
    }

    pub fn is_armed(&self) -> bool {
        self.armed
    }
}

#[derive(Derivative, Serialize, Deserialize, Debug)]
#[derivative(Copy, Clone, Eq, PartialEq)]
pub struct Id<T> {
    uuid: Uuid,
    _phantom: PhantomData<T>,
}

impl<T> Id<T> {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            uuid: Uuid::new_v4(),
            _phantom: PhantomData,
        }
    }
}
