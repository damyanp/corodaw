use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use clack_extensions::gui::{GuiApiType, GuiConfiguration, GuiSize, PluginGui};
use eframe::{
    EframeWinitApplication, UserEvent,
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
    dpi::PhysicalSize,
    event::{DeviceEvent, DeviceId, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

struct EguiClapPluginManager {
    inner: Rc<ClapPluginManager>,
    guis: RefCell<HashMap<ClapPluginId, Rc<EguiPluginGui>>>,
    windows: RefCell<HashMap<WindowId, ClapPluginId>>,
}

impl EguiClapPluginManager {
    fn new(executor: &LocalExecutor) -> Rc<Self> {
        let (gui_sender, gui_receiver) = unbounded();

        let inner = ClapPluginManager::new(gui_sender);
        Self::spawn_message_handler(executor, Rc::downgrade(&inner));

        let manager = Rc::new(Self {
            inner,
            guis: RefCell::default(),
            windows: RefCell::default(),
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

    fn show_plugin_gui(&self, event_loop: &ActiveEventLoop, clap_plugin: Rc<ClapPlugin>) {
        let mut guis = self.guis.borrow_mut();

        let plugin_id = clap_plugin.get_id();

        if guis.contains_key(&plugin_id) {
            println!("Asked to show a plugin that is already shown!");
            return;
        }

        let mut plugin = clap_plugin.plugin.borrow_mut();
        let mut plugin_handle = plugin.plugin_handle();

        let Some(plugin_gui) = plugin_handle.get_extension::<PluginGui>() else {
            println!("No GUI for plugin!");
            return;
        };

        let config = GuiConfiguration {
            api_type: GuiApiType::default_for_current_platform()
                .expect("This platform supports UI"),
            is_floating: false,
        };

        if !plugin_gui.is_api_supported(&mut plugin_handle, config) {
            println!("Plugin doesn't support API");
            return;
        }

        plugin_gui
            .create(&mut plugin_handle, config)
            .expect("create succeeds");

        let initial_size = plugin_gui.get_size(&mut plugin_handle).unwrap_or(GuiSize {
            width: 800,
            height: 600,
        });

        let is_resizeable = plugin_gui
            .get_resize_hints(&mut plugin_handle)
            .map(|h| h.can_resize_horizontally && h.can_resize_vertically)
            .unwrap_or(false);

        let window = event_loop
            .create_window(
                Window::default_attributes()
                    .with_inner_size(PhysicalSize {
                        width: initial_size.width,
                        height: initial_size.height,
                    })
                    .with_resizable(is_resizeable),
            )
            .expect("Window creation to succeed");

        unsafe {
            let window = clack_extensions::gui::Window::from_window(&window).unwrap();
            plugin_gui
                .set_parent(&mut plugin_handle, window)
                .expect("set_parent succeeds");
        }

        drop(plugin);

        let window_id = window.id();
        let gui = Rc::new(EguiPluginGui {
            clap_plugin,
            plugin_gui,
            window,
        });

        guis.insert(plugin_id, gui);
        self.windows.borrow_mut().insert(window_id, plugin_id);
    }

    fn window_event(&self, window_id: WindowId, event: &WindowEvent) -> bool {
        let mut windows = self.windows.borrow_mut();

        if let Some(id) = windows.get(&window_id) {
            match event {
                WindowEvent::CloseRequested => {
                    self.guis.borrow_mut().remove(id);
                    windows.remove(&window_id);
                }
                _ => (),
            }
            return true;
        }
        false
    }
}

struct EguiPluginGui {
    clap_plugin: Rc<ClapPlugin>,
    plugin_gui: PluginGui,
    window: Window,
}

impl Drop for EguiPluginGui {
    fn drop(&mut self) {
        self.plugin_gui
            .destroy(&mut self.clap_plugin.plugin.borrow_mut().plugin_handle());
    }
}

impl EguiPluginGui {
    fn request_resize(self: &Rc<EguiPluginGui>, size: GuiSize) {
        let _ = self.window.request_inner_size(PhysicalSize {
            width: size.width,
            height: size.height,
        });
    }
}

struct Corodaw<'a> {
    this: Weak<RefCell<Self>>,
    executor: Rc<LocalExecutor<'a>>,
    found_plugins: Vec<Rc<FoundPlugin>>,
    state: State,
    manager: Rc<EguiClapPluginManager>,

    #[allow(clippy::type_complexity)]
    pending_with_active_event_loop_fns: RefCell<Vec<Box<dyn FnOnce(&ActiveEventLoop) + 'a>>>,
}

#[derive(Default)]
struct State {
    selected_plugin: Option<Rc<FoundPlugin>>,

    modules: Vec<Module>,
    counter: u32,
}

impl<'a> Corodaw<'a> {
    fn new(executor: Rc<LocalExecutor<'a>>) -> Rc<RefCell<Self>> {
        let manager = EguiClapPluginManager::new(&executor);

        let r = Rc::new(RefCell::new(Self {
            this: Weak::default(),
            executor,
            found_plugins: get_plugins(),
            state: State::default(),
            manager,
            pending_with_active_event_loop_fns: RefCell::default(),
        }));

        r.borrow_mut().this = Rc::downgrade(&r);

        r
    }

    fn update(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add_enabled_ui(self.state.selected_plugin.is_some(), |ui| {
                    if ui.button("Add Module").clicked() {
                        let clone = self.this.upgrade().unwrap();
                        self.executor
                            .spawn(async move {
                                let mut this = clone.borrow_mut();
                                let manager = this.manager.inner.clone();
                                this.state.add_module(manager).await;
                            })
                            .detach();
                    }
                });
                ComboBox::from_id_salt("Plugin")
                    .width(ui.available_width())
                    .selected_text(display_found_plugin(&self.state.selected_plugin).to_string())
                    .show_ui(ui, |ui| {
                        for plugin in &self.found_plugins {
                            ui.selectable_value(
                                &mut self.state.selected_plugin,
                                Some(plugin.clone()),
                                plugin.name.to_owned(),
                            );
                        }
                    });
            });
            for module in &self.state.modules {
                module.add_to_ui(self, ui);
            }
        });
    }

    fn show_plugin_ui(&self, plugin: Rc<ClapPlugin>) {
        let this = self.this.upgrade().unwrap();

        self.run_with_active_event_loop(move |event_loop: &ActiveEventLoop| {
            this.borrow().manager.show_plugin_gui(event_loop, plugin);
        });
    }

    fn run_with_active_event_loop<Fn>(&self, f: Fn)
    where
        Fn: FnOnce(&ActiveEventLoop) + 'a,
    {
        self.pending_with_active_event_loop_fns
            .borrow_mut()
            .push(Box::new(f));
    }
}

