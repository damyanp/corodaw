use std::cell::Cell;

use eframe::egui::{self, Button, Color32, Margin, RichText, Slider, Stroke};
use project::{model::ModuleControl, *};

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
        let (name, muted, soloed, armed) = {
            let project = corodaw.project.borrow();
            let module = project.module(&self.id).unwrap();
            (
                module.name().to_owned(),
                module.is_muted(),
                module.is_soloed(),
                module.is_armed(),
            )
        };

        egui::Frame::new()
            .stroke(Stroke::new(1.0, Color32::WHITE))
            .inner_margin(Margin::same(5))
            .outer_margin(Margin::same(5))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.take_available_width();

                    let mut control_button =
                        |label: &str, color: Color32, selected: bool, control: ModuleControl| {
                            let color = if selected {
                                color
                            } else {
                                color.gamma_multiply(0.5)
                            };

                            if ui
                                .add(
                                    Button::new(RichText::new(label).color(Color32::BLACK))
                                        .fill(color)
                                        .selected(selected),
                                )
                                .clicked()
                            {
                                corodaw
                                    .project
                                    .borrow_mut()
                                    .module_control(&self.id, control);
                            }
                        };

                    control_button("M", Color32::ORANGE, muted, ModuleControl::ToggleMute);
                    control_button("S", Color32::GREEN, soloed, ModuleControl::ToggleSolo);
                    control_button("R", Color32::DARK_RED, armed, ModuleControl::ToggleArmed);

                    ui.add_space(1.0);
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
                            .module_control(&self.id, model::ModuleControl::SetGain(gain_value));
                    }

                    let has_gui = { corodaw.project.borrow().has_gui(&self.id) };

                    ui.add_enabled_ui(!has_gui, |ui| {
                        if ui.button("Show").clicked() {
                            let project = corodaw.project.clone();
                            let id = self.id;
                            corodaw
                                .executor
                                .spawn(async move {
                                    let future = project.borrow().show_gui(id);
                                    future.await;
                                })
                                .detach();
                        }
                    });
                });
            });
    }
}
