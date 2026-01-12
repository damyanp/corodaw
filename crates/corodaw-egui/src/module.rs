use std::{cell::Cell, rc::Rc};

use audio_graph::NodeId;
use eframe::egui::{self, Color32, Margin, Slider, Stroke};
use engine::{
    builtin::GainControl,
    plugins::{ClapPluginManager, ClapPluginShared, discovery::FoundPlugin},
};

use crate::Corodaw;

pub struct Module {
    name: String,
    clap_plugin_shared: ClapPluginShared,
    gain: Rc<GainControl>,
    gain_value: Cell<f32>,
}

impl Module {
    pub async fn new(
        name: String,
        plugin_manager: Rc<ClapPluginManager>,
        plugin: &FoundPlugin,
    ) -> Self {
        let gain_value = 1.0;
        let gain = Rc::new(GainControl::new(&plugin_manager.audio_graph, gain_value));

        let clap_plugin_shared = plugin_manager.create_plugin(plugin.clone()).await;
        let plugin_node_id = clap_plugin_shared
            .create_audio_graph_node(&plugin_manager.audio_graph)
            .await;

        let ag = &plugin_manager.audio_graph;
        for port in 0..2 {
            ag.connect(gain.node_id, port, plugin_node_id, port);
        }

        Self {
            name,
            clap_plugin_shared,
            gain,
            gain_value: Cell::new(gain_value),
        }
    }

    pub fn get_output_node(&self) -> NodeId {
        self.gain.node_id
    }

    pub fn add_to_ui(&self, corodaw: &Corodaw, ui: &mut egui::Ui) {
        egui::Frame::new()
            .stroke(Stroke::new(1.0, Color32::WHITE))
            .inner_margin(Margin::same(5))
            .outer_margin(Margin::same(5))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.take_available_width();
                    ui.label(&self.name);

                    let mut gain_value = self.gain_value.get();
                    if ui
                        .add(Slider::new(&mut gain_value, 0.0..=1.0).show_value(false))
                        .changed()
                    {
                        self.gain_value.replace(gain_value);
                        self.gain.set_gain(gain_value);
                    }

                    // TODO
                    let has_gui = false;

                    ui.add_enabled_ui(!has_gui, |ui| {
                        if ui.button("Show").clicked() {
                            corodaw.show_plugin_ui(self.clap_plugin_shared.plugin_id);
                        }
                    });
                });
            });
    }
}
