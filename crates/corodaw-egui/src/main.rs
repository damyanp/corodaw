use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use eframe::egui::{self, ComboBox};
use engine::{
    audio::Audio,
    audio_graph::{AudioGraph, audio_graph},
    plugins::{
        ClapPlugin,
        discovery::{FoundPlugin, get_plugins},
    },
};
use smol::LocalExecutor;

use crate::{module::Module, plugins::EguiClapPluginManager};

mod module;
mod plugins;

struct Corodaw<'a> {
    this: Weak<RefCell<Self>>,

    executor: Rc<LocalExecutor<'a>>,

    found_plugins: Vec<Rc<FoundPlugin>>,
    manager: Rc<EguiClapPluginManager>,

    selected_plugin: Option<Rc<FoundPlugin>>,

    modules: Vec<Module>,
    counter: u32,

    audio_graph: Rc<RefCell<AudioGraph>>,
    _audio: Audio,
}

impl<'a> Corodaw<'a> {
    fn new(executor: Rc<LocalExecutor<'a>>) -> Rc<RefCell<Self>> {
        let manager = EguiClapPluginManager::new(&executor);
        let (audio_graph, audio_graph_worker) = audio_graph();
        let audio = Audio::new(audio_graph_worker).unwrap();

        let r = Rc::new(RefCell::new(Self {
            this: Weak::default(),
            executor,
            found_plugins: get_plugins(),
            manager,
            selected_plugin: None,
            modules: Vec::default(),
            counter: 0,
            audio_graph: Rc::new(RefCell::new(audio_graph)),
            _audio: audio,
        }));

        r.borrow_mut().this = Rc::downgrade(&r);

        r
    }

    fn update(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add_enabled_ui(self.selected_plugin.is_some(), |ui| {
                    if ui.button("Add Module").clicked() {
                        let clone = self.this.upgrade().unwrap();
                        self.executor
                            .spawn(async move {
                                Corodaw::add_module(&clone).await;
                            })
                            .detach();
                    }
                });
                ComboBox::from_id_salt("Plugin")
                    .width(ui.available_width())
                    .selected_text(display_found_plugin(&self.selected_plugin).to_string())
                    .show_ui(ui, |ui| {
                        for plugin in &self.found_plugins {
                            ui.selectable_value(
                                &mut self.selected_plugin,
                                Some(plugin.clone()),
                                plugin.name.to_owned(),
                            );
                        }
                    });
            });
            for module in &self.modules {
                module.add_to_ui(self, ui);
            }
        });
    }

    fn show_plugin_ui(&self, clap_plugin: Rc<ClapPlugin>) {
        let manager = self.manager.clone();

        self.executor
            .spawn(async move {
                manager.show_plugin_gui(clap_plugin).await;
            })
            .detach();
    }

    fn has_plugin_gui(&self, plugin: &ClapPlugin) -> bool {
        self.manager.has_plugin_gui(plugin)
    }

    async fn add_module(this: &Rc<RefCell<Self>>) {
        let module = {
            let mut s = this.borrow_mut();
            let manager = s.manager.inner.clone();

            let plugin = s.selected_plugin.as_ref().unwrap().clone();
            let name = format!("Module {}: {}", s.counter, plugin.name);
            s.counter += 1;

            let audio_graph = s.audio_graph.clone();

            Module::new(name, plugin, manager, audio_graph)
        }
        .await;

        this.borrow_mut().modules.push(module);
    }
}

fn display_found_plugin(value: &Option<Rc<FoundPlugin>>) -> &str {
    value
        .as_ref()
        .map(|plugin| plugin.name.as_str())
        .unwrap_or("<none>")
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();

    struct App<'a> {
        corodaw: Rc<RefCell<Corodaw<'a>>>,
        executor: Rc<LocalExecutor<'a>>,
    }
    impl eframe::App for App<'_> {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            while self.executor.try_tick() {}

            self.corodaw.borrow_mut().update(ctx);
        }
    }

    let executor = Rc::new(LocalExecutor::new());
    let corodaw = Corodaw::new(executor.clone());

    eframe::run_native(
        "Corodaw",
        options,
        Box::new(|_| Ok(Box::new(App { executor, corodaw }))),
    )
    .unwrap();

    println!("[main] exit");

    Ok(())
}
