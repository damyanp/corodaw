use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
};

use futures::StreamExt;
use futures_channel::mpsc::{UnboundedReceiver, unbounded};
use gpui::*;
use gpui_component::{
    button::*,
    select::{SearchableVec, Select, SelectItem, SelectState},
    *,
};
#[cfg(feature = "plugin-ui-host")]
use plugin_ui_host::PluginUiHost;

#[cfg(feature = "internal-ui-host")]
use crate::gui::GpuiPluginGui;

use crate::module::Module;
use engine::{
    audio::Audio,
    audio_graph::{AudioGraph, audio_graph},
    plugins::{
        ClapPlugin, ClapPluginId, ClapPluginManager, GuiMessage, GuiMessagePayload,
        discovery::{FoundPlugin, get_plugins},
    },
};

mod module;

#[cfg(feature = "internal-ui-host")]
mod gui;

#[derive(Clone)]
struct SelectablePlugin(Rc<FoundPlugin>);

impl SelectablePlugin {
    fn new(p: Rc<FoundPlugin>) -> Self {
        SelectablePlugin(p)
    }
}

impl SelectItem for SelectablePlugin {
    type Value = Rc<FoundPlugin>;

    fn title(&self) -> SharedString {
        SharedString::new(self.0.name.as_str())
    }

    fn value(&self) -> &Self::Value {
        &self.0
    }
}

struct GpuiClapPluginManager {
    inner: Rc<ClapPluginManager>,

    #[cfg(feature = "internal-ui-host")]
    guis: RefCell<HashMap<ClapPluginId, Rc<GpuiPluginGui>>>,

    #[cfg(feature = "plugin-ui-host")]
    ui_host: Rc<PluginUiHost>,
}

impl GpuiClapPluginManager {
    pub fn new(cx: &App) -> Rc<Self> {
        let (gui_sender, gui_receiver) = unbounded();

        let inner = ClapPluginManager::new(gui_sender);

        Self::spawn_message_handler(cx, Rc::downgrade(&inner));

        let manager = Rc::new(GpuiClapPluginManager {
            inner,
            #[cfg(feature = "internal-ui-host")]
            guis: RefCell::default(),

            #[cfg(feature = "plugin-ui-host")]
            ui_host: PluginUiHost::new(),
        });
        Self::spawn_gui_message_handler(cx, Rc::downgrade(&manager), gui_receiver);

        #[cfg(feature = "plugin-ui-host")]
        Self::spawn_ui_host_message_handler(cx, manager.ui_host.clone());

        manager
    }

    #[cfg(feature = "internal-ui-host")]
    pub fn create_ui(self: &Rc<Self>, plugin: Rc<ClapPlugin>) -> Option<Rc<GpuiPluginGui>> {
        let plugin_gui = plugin.plugin.borrow_mut().plugin_handle().get_extension();
        if let Some(plugin_gui) = plugin_gui {
            let gui = Rc::new(GpuiPluginGui::new(plugin.clone(), plugin_gui));
            self.guis.borrow_mut().insert(plugin.get_id(), gui.clone());
            Some(gui)
        } else {
            None
        }
    }

    fn spawn_message_handler(cx: &App, manager: Weak<ClapPluginManager>) {
        cx.spawn(async move |_| ClapPluginManager::message_handler(manager).await)
            .detach();
    }

    fn spawn_gui_message_handler(
        cx: &App,
        manager: Weak<Self>,
        mut receiver: UnboundedReceiver<GuiMessage>,
    ) {
        cx.spawn(async move |cx| {
            println!("[gui_message_handler] start");
            while let Some(GuiMessage { plugin_id, payload }) = receiver.next().await {
                #[cfg(feature = "internal-ui-host")]
                {
                    let plugin = {
                        let Some(manager) = manager.upgrade() else {
                            break;
                        };
                        manager.guis.borrow().get(&plugin_id).unwrap().clone()
                    };

                    match payload {
                        GuiMessagePayload::ResizeHintsChanged => {
                            println!("Handling changed resize hints not supported");
                        }
                        GuiMessagePayload::RequestResize(size) => {
                            plugin.request_resize(size, cx);
                        }
                    }
                }
                #[cfg(feature = "plugin-ui-host")]
                {
                    let Some(manager) = manager.upgrade() else {
                        break;
                    };
                    manager
                        .ui_host
                        .handle_gui_message(GuiMessage { plugin_id, payload });
                }
            }

            println!("[gui_message_handler] end");
        })
        .detach();
    }

    #[cfg(feature = "plugin-ui-host")]
    fn spawn_ui_host_message_handler(cx: &App, plugin_ui_host: Rc<PluginUiHost>) {
        cx.spawn(async move |cx| {
            println!("[ui_host_message_handler] start");
            plugin_ui_host.message_handler().await;
            println!("[ui_host_message_handler] end");
        })
        .detach();
    }
}

pub struct Corodaw {
    clap_plugin_manager: Rc<GpuiClapPluginManager>,
    plugin_selector: Entity<SelectState<SearchableVec<SelectablePlugin>>>,
    modules: Vec<Module>,
    counter: u32,
    audio_graph: Rc<RefCell<AudioGraph>>,
    _audio: Audio,
}

impl Corodaw {
    fn new(plugins: Vec<Rc<FoundPlugin>>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (audio_graph, audio_graph_worker) = audio_graph();
        let audio = Audio::new(audio_graph_worker).unwrap();

        let clap_plugin_manager = GpuiClapPluginManager::new(cx);

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

        let name = format!("Module {}: {}", self.counter, plugin.name);
        self.counter += 1;

        let audio_graph = self.audio_graph.clone();

        cx.spawn(async move |e, cx| {
            let clap_plugin_manager = e
                .read_with(cx, |corodaw, _| corodaw.clap_plugin_manager.clone())
                .unwrap();

            let module = Module::new(name, clap_plugin_manager, &plugin, audio_graph, cx).await;

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
