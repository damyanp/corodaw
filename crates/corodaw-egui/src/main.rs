use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use eframe::egui::{self, Color32, ComboBox, Margin, Stroke};
use engine::plugins::{
    ClapPlugin, ClapPluginManager,
    discovery::{FoundPlugin, get_plugins},
};
use futures_channel::mpsc::unbounded;
use smol::LocalExecutor;

struct Corodaw {
    found_plugins: Vec<Rc<FoundPlugin>>,
    state: Rc<RefCell<State>>,

    manager: Rc<ClapPluginManager>,
}

#[derive(Default)]
struct State {
    selected_plugin: Option<Rc<FoundPlugin>>,

    modules: Vec<Module>,
    counter: u32,
}

impl Corodaw {
    fn new(executor: &LocalExecutor) -> Self {
        let (gui_sender, _gui_receiver) = unbounded();

        let manager = ClapPluginManager::new(gui_sender);
        Corodaw::spawn_message_handler(executor, Rc::downgrade(&manager));

        Self {
            found_plugins: get_plugins(),
            state: Rc::default(),
            manager,
        }
    }

    fn spawn_message_handler(executor: &LocalExecutor, manager: Weak<ClapPluginManager>) {
        executor
            .spawn(async move {
                ClapPluginManager::message_handler(manager).await;
            })
            .detach();
    }

    fn update(
        &mut self,
        executor: &LocalExecutor,
        ctx: &egui::Context,
        _frame: &mut eframe::Frame,
    ) {
        while executor.try_tick() {
            println!("Ticked!");
        }

        let mut state = self.state.borrow_mut();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add_enabled_ui(state.selected_plugin.is_some(), |ui| {
                    if ui.button("Add Module").clicked() {
                        let my_state = self.state.clone();
                        let manager = self.manager.clone();
                        executor
                            .spawn(async move { my_state.borrow_mut().add_module(manager).await })
                            .detach();
                    }
                });
                ComboBox::from_id_salt("Plugin")
                    .width(ui.available_width())
                    .selected_text(format!("{}", display_found_plugin(&state.selected_plugin)))
                    .show_ui(ui, |ui| {
                        for plugin in &self.found_plugins {
                            ui.selectable_value(
                                &mut state.selected_plugin,
                                Some(plugin.clone()),
                                plugin.name.to_owned(),
                            );
                        }
                    });
            });
            for module in &state.modules {
                module.add_to_ui(ui);
            }
        });
    }
}

impl State {
    async fn add_module(&mut self, manager: Rc<ClapPluginManager>) {
        println!("State::add_module");

        let Some(plugin) = &self.selected_plugin else {
            return;
        };

        let name = format!("Module {}: {}", self.counter, plugin.name);
        self.counter += 1;

        self.modules.push(Module::new(name, plugin, manager).await);
    }
}

struct Module {
    name: String,
    _plugin: Rc<ClapPlugin>,
}

impl Module {
    async fn new(name: String, plugin: &FoundPlugin, manager: Rc<ClapPluginManager>) -> Self {
        let plugin = manager.create_plugin(plugin).await;
        Self {
            name,
            _plugin: plugin,
        }
    }

    fn add_to_ui(&self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .stroke(Stroke::new(1.0, Color32::WHITE))
            .inner_margin(Margin::same(5))
            .outer_margin(Margin::same(5))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(&self.name);
                    ui.take_available_space();
                    let _ = ui.button("Show");
                });
            });
    }
}

fn display_found_plugin(value: &Option<Rc<FoundPlugin>>) -> &str {
    value
        .as_ref()
        .map(|plugin| plugin.name.as_str())
        .unwrap_or("<none>")
}

fn main() -> eframe::Result {
    let executor = LocalExecutor::new();

    let mut corodaw = Corodaw::new(&executor);

    let options = eframe::NativeOptions::default();
    eframe::run_simple_native("Corodaw", options, move |ctx, frame| {
        corodaw.update(&executor, ctx, frame);
    })
}
