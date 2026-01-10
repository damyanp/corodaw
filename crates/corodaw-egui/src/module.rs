use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use eframe::egui::{self, Color32, Margin, Slider, Stroke};
use engine::{
    audio_graph::AudioGraph,
    builtin::GainControl,
    plugins::{ClapPluginId, ClapPluginManager, discovery::FoundPlugin},
};

use crate::Corodaw;

pub struct Module {
    name: String,
    clap_plugin_id: ClapPluginId,
    gain: Rc<GainControl>,
    gain_value: Cell<f32>,
}

impl Module {
    pub async fn new(
        name: String,
        plugin: FoundPlugin,
        manager: Rc<ClapPluginManager>,
        audio_graph: Rc<RefCell<AudioGraph>>,
    ) -> Self {
        let clap_plugin_id = manager.create_plugin(plugin).await;

        let gain_value = 1.0;
        let gain = Rc::new(GainControl::default());

        let plugin_node_desc = manager.get_audio_graph_node_desc(clap_plugin_id).await;

        let mut audio_graph = audio_graph.borrow_mut();
        let plugin_id = audio_graph.add_node(plugin_node_desc);
        let gain_id = audio_graph.add_node(gain.get_node_desc(gain_value));
        audio_graph.connect(plugin_id, 0, gain_id, 0);
        audio_graph.set_output_node(gain_id, true);

        Self {
            name,
            clap_plugin_id,
            gain,
            gain_value: Cell::new(gain_value),
        }
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
                            corodaw.show_plugin_ui(self.clap_plugin_id);
                        }
                    });
                });
            });
    }
}
