use bevy_ecs::name::Name;
use gpui::*;
use gpui_component::{
    button::*,
    slider::{Slider, SliderEvent, SliderState},
    *,
};

use project::*;

use crate::CorodawProject;

pub struct Module {
    bevy_entity: bevy_ecs::entity::Entity,
    gain_slider: Entity<SliderState>,
}

impl Module {
    pub fn new(bevy_entity: bevy_ecs::entity::Entity, initial_gain: f32, cx: &mut App) -> Self {
        let gain_slider = cx.new(|_| {
            SliderState::new()
                .default_value(initial_gain)
                .min(0.0)
                .max(1.0)
                .step(0.01)
        });

        cx.subscribe(&gain_slider, move |_, event, cx| match event {
            SliderEvent::Change(slider_value) => {
                let project: &mut CorodawProject = cx.global_mut();
                project.0.write_message(ChannelMessage {
                    channel: bevy_entity,
                    control: ChannelControl::SetGain(slider_value.start()),
                });
            }
        })
        .detach();

        Self {
            bevy_entity,
            gain_slider,
        }
    }

    fn on_toggle_muted(&mut self, _e: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        CorodawProject::update_global(cx, |corodaw_project, _| {
            corodaw_project.0.write_message(ChannelMessage {
                channel: self.bevy_entity,
                control: ChannelControl::ToggleMute,
            });
        })
    }

    fn on_toggle_soloed(&mut self, _e: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        CorodawProject::update_global(cx, |corodaw_project, _| {
            corodaw_project.0.write_message(ChannelMessage {
                channel: self.bevy_entity,
                control: ChannelControl::ToggleSolo,
            });
        })
    }

    fn on_toggle_armed(&mut self, _e: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        CorodawProject::update_global(cx, |corodaw_project, _| {
            corodaw_project.0.write_message(ChannelMessage {
                channel: self.bevy_entity,
                control: ChannelControl::ToggleArmed,
            });
        })
    }

    fn on_show(&mut self, _e: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        CorodawProject::update_global(cx, |corodaw_project, _| {
            corodaw_project.0.write_message(ChannelMessage {
                channel: self.bevy_entity,
                control: ChannelControl::ShowGui,
            })
        });
    }
}

impl Render for Module {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let world = CorodawProject::global(cx).0.get_world();
        let bevy_entity = world.entity(self.bevy_entity);
        let state = bevy_entity.get::<ChannelState>();
        let has_gui = bevy_entity
            .get::<ChannelAudioView>()
            .map(|v| v.has_gui())
            .unwrap_or(false);

        if let Some(state) = state {
            let name = bevy_entity
                .get::<Name>()
                .expect("If the channel has state it should have a name");
            div()
                .border_1()
                .border_color(cx.theme().border)
                .p_5()
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            ButtonGroup::new("controls")
                                .small()
                                .outline()
                                .child(
                                    Button::new("m")
                                        .label("M")
                                        .warning()
                                        .selected(state.muted)
                                        .on_click(cx.listener(Self::on_toggle_muted)),
                                )
                                .child(
                                    Button::new("s")
                                        .label("S")
                                        .success()
                                        .selected(state.soloed)
                                        .on_click(cx.listener(Self::on_toggle_soloed)),
                                )
                                .child(
                                    Button::new("r")
                                        .label("R")
                                        .danger()
                                        .selected(state.armed)
                                        .on_click(cx.listener(Self::on_toggle_armed)),
                                ),
                        )
                        .gap_2()
                        .child(name.as_str().to_owned())
                        .child(Slider::new(&self.gain_slider).min_w_128())
                        .child(
                            Button::new("show")
                                .label("Show")
                                .disabled(has_gui)
                                .on_click(cx.listener(Self::on_show)),
                        ),
                )
        } else {
            div().child("Error!")
        }
    }
}
