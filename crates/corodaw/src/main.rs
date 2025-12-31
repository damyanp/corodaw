use gpui::*;
use gpui_component::{button::*, slider::*, *};

struct Module {
    gain_slider: Entity<SliderState>,
}

impl Module {
    pub async fn new(mut cx: AsyncApp) -> Self {
        let initial_gain = 1.0;
        let gain_slider = cx
            .new(|_| {
                SliderState::new()
                    .default_value(initial_gain)
                    .min(0.0)
                    .max(1.0)
                    .step(0.01)
            })
            .unwrap();

        Self { gain_slider }
    }
}

impl Render for Module {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let show_disabled = true;

        div()
            .border_1()
            .border_color(cx.theme().border)
            .p_5()
            .child(
                h_flex()
                    .gap_2()
                    .child("Name")
                    .child(Slider::new(&self.gain_slider).min_w_128())
                    .child(Button::new("show").label("Show").disabled(show_disabled)),
            )
    }
}

pub struct Corodaw {
    modules: Vec<Entity<Module>>,
}

impl Corodaw {
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            modules: Vec::default(), //vec![cx.new(|cx| Module::new(cx, "Master".to_owned()))],
        }
    }

    fn on_click(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.spawn(async move |e, cx| {
            let module = e
                .update(cx, |_, cx| Module::new(cx.to_async()))
                .unwrap()
                .await;

            e.update(cx, |corodaw, cx| {
                let module = cx.new(|_| module);
                corodaw.modules.push(module);
            })
            .unwrap();
        })
        .detach();
    }
}

impl Render for Corodaw {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .v_flex()
            .gap_2()
            .size_full()
            .items_start()
            .justify_start()
            .p_10()
            .child(
                div().h_flex().gap_2().w_full().child(
                    Button::new("ok")
                        .primary()
                        .label("Add Module")
                        .on_click(cx.listener(Self::on_click)),
                ),
            )
            .children(self.modules.iter().map(|m| div().w_full().child(m.clone())))
    }
}

fn main() {
    let app = Application::new();

    app.run(move |cx| {
        // This must be called before using any GPUI Component features.
        gpui_component::init(cx);

        cx.spawn(async move |cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let view = cx.new(|cx| Corodaw::new(window, cx));
                // This first level on the window, should be a Root.
                cx.new(|cx| Root::new(view, window, cx))
            })?;

            Ok::<_, anyhow::Error>(())
        })
        .detach();
    });

    println!("[main] exit");
}
