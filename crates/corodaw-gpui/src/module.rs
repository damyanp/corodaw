use gpui::*;
use gpui_component::{
    button::*,
    slider::{Slider, SliderEvent, SliderState},
    *,
};

use project::*;

use crate::CorodawProject;

pub struct Module {
    id: model::Id<model::Module>,
    gain_slider: Entity<SliderState>,
}

impl Module {
    pub fn new(id: model::Id<model::Module>, initial_gain: f32, cx: &mut App) -> Self {
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
                    .module_mut(&id)
                    .unwrap()
                    .set_gain(slider_value.start());
            }
        })
        .detach();

        Self { id, gain_slider }
    }

    fn on_show(&mut self, _e: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let project: &CorodawProject = cx.global();
        let project = project.project.clone();
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
        let project: &CorodawProject = cx.global();
        let project = project.project.borrow();
        let module = project.module(&self.id);
        let show_disabled = module
            .map(|module| module.has_gui(&project.clap_plugin_manager()))
            .unwrap_or(true);

        if let Some(module) = module {
            div()
                .border_1()
                .border_color(cx.theme().border)
                .p_5()
                .child(
                    h_flex()
                        .gap_2()
                        .child(module.name().to_owned())
                        .child(Slider::new(&self.gain_slider).min_w_128())
                        .child(
                            Button::new("show")
                                .label("Show")
                                .disabled(show_disabled)
                                .on_click(cx.listener(Self::on_show)),
                        ),
                )
        } else {
            div().child("Error!")
        }
    }
}
