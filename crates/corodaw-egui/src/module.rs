use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use eframe::egui::{self, Color32, Margin, Slider, Stroke};
use engine::{
    audio_graph::{AudioGraph, clap_adapter::get_audio_graph_node_desc_for_clap_plugin},
    builtin::GainControl,
    plugins::{ClapPlugin, ClapPluginManager, discovery::FoundPlugin},
};

use crate::{Corodaw, Spawner};

pub struct Module {
    name: String,
    plugin: Rc<ClapPlugin>,
    gain: Rc<GainControl>,
    gain_value: Cell<f32>,
}

impl Module {
    pub async fn new(
        name: String,
        plugin: Rc<FoundPlugin>,
        manager: Rc<ClapPluginManager<Spawner>>,
        audio_graph: Rc<RefCell<AudioGraph>>,
    ) -> Self {
        let plugin = manager.create_plugin(&plugin).await;

        let gain_value = 1.0;
        let gain = Rc::new(GainControl::default());

        let mut audio_graph = audio_graph.borrow_mut();
        let plugin_id = audio_graph.add_node(get_audio_graph_node_desc_for_clap_plugin(&plugin));
        let gain_id = audio_graph.add_node(gain.get_node_desc(gain_value));
        audio_graph.connect(plugin_id, 0, gain_id, 0);
        audio_graph.set_output_node(gain_id, true);

        Self {
            name,
            plugin,
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

                    let has_gui = corodaw.has_plugin_gui(&self.plugin);

                    ui.add_enabled_ui(!has_gui, |ui| {
                        if ui.button("Show").clicked() {
                            corodaw.show_plugin_ui(self.plugin.clone());
                        }
                    });
                });
            });
    }
}
