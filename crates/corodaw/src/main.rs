use std::{cell::RefCell, rc::Rc};

use gpui::*;
use gpui_component::{
    button::*,
    select::{SearchableVec, Select, SelectItem, SelectState},
    *,
};

use crate::{
    audio::Audio,
    audio_graph::{AudioGraph, audio_graph},
    module::Module,
    plugins::{
        ClapPluginManager,
        discovery::{FoundPlugin, get_plugins},
    },
};

mod audio;
mod audio_graph;
mod builtin;
mod module;
mod plugins;

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
    clap_plugin_manager: Rc<ClapPluginManager>,
    _plugins: Vec<RefCell<FoundPlugin>>,
    plugin_selector: Entity<SelectState<SearchableVec<SelectablePlugin>>>,
    modules: Vec<Module>,
    counter: u32,
    audio_graph: Rc<RefCell<AudioGraph>>,
    _audio: Audio,
}

impl Corodaw {
    fn new(
        plugins: Vec<RefCell<FoundPlugin>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (audio_graph, audio_graph_worker) = audio_graph();
        let audio = Audio::new(audio_graph_worker).unwrap();

        let clap_plugin_manager = ClapPluginManager::new(cx);

        let searchable_plugins = SearchableVec::new(
            plugins
                .iter()
                .map(SelectablePlugin::new)
                .collect::<Vec<_>>(),
        );

        let plugin_selector = cx.new(|cx| SelectState::new(searchable_plugins, None, window, cx));

        Self {
            clap_plugin_manager,
            _plugins: plugins,
            plugin_selector,
            modules: Vec::default(), //vec![cx.new(|cx| Module::new(cx, "Master".to_owned()))],
            counter: 0,
            audio_graph: Rc::new(RefCell::new(audio_graph)),
            _audio: audio,
        }
    }

    fn on_click(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let plugin = self
            .plugin_selector
            .read(cx)
            .selected_value()
            .expect("The Add button should only be enabled if a plugin is selected")
            .clone();

        let name = format!("Module {}: {}", self.counter, plugin.borrow().name);
        self.counter += 1;

        let audio_graph = self.audio_graph.clone();

        cx.spawn(async move |e, cx| {
            let clap_plugin_manager = e
                .read_with(cx, |corodaw, _| corodaw.clap_plugin_manager.clone())
                .unwrap();

            let module = Module::new(name, clap_plugin_manager, plugin, audio_graph, cx).await;

            let module = module.expect("TODO: error handling for when module creation fails");

            e.update(cx, |corodaw, _| {
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
            .children(
                self.modules
                    .iter()
                    .map(|m| div().w_full().child(m.get_ui())),
            )
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
