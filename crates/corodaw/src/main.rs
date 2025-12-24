#![allow(unused)]
use std::{cell::RefCell, rc::Rc, sync::Arc};

use gpui::*;
use gpui_component::{
    button::*,
    select::{SearchableVec, Select, SelectItem, SelectState},
    slider::{Slider, SliderState},
    *,
};

use crate::{
    plugins::{FoundPlugin, get_plugins},
    project::ClapPlugin,
};

mod plugins;
mod project;

struct Module {
    name: String,
    plugin: ClapPlugin,
    main_volume: Entity<SliderState>,
}

impl Module {
    pub fn new(name: String, mut plugin: RefCell<FoundPlugin>, cx: &mut App) -> Self {
        let main_volume = cx.new(|_| SliderState::new().min(0.0).max(1.0));

        let plugin = RefCell::get_mut(&mut plugin);

        let plugin = ClapPlugin::new(plugin, cx);

        Self {
            name,
            plugin,
            main_volume,
        }
    }

    fn on_show(&mut self, e: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.plugin.show_gui(window, cx);
    }
}

impl Render for Module {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let show_disabled = self.plugin.has_gui();

        div()
            .border_1()
            .border_color(cx.theme().border)
            .p_5()
            .child(
                h_flex()
                    .gap_2()
                    .child(self.name.clone())
                    .child(Slider::new(&self.main_volume).min_w_128())
                    .child(
                        Button::new("show")
                            .label("Show")
                            .disabled(show_disabled)
                            .on_click(cx.listener(Self::on_show)),
                    ),
            )
    }
}

#[derive(Clone)]
struct SelectablePlugin(RefCell<FoundPlugin>);

impl SelectablePlugin {
    fn new(p: &RefCell<FoundPlugin>) -> Self {
        SelectablePlugin(p.clone())
    }
}

impl SelectItem for SelectablePlugin {
    type Value = RefCell<FoundPlugin>;

    fn title(&self) -> SharedString {
        self.0.borrow().name.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.0
    }
}

pub struct Corodaw {
    plugins: Vec<RefCell<FoundPlugin>>,
    plugin_selector: Entity<SelectState<SearchableVec<SelectablePlugin>>>,
    modules: Vec<Entity<Module>>,
    counter: u32,
}

impl Corodaw {
    fn new(
        plugins: Vec<RefCell<FoundPlugin>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let searchable_plugins = SearchableVec::new(
            plugins
                .iter()
                .map(SelectablePlugin::new)
                .collect::<Vec<_>>(),
        );

        let plugin_selector = cx.new(|cx| SelectState::new(searchable_plugins, None, window, cx));

        Self {
            plugins,
            plugin_selector,
            modules: Vec::default(), //vec![cx.new(|cx| Module::new(cx, "Master".to_owned()))],
            counter: 0,
        }
    }

    fn on_click(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let plugin = self
            .plugin_selector
            .read(cx)
            .selected_value()
            .expect("The Add button should only be enabled if a plugin is selected")
            .clone();

        let module = cx.new(|cx| {
            let name = format!("Module {}: {}", self.counter, plugin.borrow().name);

            Module::new(name, plugin, cx)
        });

        self.modules.push(module);
        self.counter += 1;
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
                            .disabled(self.plugin_selector.read(cx).selected_value().is_none()),
                    )
                    .child(Select::new(&self.plugin_selector)),
            )
            .children(self.modules.iter().map(|m| div().w_full().child(m.clone())))
    }
}

fn main() {
    let app = Application::new();

    let plugins = get_plugins();

    println!("Found {} plugins", plugins.len());
    for plugin in &plugins {
        let plugin = plugin.borrow();
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

    println!("[main] exit");
}
