use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use eframe::egui::{self, ComboBox};
use engine::{
    audio::Audio,
    audio_graph::{AudioGraph, audio_graph},
    plugins::{
        ClapPlugin, ClapPluginManager,
        discovery::{FoundPlugin, get_plugins},
    },
};
use smol::LocalExecutor;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};

use crate::{app::App, module::Module, plugins::EguiClapPluginManager};

mod app;
mod module;
mod plugins;

struct Corodaw<'a> {
    this: Weak<RefCell<Self>>,

    executor: Rc<LocalExecutor<'a>>,
    #[allow(clippy::type_complexity)]
    pending_with_active_event_loop_fns: RefCell<Vec<Box<dyn FnOnce(&ActiveEventLoop) + 'a>>>,

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
            pending_with_active_event_loop_fns: RefCell::default(),
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
                                let mut this = clone.borrow_mut();
                                let manager = this.manager.inner.clone();
                                this.add_module(manager).await;
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

    async fn add_module(&mut self, manager: Rc<ClapPluginManager>) {
        let plugin = self.selected_plugin.as_ref().unwrap();

        let name = format!("Module {}: {}", self.counter, plugin.name);
        self.counter += 1;

        let module = Module::new(name, plugin, manager, self.audio_graph.clone()).await;

        self.modules.push(module);
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

    let eventloop = EventLoop::<eframe::UserEvent>::with_user_event()
        .build()
        .unwrap();
    eventloop.set_control_flow(ControlFlow::Wait);

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
