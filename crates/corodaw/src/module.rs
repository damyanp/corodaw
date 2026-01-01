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
    plugins::{ClapPlugin, ClapPluginManager, discovery::FoundPlugin},
};

pub struct Module {
    _audio: ModuleAudio,
    ui: Entity<ModuleUI>,
}

struct ModuleAudio {
    plugin: Rc<ClapPlugin>,
    gain: Rc<GainControl>,
}

struct ModuleUI {
    name: String,
    gain_slider: Entity<SliderState>,
    plugin: Rc<ClapPlugin>,
}

impl Module {
    pub async fn new(
        name: String,
        plugin_manager: Rc<ClapPluginManager>,
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

impl ModuleAudio {
    async fn new(
        plugin_manager: Rc<ClapPluginManager>,
        mut plugin: RefCell<FoundPlugin>,
        audio_graph: Rc<RefCell<AudioGraph>>,
        initial_gain: f32,
    ) -> ModuleAudio {
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
        module_audio: &ModuleAudio,
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
        self.plugin.show_gui(window, cx);
    }
}

impl Render for ModuleUI {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let show_disabled = self.plugin.has_gui();

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
