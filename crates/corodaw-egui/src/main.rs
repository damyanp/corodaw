use std::{cell::RefCell, rc::Rc, time::Duration};

use eframe::{
    UserEvent,
    egui::{self, ComboBox},
};
use engine::{
    audio::Audio,
    audio_graph::{AudioGraph, audio_graph},
    plugins::{
        ClapPluginId, ClapPluginManager,
        discovery::{FoundPlugin, get_plugins},
    },
};
use smol::LocalExecutor;
use winit::event_loop::EventLoop;

use crate::module::Module;

mod module;

struct Corodaw {
    executor: Rc<LocalExecutor<'static>>,

    found_plugins: Vec<FoundPlugin>,
    manager: Rc<ClapPluginManager>,

    selected_plugin: Option<FoundPlugin>,

    modules: Rc<RefCell<Vec<Module>>>,
    counter: u32,

    audio_graph: Rc<RefCell<AudioGraph>>,
    _audio: Audio,
}

impl Corodaw {
    fn new(executor: Rc<LocalExecutor<'static>>) -> Self {
        let manager = Rc::new(ClapPluginManager::new());
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
                        self.add_module(ctx.clone());
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

    fn show_plugin_ui(&self, clap_plugin_id: ClapPluginId) {
        let manager = self.manager.clone();
        manager.show_gui(clap_plugin_id);
    }

    fn add_module(&mut self, ctx: egui::Context) {
        let found_plugin = self.selected_plugin.as_ref().unwrap().clone();
        let name = format!("Module {}: {}", self.counter, found_plugin.name);
        self.counter += 1;

        let modules = self.modules.clone();
        let audio_graph = self.audio_graph.clone();
        let manager = self.manager.clone();

        self.executor
            .spawn(async move {
                let module = Module::new(name, found_plugin, manager, audio_graph).await;
                modules.borrow_mut().push(module);
                ctx.request_repaint();
            })
            .detach();
    }
}

fn display_found_plugin(value: &Option<FoundPlugin>) -> &str {
    value
        .as_ref()
        .map(|plugin| plugin.name.as_str())
        .unwrap_or("<none>")
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();

    let executor = Rc::new(LocalExecutor::new());

    struct App {
        corodaw: Corodaw,
    }
    impl eframe::App for App {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            self.corodaw.update(ctx);
        }
    }

    let mut eventloop = EventLoop::<UserEvent>::with_user_event().build()?;

    let mut app = eframe::create_native(
        "Corodaw",
        options,
        Box::new(|_| {
            let corodaw = Corodaw::new(executor.clone());

            Ok(Box::new(App { corodaw }))
        }),
        &eventloop,
    );

    loop {
        while executor.try_tick() {}

        match app.pump_eframe_app(&mut eventloop, Some(Duration::from_millis(16))) {
            eframe::EframePumpStatus::Continue(_control_flow) => (),
            eframe::EframePumpStatus::Exit(_) => {
                break;
            }
        }
    }

    println!("[main] exit");

    Ok(())
}
