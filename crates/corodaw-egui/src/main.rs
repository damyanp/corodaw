use std::{cell::RefCell, rc::Rc, time::Duration};

use audio_graph::{AudioGraph, NodeId};
use eframe::{
    UserEvent,
    egui::{self, ComboBox},
};
use engine::{
    audio::Audio,
    builtin::Summer,
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

    summer: NodeId,

    _audio: Audio,
}

impl Corodaw {
    fn new(executor: Rc<LocalExecutor<'static>>) -> Self {
        let (audio_graph, audio_graph_worker) = AudioGraph::new();
        let audio = Audio::new(audio_graph_worker).unwrap();

        let summer = audio_graph.add_node(0, 2, Box::new(Summer));
        audio_graph.set_output_node(summer);

        let clap_plugin_manager = Rc::new(ClapPluginManager::new(audio_graph));

        Self {
            executor,
            found_plugins: get_plugins(),
            manager: clap_plugin_manager,
            selected_plugin: None,
            modules: Rc::default(),
            counter: 0,
            summer,
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
        let summer = self.summer;
        let clap_plugin_manager = self.manager.clone();

        self.executor
            .spawn(async move {
                let module = Module::new(name, clap_plugin_manager.clone(), &found_plugin).await;
                let mut modules = modules.borrow_mut();

                for port in 0..2 {
                    clap_plugin_manager.audio_graph.connect_grow_input(
                        summer,
                        modules.len() * 2 + port,
                        module.get_output_node(),
                        port,
                    );
                }
                modules.push(module);

                clap_plugin_manager.audio_graph.update();
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
