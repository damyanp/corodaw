use gpui::*;
use gpui_component::{
    button::*,
    slider::{Slider, SliderState},
    *,
};

mod plugins;

struct Module {
    name: String,
    main_volume: Entity<SliderState>,
}

impl Module {
    pub fn new(cx: &mut Context<Self>, name: String) -> Self {
        let main_volume = cx.new(|_| SliderState::new().min(0.0).max(1.0));

        Self { name, main_volume }
    }
}

impl Render for Module {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .border_1()
            .border_color(cx.theme().border)
            .p_5()
            .child(
                h_flex()
                    .gap_2()
                    .child(self.name.clone())
                    .child(Slider::new(&self.main_volume).min_w_128()),
            )
    }
}

pub struct Corodaw {
    modules: Vec<Entity<Module>>,
    counter: u32,
}

impl Corodaw {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            modules: vec![cx.new(|cx| Module::new(cx, "Master".to_owned()))],
            counter: 0,
        }
    }

    fn on_click(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.counter += 1;
        self.modules
            .push(cx.new(|cx| Module::new(cx, format!("Module {}", self.counter))));
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
                Button::new("ok")
                    .primary()
                    .label("Add Module")
                    .on_click(cx.listener(Self::on_click)),
            )
            .children(self.modules.iter().map(|m| div().w_full().child(m.clone())))
    }
}

fn main() {
    let app = Application::new();

    println!("Scanning for plugins...");
    let plugins = plugins::find_plugins();
    println!("Found {} plugins", plugins.len());
    for plugin in &plugins {
        let name = plugin
            .descriptor
            .name()
            .map(|n| n.to_str().ok())
            .flatten()
            .unwrap_or("<no name>");
        println!("Plugin: {name}");
        for feature in plugin.descriptor.features() {
            println!(
                "  - Feature: {}",
                feature.to_str().ok().unwrap_or("<bad feature>")
            )
        }
    }

    app.run(move |cx| {
        // This must be called before using any GPUI Component features.
        gpui_component::init(cx);

        cx.spawn(async move |cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let view = cx.new(|cx| Corodaw::new(cx));
                // This first level on the window, should be a Root.
                cx.new(|cx| Root::new(view, window, cx))
            })?;

            Ok::<_, anyhow::Error>(())
        })
        .detach();
    });
}
