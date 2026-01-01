use std::{cell::RefCell, rc::Rc};

use gpui::*;
use gpui_component::{
    button::*,
    slider::{Slider, SliderEvent, SliderState},
    *,
};

use engine::{
    audio_graph::{AudioGraph, clap_adapter::get_audio_graph_node_desc_for_clap_plugin},
    builtin::GainControl,
    plugins::{ClapPlugin, ClapPluginManager, Gui, discovery::FoundPlugin},
};

use crate::gui::GpuiPluginGui;

pub struct Module<GUI>
where
    GUI: Gui,
{
    _audio: ModuleAudio<GUI>,
    ui: Entity<ModuleUI>,
}

struct ModuleAudio<GUI>
where
    GUI: Gui,
{
    plugin: Rc<ClapPlugin<GUI>>,
    gain: Rc<GainControl>,
}

struct ModuleUI {
    name: String,
    gain_slider: Entity<SliderState>,
    plugin: Rc<ClapPlugin<GpuiPluginGui>>,
}

impl Module<GpuiPluginGui> {
    pub async fn new(
        name: String,
        plugin_manager: Rc<ClapPluginManager<GpuiPluginGui>>,
        plugin: RefCell<FoundPlugin>,
        audio_graph: Rc<RefCell<AudioGraph>>,
        cx: &mut AsyncApp,
    ) -> Result<Self> {
        let initial_gain = 1.0;

        let audio = ModuleAudio::new(plugin_manager, plugin, audio_graph, initial_gain).await;
        let ui = cx.new(|cx| ModuleUI::new(name, initial_gain, &audio, cx))?;

        Ok(Self { _audio: audio, ui })
    }

    pub fn get_ui(&self) -> AnyElement {
        self.ui.clone().into_any_element()
    }
}

impl<GUI> ModuleAudio<GUI>
where
    GUI: Gui,
{
    async fn new(
        plugin_manager: Rc<ClapPluginManager<GUI>>,
        mut plugin: RefCell<FoundPlugin>,
        audio_graph: Rc<RefCell<AudioGraph>>,
        initial_gain: f32,
    ) -> ModuleAudio<GUI> {
        let plugin = RefCell::get_mut(&mut plugin);
        let plugin = plugin_manager.create_plugin(plugin).await;

        let gain = Rc::new(GainControl::default());

        let mut audio_graph = audio_graph.borrow_mut();
        let plugin_id = audio_graph.add_node(get_audio_graph_node_desc_for_clap_plugin(&plugin));
        let gain_id = audio_graph.add_node(gain.get_node_desc(initial_gain));

        audio_graph.connect(plugin_id, 0, gain_id, 0);
        audio_graph.set_output_node(gain_id, true);

        Self { plugin, gain }
    }
}

impl ModuleUI {
    fn new(
        name: impl Into<String>,
        initial_gain: f32,
        module_audio: &ModuleAudio<GpuiPluginGui>,
        cx: &mut App,
    ) -> ModuleUI {
        let gain_slider = cx.new(|_| {
            SliderState::new()
                .default_value(initial_gain)
                .min(0.0)
                .max(1.0)
                .step(0.01)
        });

        let gain = module_audio.gain.clone();
        cx.subscribe(&gain_slider, move |_, event, _| match event {
            SliderEvent::Change(slider_value) => gain.set_gain(slider_value.start()),
        })
        .detach();

        Self {
            name: name.into(),
            gain_slider,
            plugin: module_audio.plugin.clone(),
        }
    }

    fn on_show(&mut self, _e: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let p = self.plugin.clone();
        self.plugin
            .gui
            .borrow_mut()
            .as_mut()
            .expect("on_show should only be called for a plugin that has a gui")
            .show(p, window, cx);
    }
}

impl Render for ModuleUI {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let show_disabled = self
            .plugin
            .gui
            .borrow()
            .as_ref()
            .map(|g| g.has_gui())
            .unwrap_or(true);

        div()
            .border_1()
            .border_color(cx.theme().border)
            .p_5()
            .child(
                h_flex()
                    .gap_2()
                    .child(self.name.clone())
                    .child(Slider::new(&self.gain_slider).min_w_128())
                    .child(
                        Button::new("show")
                            .label("Show")
                            .disabled(show_disabled)
                            .on_click(cx.listener(Self::on_show)),
                    ),
            )
    }
}
