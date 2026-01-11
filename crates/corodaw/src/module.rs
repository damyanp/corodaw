use std::rc::Rc;

use gpui::*;
use gpui_component::{
    button::*,
    slider::{Slider, SliderEvent, SliderState},
    *,
};

use engine::{
    audio_graph::NodeId,
    builtin::GainControl,
    plugins::{ClapPluginId, ClapPluginManager, ClapPluginShared, discovery::FoundPlugin},
};

pub struct Module {
    audio: ModuleAudio,
    ui: Entity<ModuleUI>,
}

struct ModuleAudio {
    clap_plugin_shared: ClapPluginShared,
    gain: Rc<GainControl>,
}

struct ModuleUI {
    name: String,
    gain_slider: Entity<SliderState>,
    clap_plugin_id: ClapPluginId,
    plugin_manager: Rc<ClapPluginManager>,
}

impl Module {
    pub async fn new(
        name: String,
        plugin_manager: Rc<ClapPluginManager>,
        plugin: &FoundPlugin,
        cx: &mut AsyncApp,
    ) -> Result<Self> {
        let initial_gain = 1.0;

        let audio = ModuleAudio::new(plugin_manager.clone(), plugin, initial_gain).await;

        let ui = cx.new(|cx| ModuleUI::new(name, initial_gain, &audio, plugin_manager, cx))?;

        Ok(Self { audio, ui })
    }

    pub fn get_ui(&self) -> AnyElement {
        self.ui.clone().into_any_element()
    }

    pub fn get_output_node(&self) -> NodeId {
        self.audio.gain.node_id
    }
}

impl ModuleAudio {
    async fn new(
        plugin_manager: Rc<ClapPluginManager>,
        plugin: &FoundPlugin,
        initial_gain: f32,
    ) -> ModuleAudio {
        let gain = Rc::new(GainControl::new(&plugin_manager.audio_graph, initial_gain));

        let clap_plugin_shared = plugin_manager.create_plugin(plugin.clone()).await;
        let plugin_node_id = clap_plugin_shared
            .create_audio_graph_node(&plugin_manager.audio_graph)
            .await;

        let ag = &plugin_manager.audio_graph;

        // TODO: this assumes ports 0 & 1 are the right ones to connect!
        for port in 0..2 {
            ag.connect(gain.node_id, port, plugin_node_id, port);
        }

        Self {
            clap_plugin_shared,
            gain,
        }
    }
}

impl ModuleUI {
    fn new(
        name: impl Into<String>,
        initial_gain: f32,
        module_audio: &ModuleAudio,
        manager: Rc<ClapPluginManager>,
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
            plugin_manager: manager,
            clap_plugin_id: module_audio.clap_plugin_shared.plugin_id,
        }
    }

    fn on_show(&mut self, _e: &ClickEvent, _: &mut Window, _: &mut Context<Self>) {
        self.plugin_manager.show_gui(self.clap_plugin_id);
    }
}

impl Render for ModuleUI {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let show_disabled = false; // TODO

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
