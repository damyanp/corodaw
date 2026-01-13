use std::cell::Cell;

use eframe::egui::{self, Color32, Margin, Slider, Stroke};
use project::*;

use crate::Corodaw;

pub struct Module {
    id: model::Id<model::Module>,
    gain_value: Cell<f32>,
}

impl Module {
    pub fn new(id: model::Id<model::Module>, initial_gain: f32) -> Self {
        Self {
            id,
            gain_value: Cell::new(initial_gain),
        }
    }

    pub fn add_to_ui(&self, corodaw: &Corodaw, ui: &mut egui::Ui) {
        let name = corodaw
            .project
            .borrow()
            .module(&self.id)
            .unwrap()
            .name()
            .to_owned();

        egui::Frame::new()
            .stroke(Stroke::new(1.0, Color32::WHITE))
            .inner_margin(Margin::same(5))
            .outer_margin(Margin::same(5))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.take_available_width();
                    ui.label(name);

                    let mut gain_value = self.gain_value.get();
                    if ui
                        .add(Slider::new(&mut gain_value, 0.0..=1.0).show_value(false))
                        .changed()
                    {
                        self.gain_value.replace(gain_value);

                        corodaw
                            .project
                            .borrow_mut()
                            .module_mut(&self.id)
                            .unwrap()
                            .set_gain(gain_value);
                    }

                    // TODO
                    let has_gui = false;

                    ui.add_enabled_ui(!has_gui, |ui| {
                        if ui.button("Show").clicked() {
                            corodaw.project.borrow().show_gui(&self.id);
                        }
                    });
                });
            });
    }
}
