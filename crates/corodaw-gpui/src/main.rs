use std::{cell::RefCell, rc::Rc};

use engine::plugins::discovery::{FoundPlugin, get_plugins};
use project::*;

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

#[derive(Default)]
pub struct CorodawProject {
    project: Rc<RefCell<model::Project>>,
}

impl Global for CorodawProject {}

impl CorodawProject {
    async fn new_module(name: String, plugin: &FoundPlugin, cx: &AsyncApp) -> Module {
        let initial_gain = 1.0;

        let project = cx
            .read_global(|project: &CorodawProject, _| project.project.clone())
            .unwrap();

        let audio_graph = project.borrow().audio_graph();
        let clap_plugin_manager = project.borrow().clap_plugin_manager();

        let module = model::Channel::new(
            name,
            &audio_graph,
            &clap_plugin_manager,
            plugin,
            initial_gain,
        )
        .await;
        let module_id = project.borrow_mut().add_channel(module);

        cx.update(|cx| Module::new(module_id, initial_gain, cx))
            .unwrap()
    }
}

pub struct Corodaw {
    plugin_selector: Entity<SelectState<SearchableVec<SelectablePlugin>>>,
    modules: Vec<Entity<Module>>,
}

impl Corodaw {
    fn new(plugins: Vec<FoundPlugin>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.set_global(CorodawProject::default());

        let searchable_plugins = SearchableVec::new(
            plugins
                .into_iter()
                .map(SelectablePlugin::new)
                .collect::<Vec<_>>(),
        );

        let plugin_selector = cx.new(|cx| SelectState::new(searchable_plugins, None, window, cx));

        Self {
            plugin_selector,
            modules: Vec::new(),
        }
    }

    async fn add_module(this: Entity<Corodaw>, cx: &mut AsyncApp) {
        let (plugin, name) = cx
            .read_entity(&this, |corodaw, cx| {
                let plugin = corodaw
                    .plugin_selector
                    .read(cx)
                    .selected_value()
                    .expect("The Add button should only be enabled if a plugin is selected")
                    .clone();
                let name = format!("Module {}: {}", corodaw.modules.len() + 1, plugin.name);
                (plugin, name)
            })
            .unwrap();

        let module = CorodawProject::new_module(name, &plugin, cx).await;

        cx.update_entity(&this, |corodaw, cx| {
            corodaw.modules.push(cx.new(|_| module));
        })
        .unwrap();

        cx.refresh().unwrap();
    }

    fn on_click(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.spawn(async move |e, cx| {
            let corodaw = e.upgrade().unwrap();
            Self::add_module(corodaw, cx).await;
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

    println!("[main] exit");
}
