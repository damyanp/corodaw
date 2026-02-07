use audio_graph::{StateReader, StateValue};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;

use corodaw_widgets::arranger::{ArrangerDataProvider, ArrangerWidget};
use corodaw_widgets::meter::Meter;
use eframe::egui::text::{CCursor, CCursorRange};
use eframe::egui::{
    Align, Align2, Button, Color32, FontId, Frame, Id, Key, Label, Layout, Margin, Popup, Rect,
    RichText, Sense, Slider, Stroke, TextEdit, Ui, pos2, vec2,
};
use egui_extras::{Size, StripBuilder};
use project::{
    AvailablePlugin, ChannelAudioView, ChannelControl, ChannelData, ChannelGainControl,
    ChannelMessage, ChannelOrder, ChannelState,
};

#[derive(SystemParam)]
#[expect(clippy::type_complexity)]
pub struct ArrangerData<'w, 's> {
    commands: Commands<'w, 's>,
    channels: Query<
        'w,
        's,
        (
            Entity,
            &'static Name,
            &'static ChannelState,
            Option<&'static ChannelGainControl>,
            Option<&'static ChannelAudioView>,
        ),
    >,
    available_plugins: Query<'w, 's, &'static AvailablePlugin>,
    channel_order: Single<'w, 's, &'static mut ChannelOrder>,
    state_reader: NonSend<'w, StateReader>,
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

        let Ok((entity, name, state, gain_control, audio_view)) = self.channels.get(entity) else {
            return;
        };

        let mut messages: Vec<ChannelMessage> = Vec::new();

        let peaks = gain_control.and_then(|gc| self.state_reader.get(&gc.0.entity));

