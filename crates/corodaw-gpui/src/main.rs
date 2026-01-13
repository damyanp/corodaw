use std::rc::Rc;

use audio_graph::{AudioGraph, NodeId};
use engine::{
    audio::Audio,
    builtin::Summer,
    plugins::{
        ClapPluginManager,
        discovery::{FoundPlugin, get_plugins},
    },
};
use gpui::*;
use gpui_component::{
    button::*,
    select::{SearchableVec, Select, SelectItem, SelectState},
    *,
};

use crate::module::Module;

mod module;

#[derive(Clone)]
struct SelectablePlugin(FoundPlugin);

impl SelectablePlugin {
    fn new(p: FoundPlugin) -> Self {
        Self(p)
    }
}

impl SelectItem for SelectablePlugin {
    type Value = FoundPlugin;

    fn title(&self) -> SharedString {
        SharedString::new(self.0.name.as_str())
    }

    fn value(&self) -> &Self::Value {
        &self.0
    }
}

pub struct Corodaw {
    clap_plugin_manager: Rc<ClapPluginManager>,
    plugin_selector: Entity<SelectState<SearchableVec<SelectablePlugin>>>,
    modules: Vec<Module>,
    summer: NodeId,
    counter: u32,
    _audio: Audio,
}

impl Corodaw {
    fn new(plugins: Vec<FoundPlugin>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (audio_graph, audio_graph_worker) = AudioGraph::new();
        let audio = Audio::new(audio_graph_worker).unwrap();

        let summer = audio_graph.add_node(0, 2, Box::new(Summer));
        audio_graph.set_output_node(summer);

        let clap_plugin_manager = Rc::new(ClapPluginManager::new(audio_graph));

        let searchable_plugins = SearchableVec::new(
            plugins
                .into_iter()
                .map(SelectablePlugin::new)
                .collect::<Vec<_>>(),
        );

        let plugin_selector = cx.new(|cx| SelectState::new(searchable_plugins, None, window, cx));

        Self {
            clap_plugin_manager,
            plugin_selector,
            modules: Vec::default(),
            counter: 0,
            _audio: audio,
            summer,
        }
    }

    fn add_module(&mut self, module: Module) {
        for port in 0..2 {
            self.clap_plugin_manager.audio_graph.connect_grow_inputs(
                self.summer,
                self.modules.len() * 2 + port,
                module.get_output_node(),
                port,
            );
        }
        self.modules.push(module);

        self.clap_plugin_manager.audio_graph.update();
    }

    fn on_click(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let plugin = self
            .plugin_selector
            .read(cx)
            .selected_value()
            .expect("The Add button should only be enabled if a plugin is selected")
            .clone();

        let name = format!("Module {}: {}", self.counter, plugin.name);
        self.counter += 1;

        cx.spawn(async move |e, cx| {
            let clap_plugin_manager = e
                .read_with(cx, |corodaw, _| corodaw.clap_plugin_manager.clone())
                .unwrap();

            let module = Module::new(name, clap_plugin_manager, &plugin, cx).await;

            let module = module.expect("TODO: error handling for when module creation fails");

            e.update(cx, |corodaw, _| {
                corodaw.add_module(module);
            })
            .unwrap();

            cx.refresh().unwrap();
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
