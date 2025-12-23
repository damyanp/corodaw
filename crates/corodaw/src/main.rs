use std::io::Write;

use gpui::*;
use gpui_component::{
    button::*,
    slider::{Slider, SliderState},
    *,
};

use crate::plugins::FoundPlugin;

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

    let plugins = get_plugins();

    println!("Found {} plugins", plugins.len());
    for plugin in &plugins {
        println!("{}: {} ({})", plugin.id, plugin.name, plugin.path.display());
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

fn get_plugins() -> Vec<FoundPlugin> {
    // First try loading from cache.
    match std::fs::read_to_string(".plugins.json") {
        Ok(contents) => match serde_json::from_str::<Vec<FoundPlugin>>(&contents) {
            Ok(plugins) => {
                println!("Loaded {} plugins from .plugins.json", plugins.len());
                return plugins;
            }
            Err(err) => {
                println!("Failed to parse .plugins.json ({err}); falling back to scanning...");
            }
        },
        Err(err) => {
            println!("No .plugins.json cache found ({err}); scanning for plugins...");
        }
    }

    // Fallback: scan and then write cache.
    println!("Scanning for plugins...");
    let plugins = plugins::find_plugins();

    let plugins_json = serde_json::to_string_pretty(&plugins)
        .unwrap_or_else(|e| format!("{{\"error\":\"failed to serialize plugins: {e}\"}}"));

    // Write JSON to ".plugins.json" next to the current working directory.
    let mut f = std::fs::File::create(".plugins.json").expect("create .plugins.json");
    f.write_all(plugins_json.as_bytes())
        .and_then(|_| f.write_all(b"\n"))
        .expect("write .plugins.json");

    println!("Wrote .plugins.json");
    plugins
}
