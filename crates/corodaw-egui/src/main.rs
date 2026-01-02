use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    time::Duration,
};

use clack_extensions::gui::GuiSize;
use eframe::{
    EframePumpStatus,
    egui::{self, Color32, ComboBox, Margin, Stroke, ViewportBuilder, ahash::HashMap},
};
use engine::plugins::{
    ClapPlugin, ClapPluginId, ClapPluginManager, GuiMessage, GuiMessagePayload,
    discovery::{FoundPlugin, get_plugins},
};
use futures::StreamExt;
use futures_channel::mpsc::{UnboundedReceiver, unbounded};
use smol::LocalExecutor;
use winit::event_loop::{ControlFlow, EventLoop};

struct EguiClapPluginManager {
    inner: Rc<ClapPluginManager>,
    guis: RefCell<HashMap<ClapPluginId, Rc<EguiPluginGui>>>,
}

impl EguiClapPluginManager {
    fn new(executor: &LocalExecutor) -> Rc<Self> {
        let (gui_sender, gui_receiver) = unbounded();

        let inner = ClapPluginManager::new(gui_sender);
        Self::spawn_message_handler(executor, Rc::downgrade(&inner));

        let manager = Rc::new(Self {
            inner,
            guis: RefCell::default(),
        });
        Self::spawn_gui_message_handler(executor, Rc::downgrade(&manager), gui_receiver);

        manager
    }

    fn spawn_message_handler(executor: &LocalExecutor, manager: Weak<ClapPluginManager>) {
        executor
            .spawn(async move {
                ClapPluginManager::message_handler(manager).await;
            })
            .detach();
    }

    fn spawn_gui_message_handler(
        executor: &LocalExecutor,
        manager: Weak<Self>,
        mut receiver: UnboundedReceiver<GuiMessage>,
    ) {
        executor
            .spawn(async move {
                println!("[gui_message_handler] start");
                while let Some(GuiMessage { plugin_id, payload }) = receiver.next().await {
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
                            plugin.request_resize(size);
                        }
                    }
                }
                println!("[gui_message_handler] end");
            })
            .detach();
    }
}

struct EguiPluginGui;

impl EguiPluginGui {
    fn request_resize(self: &Rc<EguiPluginGui>, size: GuiSize) {
        todo!();
    }
}

struct Corodaw<'a> {
    executor: LocalExecutor<'a>,
    found_plugins: Vec<Rc<FoundPlugin>>,
    state: Rc<RefCell<State>>,

    manager: Rc<EguiClapPluginManager>,
}

#[derive(Default)]
struct State {
    selected_plugin: Option<Rc<FoundPlugin>>,

    modules: Vec<Module>,
    counter: u32,
}

impl Corodaw<'_> {
    fn new() -> Self {
        let executor = LocalExecutor::new();
        let manager = EguiClapPluginManager::new(&executor);

        Self {
            executor,
            found_plugins: get_plugins(),
            state: Rc::default(),
            manager,
        }
    }
}

impl eframe::App for Corodaw<'_> {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        while self.executor.try_tick() {
            println!("Ticked!");
        }

        let mut state = self.state.borrow_mut();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add_enabled_ui(state.selected_plugin.is_some(), |ui| {
                    if ui.button("Add Module").clicked() {
                        let my_state = self.state.clone();
                        let manager = self.manager.inner.clone();
                        self.executor
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
    let options = eframe::NativeOptions::default();

    let mut eventloop = EventLoop::<eframe::UserEvent>::with_user_event()
        .build()
        .unwrap();
    eventloop.set_control_flow(ControlFlow::Poll);

    let mut app = eframe::create_native(
        "Corodaw",
        options,
        Box::new(|_| Ok(Box::new(Corodaw::new()))),
        &eventloop,
    );

    while let EframePumpStatus::Continue(cf) =
        app.pump_eframe_app(&mut eventloop, Some(Duration::ZERO))
    {
        println!("{:?}", cf);
    }

    Ok(())
}