impl State {
    async fn add_module(&mut self, manager: Rc<ClapPluginManager>) {
        let plugin = self.selected_plugin.as_ref().unwrap();

        let name = format!("Module {}: {}", self.counter, plugin.name);
        self.counter += 1;

        let module = Module::new(name, plugin, manager).await;

        self.modules.push(module);
    }
}

struct Module {
    name: String,
    plugin: Rc<ClapPlugin>,
}

impl Module {
    async fn new(name: String, plugin: &FoundPlugin, manager: Rc<ClapPluginManager>) -> Self {
        let plugin = manager.create_plugin(plugin).await;
        Self { name, plugin }
    }

    fn add_to_ui(&self, corodaw: &Corodaw, ui: &mut egui::Ui) {
        egui::Frame::new()
            .stroke(Stroke::new(1.0, Color32::WHITE))
            .inner_margin(Margin::same(5))
            .outer_margin(Margin::same(5))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(&self.name);
                    ui.take_available_space();
                    if ui.button("Show").clicked() {
                        corodaw.show_plugin_ui(self.plugin.clone());
                    }
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

struct App<'a> {
    executor: Rc<LocalExecutor<'a>>,
    corodaw: Rc<RefCell<Corodaw<'a>>>,
    eframe: EframeWinitApplication<'a>,
}

impl<'a> App<'a> {
    fn new(
        executor: Rc<LocalExecutor<'a>>,
        corodaw: Rc<RefCell<Corodaw<'a>>>,
        eframe: EframeWinitApplication<'a>,
    ) -> Self {
        Self {
            executor,
            corodaw,
            eframe,
        }
    }
}

impl ApplicationHandler<UserEvent> for App<'_> {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        for f in self
            .corodaw
            .borrow()
            .pending_with_active_event_loop_fns
            .replace(Vec::default())
        {
            f(event_loop);
        }

        while self.executor.try_tick() {}

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
        if self
            .corodaw
            .borrow()
            .manager
            .window_event(window_id, &event)
        {
            return;
        }

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
    let corodaw = Corodaw::new(executor.clone());

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

    let mut app = App::new(executor, corodaw, eframe);

    eventloop.run_app(&mut app)?;

    println!("[main] exit");

    Ok(())
}
