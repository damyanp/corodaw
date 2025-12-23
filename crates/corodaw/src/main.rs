use gpui::*;
use gpui_component::{
    button::*,
    select::{SearchableVec, Select, SelectItem, SelectState},
    slider::{Slider, SliderState},
    *,
};

use crate::plugins::{FoundPlugin, get_plugins};

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

impl SelectItem for FoundPlugin {
    type Value = Self;

    fn title(&self) -> SharedString {
        self.name.clone()
    }

    fn value(&self) -> &Self::Value {
        self
    }
}

pub struct Corodaw {
    plugins: Entity<SelectState<SearchableVec<FoundPlugin>>>,
    modules: Vec<Entity<Module>>,
    counter: u32,
}

impl Corodaw {
    fn new(plugins: Vec<FoundPlugin>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let plugins = cx.new(|cx| SelectState::new(SearchableVec::new(plugins), None, window, cx));

        Self {
            plugins,
            modules: vec![cx.new(|cx| Module::new(cx, "Master".to_owned()))],
            counter: 0,
        }
    }

    fn on_click(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let plugin = self
            .plugins
            .read(cx)
            .selected_value()
            .expect("The Add button should only be enabled if a plugin is selected");
        let name = plugin.name.clone();

        self.counter += 1;
        self.modules
            .push(cx.new(|cx| Module::new(cx, format!("Module {}: {}", self.counter, name))));
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
                div()
                    .h_flex()
                    .gap_2()
                    .w_full()
                    .child(
                        Button::new("ok")
                            .primary()
                            .label("Add Module")
                            .on_click(cx.listener(Self::on_click))
                            .disabled(
                                self.plugins
                                    .read_with(cx, |p, _| p.selected_value().is_none()),
                            ),
                    )
                    .child(Select::new(&self.plugins)),
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
                let view = cx.new(|cx| Corodaw::new(plugins, window, cx));
                // This first level on the window, should be a Root.
                cx.new(|cx| Root::new(view, window, cx))
            })?;

            Ok::<_, anyhow::Error>(())
        })
        .detach();
    });
}
