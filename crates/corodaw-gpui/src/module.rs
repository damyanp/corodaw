use gpui::*;
use gpui_component::{
    button::*,
    slider::{Slider, SliderEvent, SliderState},
    *,
};

use project::{model::ChannelControl, *};

use crate::CorodawProject;

pub struct Module {
    id: model::Id<model::Channel>,
    gain_slider: Entity<SliderState>,
}

impl Module {
    pub fn new(id: model::Id<model::Channel>, initial_gain: f32, cx: &mut App) -> Self {
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
                project
                    .project
                    .borrow_mut()
                    .channel_control(&id, model::ChannelControl::SetGain(slider_value.start()));
            }
        })
        .detach();

        Self { id, gain_slider }
    }

    fn on_toggle_muted(&mut self, _e: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        CorodawProject::update_global(cx, |corodaw_project, _| {
            corodaw_project
                .project
                .borrow_mut()
                .channel_control(&self.id, ChannelControl::ToggleMute);
        })
    }

    fn on_toggle_soloed(&mut self, _e: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        CorodawProject::update_global(cx, |corodaw_project, _| {
            corodaw_project
                .project
                .borrow_mut()
                .channel_control(&self.id, ChannelControl::ToggleSolo);
        })
    }

    fn on_toggle_armed(&mut self, _e: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        CorodawProject::update_global(cx, |corodaw_project, _| {
            corodaw_project
                .project
                .borrow_mut()
                .channel_control(&self.id, ChannelControl::ToggleArmed);
        })
    }

    fn on_show(&mut self, _e: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let project = CorodawProject::global(cx).project.clone();
        let id = self.id;

        cx.spawn(async move |_, cx| {
            let future = project.borrow().show_gui(id);
            future.await;
            cx.refresh().unwrap();
        })
        .detach();
    }
}

impl Render for Module {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let project = CorodawProject::global(cx).project.borrow();
        let module = project.channel(&self.id);

        if let Some(module) = module {
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
                                        .selected(module.is_muted())
                                        .on_click(cx.listener(Self::on_toggle_muted)),
                                )
                                .child(
                                    Button::new("s")
                                        .label("S")
                                        .success()
                                        .selected(module.is_soloed())
                                        .on_click(cx.listener(Self::on_toggle_soloed)),
                                )
                                .child(
                                    Button::new("r")
                                        .label("R")
                                        .danger()
                                        .selected(module.is_armed())
                                        .on_click(cx.listener(Self::on_toggle_armed)),
                                ),
                        )
                        .gap_2()
                        .child(module.name().to_owned())
                        .child(Slider::new(&self.gain_slider).min_w_128())
                        .child(
                            Button::new("show")
                                .label("Show")
                                .disabled(project.has_gui(&self.id))
                                .on_click(cx.listener(Self::on_show)),
                        ),
                )
        } else {
            div().child("Error!")
        }
    }
}
