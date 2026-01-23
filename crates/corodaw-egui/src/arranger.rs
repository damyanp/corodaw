use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;

use corodaw_egui_widgets::arranger::{ArrangerDataProvider, ArrangerWidget};
use eframe::egui::{Align, Button, Color32, Frame, Layout, Margin, RichText, Slider, Stroke, Ui};
use project::{ChannelAudioView, ChannelControl, ChannelMessage, ChannelState};

#[derive(SystemParam)]
pub struct ArrangerData<'w, 's> {
    commands: Commands<'w, 's>,
    channels: Query<
        'w,
        's,
        (
            Entity,
            &'static Name,
            &'static ChannelState,
            Option<&'static ChannelAudioView>,
        ),
    >,
}

impl ArrangerDataProvider for ArrangerData<'_, '_> {
    fn num_channels(&self) -> usize {
        self.channels.count()
    }

    fn channel_height(&self, _: usize) -> f32 {
        100.0
    }

    fn show_channel(&mut self, index: usize, ui: &mut Ui) {
        let (entity, name, state, audio_view) = self.channels.iter().nth(index).unwrap();

        Frame::new()
            .stroke(Stroke::new(1.0, Color32::WHITE))
            .inner_margin(Margin::same(5))
            .outer_margin(Margin::same(5))
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
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
                                self.commands.write_message(ChannelMessage {
                                    channel: entity,
                                    control,
                                });
                            }
                        };

                    control_button(
                        "M",
                        Color32::ORANGE,
                        state.muted,
                        ChannelControl::ToggleMute,
                    );
                    control_button(
                        "S",
                        Color32::GREEN,
                        state.soloed,
                        ChannelControl::ToggleSolo,
                    );
                    control_button(
                        "R",
                        Color32::DARK_RED,
                        state.armed,
                        ChannelControl::ToggleArmed,
                    );

                    ui.add_space(1.0);
                    ui.label(name.as_str());

                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                        ui.take_available_space();

                        if let Some(audio_view) = audio_view {
                            let has_gui = audio_view.has_gui();

                            ui.add_enabled_ui(!has_gui, |ui| {
                                if ui.button("Show").clicked() {
                                    self.commands.write_message(ChannelMessage {
                                        channel: entity,
                                        control: ChannelControl::ShowGui,
                                    });
                                }
                            });
                        }

                        let mut gain_value = state.gain_value;
                        ui.spacing_mut().slider_width = ui.available_size().x;
                        if ui
                            .add(Slider::new(&mut gain_value, 0.0..=1.0).show_value(false))
                            .changed()
                        {
                            self.commands.write_message(ChannelMessage {
                                channel: entity,
                                control: ChannelControl::SetGain(gain_value),
                            });
                        }
                    });
                });
            });
    }

    fn show_strip(&mut self, _: usize, _: &mut Ui) {}
}

pub fn arranger_ui(mut ui: InMut<Ui>, data: ArrangerData) {
    ArrangerWidget::new("arranger").show(data, &mut ui);
}