        Frame::new()
            .stroke(Stroke::new(1.0, Color32::WHITE))
            .inner_margin(Margin::same(5))
            .outer_margin(Margin::same(0))
            .show(ui, |ui| {
                StripBuilder::new(ui)
                    .size(Size::remainder())
                    .size(Size::exact(20.0))
                    .horizontal(|mut strip| {
                        strip.strip(|builder| {
                            builder
                                .size(Size::remainder())
                                .size(Size::remainder())
                                .vertical(|mut strip| {
                                    strip.cell(|ui| {
                                        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                                            mute_solo_arm_buttons(&mut messages, entity, state, ui);
                                            show_channel_name_editor(
                                                &mut messages,
                                                entity,
                                                name,
                                                ui,
                                            );
                                        });
                                    });
                                    strip.cell(|ui| {
                                        ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                                            let input_button_response;

                                            if let Some(audio_view) = audio_view {
                                                input_button_response = ui.button("🎵");
                                                show_gui_button(
                                                    &mut messages,
                                                    entity,
                                                    audio_view,
                                                    ui,
                                                );
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

                                            show_gain_slider(&mut messages, entity, state, ui);
                                        });
                                    });
                                });
                        });
                        strip.cell(|ui| show_meters(peaks, ui));
                    });
            });

        self.messages.write_batch(messages);
    }

    fn show_strip(&mut self, _: usize, ui: &mut Ui) {
        let strip_rect = ui.available_rect_before_wrap();

        const MEASURES: usize = 32;
        const BEATS_PER_MEASURE: usize = 4;
        const BEAT_WIDTH: f32 = 20.0;

        let r = Rect::from_min_size(
            strip_rect.min,
            vec2(
                MEASURES as f32 * BEATS_PER_MEASURE as f32 * BEAT_WIDTH,
                strip_rect.height(),
            ),
        );

        ui.advance_cursor_after_rect(r);

        let p = ui.painter();

        p.rect_filled(r, 10.0, Color32::LIGHT_BLUE);

        for measure in 0..MEASURES {
            let x = r.min.x + (measure * BEATS_PER_MEASURE) as f32 * BEAT_WIDTH;

            p.vline(
                x,
                strip_rect.shrink(30.0).y_range(),
                Stroke::new(2.0, Color32::DARK_BLUE),
            );

            p.text(
                pos2(x, strip_rect.top() + 10.0),
                Align2::CENTER_TOP,
                format!("{measure}"),
                FontId::default(),
                Color32::BLACK,
            );

            for beat in 1..BEATS_PER_MEASURE {
                let x = x + beat as f32 * BEAT_WIDTH;

                p.vline(
                    x,
                    strip_rect.shrink(30.0).y_range(),
                    Stroke::new(1.0, Color32::DARK_BLUE),
                );
            }
        }

        // ui.painter()
        //     .rect_filled(r, 5.0, ui.style().visuals.widgets.inactive.bg_fill);
    }

    fn on_add_channel(&mut self, index: usize) {
        self.channel_order
            .as_mut()
            .add_channel(&mut self.commands, index);
    }

    fn move_channel(&mut self, index: usize, destination: usize) {
        self.channel_order.as_mut().move_channel(index, destination);
    }

    fn show_channel_menu(&mut self, index: usize, ui: &mut Ui) {
        let entity = *self
            .channel_order
            .as_ref()
            .channel_order
            .get(index)
            .expect("ChannelOrder index out of bounds");

        let (_entity, name, _state, _gain_control, _audio_view) =
            self.channels.get(entity).unwrap();

        ui.label(name.as_str());
        ui.separator();
        if ui.button("Delete").clicked() {
            self.channel_order
                .as_mut()
                .delete_channel(&mut self.commands, index);
        }
        ui.separator();
        if ui.button("Add Channel").clicked() {
            self.on_add_channel((index + 1).min(self.num_channels()));
        }
    }

    fn show_strip_menu(&mut self, _: usize, ui: &mut Ui) {
        // nothing
        ui.close();
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
                plugin_state: None,
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
    // When we click on the label we switch to letting us rename the channel
    let name_edit_id = Id::new(("channel_name_edit", entity));
    let mut edit_value = ui
        .ctx()
        .data_mut(|d| d.get_temp::<Option<String>>(name_edit_id))
        .unwrap_or(None);

    // We want the text to be selected when the text box is initially created
    let name_edit_select_all_id = Id::new(("channel_name_select_all", entity));
    let mut select_all = ui
        .ctx()
        .data_mut(|d| d.get_temp::<bool>(name_edit_select_all_id))
        .unwrap_or(false);

    if let Some(value) = edit_value.as_mut() {
        let response = ui.add(TextEdit::singleline(value).id(name_edit_id));
        if select_all {
            if let Some(mut state) = TextEdit::load_state(ui.ctx(), name_edit_id) {
                let char_count = value.chars().count();
                let range = CCursorRange::two(CCursor::new(0), CCursor::new(char_count));
                state.cursor.set_char_range(Some(range));
                TextEdit::store_state(ui.ctx(), name_edit_id, state);
            }
            select_all = false;
        }
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
        ui.with_layout(Layout::top_down_justified(Align::Min), |ui| {
            let response = ui.add(
                Label::new(name.as_str())
                    .sense(Sense::click())
                    .wrap_mode(eframe::egui::TextWrapMode::Truncate),
            );
            if response.clicked() {
                edit_value = Some(name.as_str().to_owned());
                select_all = true;
                ui.ctx().memory_mut(|m| m.request_focus(name_edit_id));
            }
        });
    }

    ui.ctx().data_mut(|d| {
        d.insert_temp(name_edit_id, edit_value);
        d.insert_temp(name_edit_select_all_id, select_all);
    });
}

fn show_gain_slider(
    messages: &mut Vec<ChannelMessage>,
    entity: Entity,
    state: &ChannelState,
    ui: &mut Ui,
) {
    let mut gain_value = state.gain_value;
    ui.vertical(|ui| {
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
    });
}

fn show_meters(peaks: Option<&StateValue>, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(1.0, 0.0);

        let values: &[f32] = match peaks.unwrap_or(&StateValue::None) {
            StateValue::None => &[],
            StateValue::Mono(v) => &[*v],
            StateValue::Stereo(l, r) => &[*l, *r],
        };

        let h = ui.available_height();
        ui.add(Meter::new(values).height(h).width(ui.available_width()));
    });
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
        |label: &str, color: Color32, selected: bool, control: fn(bool) -> ChannelControl| {
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
                    control: control(!selected),
                });
            }
        };

    control_button("M", Color32::ORANGE, state.muted, ChannelControl::Mute);
    control_button("S", Color32::GREEN, state.soloed, ChannelControl::Solo);
    control_button("R", Color32::DARK_RED, state.armed, ChannelControl::Armed);
}

pub fn arranger_ui_system(mut ui: InMut<Ui>, data: ArrangerData) {
    ArrangerWidget::new("arranger").show(data, &mut ui);
}
