use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;

use corodaw_widgets::arranger::{ArrangerDataProvider, ArrangerWidget};
use eframe::egui::{Button, Color32, Frame, Margin, Popup, RichText, Slider, Stroke, Ui};
use project::{AvailablePlugin, ChannelAudioView, ChannelControl, ChannelMessage, ChannelState};

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
    available_plugins: Query<'w, 's, (Entity, &'static AvailablePlugin)>,
    messages: MessageWriter<'w, ChannelMessage>,
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

        let mut messages: Vec<ChannelMessage> = Vec::new();

        Frame::new()
            .stroke(Stroke::new(1.0, Color32::WHITE))
            .inner_margin(Margin::same(5))
            .outer_margin(Margin::same(5))
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                ui.horizontal(|ui| {
                    ui.take_available_width();

                    mute_solo_arm_buttons(&mut messages, entity, state, ui);

                    ui.add_space(1.0);
                    ui.label(name.as_str());
                    ui.add_space(1.0);

                    show_gain_slider(&mut messages, entity, state, ui);
                });
                ui.horizontal(|ui| {
                    let input_button_response;

                    if let Some(audio_view) = audio_view {
                        input_button_response = ui.button("🎵");
                        show_gui_button(&mut messages, entity, audio_view, ui);
                    } else {
                        input_button_response = ui.button("?");
                    }

                    Popup::menu(&input_button_response).show(|ui| {
                        show_available_plugins_menu(
                            &mut messages,
                            entity,
                            self.available_plugins,
                            ui,
                        );
                    });
                });
            });

        self.messages.write_batch(messages);
    }

    fn show_strip(&mut self, _: usize, ui: &mut Ui) {
        let r = ui.available_rect_before_wrap();
        ui.painter()
            .rect_filled(r, 5.0, ui.style().visuals.widgets.inactive.bg_fill);
    }

    fn on_add_channel(&mut self) {
        self.commands.spawn(project::new_channel());
    }
}

fn show_available_plugins_menu(
    messages: &mut Vec<ChannelMessage>,
    channel_entity: Entity,
    available_plugins: Query<'_, '_, (Entity, &'static AvailablePlugin), ()>,
    ui: &mut Ui,
) {
    for (plugin_entity, AvailablePlugin(found_plugin)) in available_plugins.iter() {
        if ui.button(found_plugin.name.as_str()).clicked() {
            messages.push(ChannelMessage {
                channel: channel_entity,
                control: ChannelControl::SetPlugin(plugin_entity),
            });
        }
    }
}

fn show_gain_slider(
    messages: &mut Vec<ChannelMessage>,
    entity: Entity,
    state: &ChannelState,
    ui: &mut Ui,
) {
    let mut gain_value = state.gain_value;
    ui.spacing_mut().slider_width = ui.available_size().x;
    if ui
        .add(Slider::new(&mut gain_value, 0.0..=1.0).show_value(false))
        .changed()
    {
        messages.push(ChannelMessage {
            channel: entity,
            control: ChannelControl::SetGain(gain_value),
        });
    }
}

fn show_gui_button(
    messages: &mut Vec<ChannelMessage>,
    entity: Entity,
    audio_view: &ChannelAudioView,
    ui: &mut Ui,
) {
    let has_gui = audio_view.has_gui();

    ui.add_enabled_ui(!has_gui, |ui| {
        if ui.button("Show").clicked() {
            messages.push(ChannelMessage {
                channel: entity,
                control: ChannelControl::ShowGui,
            });
        }
    });
}

fn mute_solo_arm_buttons(
    messages: &mut Vec<ChannelMessage>,
    entity: Entity,
    state: &ChannelState,
    ui: &mut Ui,
) {
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
                messages.push(ChannelMessage {
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
}

pub fn arranger_ui(mut ui: InMut<Ui>, data: ArrangerData) {
    ArrangerWidget::new("arranger").show(data, &mut ui);
}
