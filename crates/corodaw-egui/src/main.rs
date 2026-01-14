use std::{cell::RefCell, rc::Rc, time::Duration};

use eframe::{
    UserEvent,
    egui::{self, ComboBox},
};
use engine::plugins::discovery::{FoundPlugin, get_plugins};
use project::*;
use smol::LocalExecutor;
use winit::event_loop::EventLoop;

use crate::module::Module;

mod module;

struct Corodaw {
    executor: Rc<LocalExecutor<'static>>,

    found_plugins: Vec<FoundPlugin>,

    selected_plugin: Option<FoundPlugin>,

    project: Rc<RefCell<model::Project>>,
    modules: Rc<RefCell<Vec<Module>>>,
}

impl Corodaw {
    fn new(executor: Rc<LocalExecutor<'static>>) -> Self {
        Self {
            executor,
            found_plugins: get_plugins(),
            selected_plugin: None,
            project: Rc::default(),
            modules: Rc::default(),
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

    fn add_module(&mut self, ctx: egui::Context) {
        let found_plugin = self.selected_plugin.as_ref().unwrap().clone();

        let name = format!(
            "Module {}: {}",
            self.modules.borrow().len() + 1,
            found_plugin.name
        );
        let modules = self.modules.clone();
        let project = self.project.clone();

        self.executor
            .spawn(async move {
                let audio_graph = project.borrow().audio_graph();
                let clap_plugin_manager = project.borrow().clap_plugin_manager();
                let initial_gain = 1.0;
                let module = model::Module::new(
                    name,
                    &audio_graph,
                    &clap_plugin_manager,
                    &found_plugin,
                    initial_gain,
                )
                .await;
                let module_id = project.borrow_mut().add_module(module);

                let module = Module::new(module_id, initial_gain);
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
