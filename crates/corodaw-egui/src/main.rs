use std::{cell::RefCell, rc::Rc};

use eframe::egui::{self, ComboBox};
use engine::{
    audio::Audio,
    audio_graph::{AudioGraph, audio_graph},
    plugins::{
        ClapPlugin, ClapPluginManager, MainThreadSpawn,
        discovery::{FoundPlugin, get_plugins},
    },
};
use smol::LocalExecutor;

use crate::module::Module;

mod module;

struct Corodaw {
    executor: Rc<LocalExecutor<'static>>,

    found_plugins: Vec<Rc<FoundPlugin>>,
    manager: Rc<ClapPluginManager<Spawner>>,

    selected_plugin: Option<Rc<FoundPlugin>>,

    modules: Rc<RefCell<Vec<Module>>>,
    counter: u32,

    audio_graph: Rc<RefCell<AudioGraph>>,
    _audio: Audio,
}

impl Corodaw {
    fn new(executor: Rc<LocalExecutor<'static>>) -> Self {
        let manager = ClapPluginManager::new(Spawner(executor.clone()));
        let (audio_graph, audio_graph_worker) = audio_graph();
        let audio = Audio::new(audio_graph_worker).unwrap();

        Self {
            executor,
            found_plugins: get_plugins(),
            manager,
            selected_plugin: None,
            modules: Rc::default(),
            counter: 0,
            audio_graph: Rc::new(RefCell::new(audio_graph)),
            _audio: audio,
        }
    }

    fn update(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add_enabled_ui(self.selected_plugin.is_some(), |ui| {
                    if ui.button("Add Module").clicked() {
                        self.add_module();
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
            for module in self.modules.borrow().iter() {
                module.add_to_ui(self, ui);
            }
        });
    }

    fn show_plugin_ui(&self, clap_plugin: Rc<ClapPlugin>) {
        let manager = self.manager.clone();

        self.executor
            .spawn(async move {
                manager.show_gui(&clap_plugin).await;
            })
            .detach();
    }

    fn has_plugin_gui(&self, plugin: &Rc<ClapPlugin>) -> bool {
        self.manager.has_gui(plugin)
    }

    fn add_module(&mut self) {
        let plugin = self.selected_plugin.as_ref().unwrap().clone();
        let name = format!("Module {}: {}", self.counter, plugin.name);
        self.counter += 1;

        let modules = self.modules.clone();
        let audio_graph = self.audio_graph.clone();
        let manager = self.manager.clone();

        self.executor
            .spawn(async move {
                let module = Module::new(name, plugin, manager, audio_graph).await;
                modules.borrow_mut().push(module);
            })
            .detach();
    }
}

fn display_found_plugin(value: &Option<Rc<FoundPlugin>>) -> &str {
    value
        .as_ref()
        .map(|plugin| plugin.name.as_str())
        .unwrap_or("<none>")
}

#[derive(Clone)]
struct Spawner(Rc<LocalExecutor<'static>>);

impl MainThreadSpawn for Spawner {
    fn spawn(&self, future: impl Future<Output = ()> + 'static) {
        self.0.spawn(future).detach();
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();

    struct App {
        corodaw: Corodaw,
        executor: Rc<LocalExecutor<'static>>,
    }
    impl eframe::App for App {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            while self.executor.try_tick() {}

            self.corodaw.update(ctx);
        }
    }

    eframe::run_native(
        "Corodaw",
        options,
        Box::new(|_| {
            let executor = Rc::new(LocalExecutor::new());
            let corodaw = Corodaw::new(executor.clone());

            Ok(Box::new(App { executor, corodaw }))
        }),
    )
    .unwrap();

    println!("[main] exit");

    Ok(())
}
