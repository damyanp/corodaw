use std::cell::Cell;

use bevy_ecs::{entity::Entity, name::Name};
use eframe::egui::{self, Button, Color32, Margin, RichText, Slider, Stroke};
use project::*;

pub struct Module {
    entity: Entity,
    gain_value: Cell<f32>,
}

impl Module {
    pub fn new(entity: Entity, initial_gain: f32) -> Self {
        Self {
            entity,
            gain_value: Cell::new(initial_gain),
        }
    }

    pub fn add_to_ui(&self, project: &mut Project, ui: &mut egui::Ui) {
        let world = project.get_world();
        let entity = world.entity(self.entity);
        let name = entity.get::<Name>().unwrap().as_str().to_owned();
        let has_gui = entity
            .get::<ChannelAudioView>()
            .map(|v| v.has_gui())
            .unwrap_or(false);

        let (muted, soloed, armed) = {
            let state = entity.get::<ChannelState>().unwrap();
            (state.muted, state.soloed, state.armed)
        };

        egui::Frame::new()
            .stroke(Stroke::new(1.0, Color32::WHITE))
            .inner_margin(Margin::same(5))
            .outer_margin(Margin::same(5))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.take_available_width();

                    let mut control_button =
                        |label: &str, color: Color32, selected: bool, control: ChannelControl| {
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
                                project.write_message(ChannelMessage {
                                    channel: self.entity,
                                    control,
                                });
                            }
                        };

                    control_button("M", Color32::ORANGE, muted, ChannelControl::ToggleMute);
                    control_button("S", Color32::GREEN, soloed, ChannelControl::ToggleSolo);
                    control_button("R", Color32::DARK_RED, armed, ChannelControl::ToggleArmed);

                    ui.add_space(1.0);
                    ui.label(name);

                    let mut gain_value = self.gain_value.get();
                    if ui
                        .add(Slider::new(&mut gain_value, 0.0..=1.0).show_value(false))
                        .changed()
                    {
                        self.gain_value.replace(gain_value);

                        project.write_message(ChannelMessage {
                            channel: self.entity,
                            control: ChannelControl::SetGain(gain_value),
                        });
                    }

                    ui.add_enabled_ui(!has_gui, |ui| {
                        if ui.button("Show").clicked() {
                            project.write_message(ChannelMessage {
                                channel: self.entity,
                                control: ChannelControl::ShowGui,
                            });
                        }
                    });
                });
            });
    }
}
