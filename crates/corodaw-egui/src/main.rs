use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use clack_extensions::gui::GuiSize;
use eframe::{
    UserEvent,
    egui::{self, Color32, ComboBox, Margin, Stroke, ahash::HashMap},
};
use engine::plugins::{
    ClapPlugin, ClapPluginId, ClapPluginManager, GuiMessage, GuiMessagePayload,
    discovery::{FoundPlugin, get_plugins},
};
use futures::StreamExt;
use futures_channel::mpsc::{UnboundedReceiver, unbounded};
use smol::LocalExecutor;
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::WindowId,
};

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
    fn request_resize(self: &Rc<EguiPluginGui>, _size: GuiSize) {
        todo!();
    }
}

struct Corodaw<'a> {
    executor: Rc<LocalExecutor<'a>>,
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

impl<'a> Corodaw<'a> {
    fn new(executor: Rc<LocalExecutor<'a>>) -> Self {
        let manager = EguiClapPluginManager::new(&executor);

        Self {
            executor,
            found_plugins: get_plugins(),
            state: Rc::default(),
            manager,
        }
    }

    fn update(&mut self, ctx: &egui::Context) {
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

struct App<'a, T> {
    _corodaw: Rc<RefCell<Corodaw<'a>>>,
    eframe: T,
}

impl<'a, T> App<'a, T> {
    fn new(corodaw: Rc<RefCell<Corodaw<'a>>>, eframe: T) -> Self {
        Self {
            _corodaw: corodaw,
            eframe,
        }
    }
}

impl<T> ApplicationHandler<UserEvent> for App<'_, T>
where
    T: ApplicationHandler<UserEvent>,
{
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        self.eframe.new_events(event_loop, cause);
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.eframe.resumed(event_loop);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        self.eframe.user_event(event_loop, event);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        self.eframe.window_event(event_loop, window_id, event);
    }

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        device_id: DeviceId,
        event: DeviceEvent,
    ) {
        self.eframe.device_event(event_loop, device_id, event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.eframe.about_to_wait(event_loop);
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.eframe.suspended(event_loop);
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        self.eframe.exiting(event_loop);
    }

    fn memory_warning(&mut self, event_loop: &ActiveEventLoop) {
        self.eframe.memory_warning(event_loop);
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();

    let eventloop = EventLoop::<eframe::UserEvent>::with_user_event()
        .build()
        .unwrap();
    eventloop.set_control_flow(ControlFlow::Poll);

    let executor = Rc::new(LocalExecutor::new());
    let corodaw = Rc::new(RefCell::new(Corodaw::new(executor.clone())));

    struct AppProxy<'a> {
        corodaw: Rc<RefCell<Corodaw<'a>>>,
    }
    impl eframe::App for AppProxy<'_> {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            self.corodaw.borrow_mut().update(ctx);
        }
    }

    let corodaw_for_proxy = corodaw.clone();
    let eframe = eframe::create_native(
        "Corodaw",
        options,
        Box::new(|_| {
            Ok(Box::new(AppProxy {
                corodaw: corodaw_for_proxy,
            }))
        }),
        &eventloop,
    );

    let mut app = App::new(corodaw, eframe);

    eventloop.run_app(&mut app)?;

    Ok(())
}
