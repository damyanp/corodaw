use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;

use corodaw_widgets::arranger::{ArrangerDataProvider, ArrangerWidget};
use eframe::egui::{
    Button, Color32, Frame, Id, Key, Label, Margin, Popup, RichText, Sense, Slider, Stroke,
    TextEdit, Ui,
};
use project::{
    AvailablePlugin, ChannelAudioView, ChannelControl, ChannelData, ChannelMessage, ChannelOrder,
    ChannelState,
};

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
    available_plugins: Query<'w, 's, &'static AvailablePlugin>,
    channel_order: Single<'w, 's, &'static mut ChannelOrder>,
    messages: MessageWriter<'w, ChannelMessage>,
}

impl ArrangerDataProvider for ArrangerData<'_, '_> {
    fn num_channels(&self) -> usize {
        self.channel_order.as_ref().channel_order.len()
    }

    fn channel_height(&self, _: usize) -> f32 {
        100.0
    }

    fn show_channel(&mut self, index: usize, ui: &mut Ui) {
        let entity = *self
            .channel_order
            .as_ref()
            .channel_order
            .get(index)
            .expect("ChannelOrder index out of bounds");

        let (entity, name, state, audio_view) = self.channels.get(entity).unwrap();

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
                    show_channel_name_editor(&mut messages, entity, name, ui);
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
                            &mut self.commands,
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
        self.channel_order.as_mut().add_channel(&mut self.commands);
    }
}

fn show_available_plugins_menu(
    commands: &mut Commands,
    channel_entity: Entity,
    available_plugins: Query<'_, '_, &'static AvailablePlugin, ()>,
    ui: &mut Ui,
) {
    for AvailablePlugin(found_plugin) in available_plugins.iter() {
        if ui.button(found_plugin.name.as_str()).clicked() {
            commands.entity(channel_entity).insert(ChannelData {
                plugin_id: found_plugin.id.clone(),
            });
        }
    }
}

fn show_channel_name_editor(
    messages: &mut Vec<ChannelMessage>,
    entity: Entity,
    name: &Name,
    ui: &mut Ui,
) {
    let name_edit_id = Id::new(("channel_name_edit", entity));
    let mut edit_value = ui
        .ctx()
        .data_mut(|d| d.get_temp::<Option<String>>(name_edit_id))
        .unwrap_or(None);

    if let Some(value) = edit_value.as_mut() {
        let response = ui.add(TextEdit::singleline(value).id(name_edit_id));
        let cancel = ui.input(|i| i.key_pressed(Key::Escape));
        let commit = response.lost_focus() || ui.input(|i| i.key_pressed(Key::Enter));
        if cancel {
            edit_value = None;
        } else if commit {
            let trimmed = value.trim();
            if !trimmed.is_empty() && trimmed != name.as_str() {
                messages.push(ChannelMessage {
                    channel: entity,
                    control: ChannelControl::SetName(trimmed.to_owned()),
                });
            }
            edit_value = None;
        }
    } else {
        let response = ui.add(Label::new(name.as_str()).sense(Sense::click()));
        if response.clicked() {
            edit_value = Some(name.as_str().to_owned());
            ui.ctx().memory_mut(|m| m.request_focus(name_edit_id));
        }
    }

    ui.ctx()
        .data_mut(|d| d.insert_temp(name_edit_id, edit_value));
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
