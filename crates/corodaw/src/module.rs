use std::{cell::RefCell, rc::Rc};

use gpui::*;
use gpui_component::{
    button::*,
    slider::{Slider, SliderEvent, SliderState},
    *,
};

use crate::{
    audio_graph::{AudioGraph, NodeId, clap_adapter::get_audio_graph_node_desc_for_clap_plugin},
    builtin::GainControl,
    plugins::{ClapPlugin, ClapPluginManager, discovery::FoundPlugin},
};

pub struct Module {
    name: String,
    plugin: Rc<ClapPlugin>,
    gain_slider: Entity<SliderState>,
    _gain: Entity<GainControl>,
    _plugin_id: NodeId,
    _gain_id: NodeId,
}

impl Module {
    pub async fn new(
        name: String,
        plugin_manager: Rc<ClapPluginManager>,
        mut plugin: RefCell<FoundPlugin>,
        audio_graph: Rc<RefCell<AudioGraph>>,
        mut cx: AsyncApp,
    ) -> Self {
        let initial_gain = 1.0;
        let gain_slider = cx
            .new(|_| {
                SliderState::new()
                    .default_value(initial_gain)
                    .min(0.0)
                    .max(1.0)
                    .step(0.01)
            })
            .unwrap();
        let gain = cx.new(|_| GainControl::default()).unwrap();

        let plugin = RefCell::get_mut(&mut plugin);
        let plugin = plugin_manager.create_plugin(plugin).await;

        let mut audio_graph = audio_graph.borrow_mut();
        let plugin_id = audio_graph.add_node(get_audio_graph_node_desc_for_clap_plugin(&plugin));

        let gain_id = gain
            .update(&mut cx, |gain, _| {
                audio_graph.add_node(gain.get_node_desc(initial_gain))
            })
            .unwrap();

        audio_graph.connect(plugin_id, 0, gain_id, 0);
        audio_graph.set_output_node(gain_id, true);

        let gain_for_subscription = gain.clone();
        cx.subscribe(&gain_slider, move |_, event, cx| match event {
            SliderEvent::Change(slider_value) => {
                gain_for_subscription.update(cx, |gain, _| gain.set_gain(slider_value.start()))
            }
        })
        .unwrap()
        .detach();

        Self {
            name,
            plugin,
            _gain: gain,
            gain_slider,
            _plugin_id: plugin_id,
            _gain_id: gain_id,
        }
    }

    fn on_show(&mut self, _e: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.plugin.show_gui(window, cx);
    }
}

impl Render for Module {
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
