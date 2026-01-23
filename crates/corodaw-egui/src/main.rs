use std::{
    cell::{RefCell, RefMut},
    ops::DerefMut,
    rc::Rc,
    time::Duration,
};

use bevy_app::Update;
use bevy_ecs::{
    entity::Entity,
    query::Added,
    system::{NonSend, Query},
};
use corodaw_egui_widgets::arranger::ArrangerWidget;
use eframe::{
    UserEvent,
    egui::{self, ComboBox, Ui},
};
use engine::plugins::discovery::{FoundPlugin, get_plugins};
use project::{AddChannel, ChannelState};
use smol::{LocalExecutor, Task};
use winit::event_loop::EventLoop;

use crate::module::Module;

mod module;

struct Corodaw {
    app: Rc<RefCell<bevy_app::App>>,
    state: Rc<RefCell<CorodawState>>,
    found_plugins: Vec<FoundPlugin>,
    executor: LocalExecutor<'static>,
    current_task: Option<Task<()>>,
}

#[derive(Default)]
struct CorodawState {
    selected_plugin: Option<FoundPlugin>,
    modules: Vec<Module>,
}

impl Default for Corodaw {
    fn default() -> Self {
        let state: Rc<RefCell<CorodawState>> = Rc::default();

        let mut app = project::make_app();
        app.add_systems(Update, update_channels)
            .insert_non_send_resource(state.clone());

        Self {
            app: Rc::new(RefCell::new(app)),
            found_plugins: get_plugins(),
            executor: LocalExecutor::new(),
            state,
            current_task: None,
        }
    }
}

impl eframe::App for Corodaw {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.app.borrow_mut().update();
        while self.executor.try_tick() {}
        if let Some(task) = &self.current_task {
            if task.is_finished() {
                self.current_task = None;
            }
        }

        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            if self.current_task.is_some() {
                ui.disable();
            }
            egui::MenuBar::new().ui(ui, |ui| self.main_menu_bar(ui));
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.current_task.is_some() {
                ui.disable();
            }

            ui.horizontal(|ui| {
                let mut state = self.state.borrow_mut();
                let mut app = self.app.borrow_mut();

                ui.add_enabled_ui(state.selected_plugin.is_some(), |ui| {
                    if ui.button("Add Module").clicked() {
                        app.world_mut()
                            .trigger(AddChannel(state.selected_plugin.clone().unwrap()));
                    }
                });

                ComboBox::from_id_salt("Plugin")
                    .width(ui.available_width())
                    .selected_text(display_found_plugin(&state.selected_plugin).to_string())
                    .show_ui(ui, |ui| {
                        let mut selected_plugin = state.selected_plugin.clone();
                        for plugin in &self.found_plugins {
                            ui.selectable_value(
                                &mut selected_plugin,
                                Some(plugin.clone()),
                                plugin.name.to_owned(),
                            );
                        }
                        state.selected_plugin = selected_plugin;
                    });
            });

            self.state
                .borrow_mut()
                .add_modules(RefMut::deref_mut(&mut self.app.borrow_mut()), ui);

            ArrangerWidget::new("arranger").show(ui);
        });
    }
}

impl Corodaw {
    fn main_menu_bar(&mut self, ui: &mut Ui) {
        ui.menu_button("File", |ui| {
            if ui.button("Open...").clicked() {
                todo!()
            }
            if ui.button("Save...").clicked() {
                self.save();
            }
            ui.separator();
            if ui.button("Quit").clicked() {
                todo!();
            }
        });
    }

    fn save(&mut self) {
        assert!(self.current_task.is_none());

        self.current_task = Some(self.executor.spawn(async move {
            let file = rfd::AsyncFileDialog::new()
                .add_filter("Corodaw Project", &[".cod"])
                .save_file()
                .await;

            let filename = file
                .map(|fh| fh.file_name())
                .unwrap_or("nothing".to_owned());

            println!("Chose: {}", filename);
        }));
    }
}

impl CorodawState {
    fn add_modules(&mut self, app: &mut bevy_app::App, ui: &mut Ui) {
        for module in self.modules.iter() {
            module.add_to_ui(app, ui);
        }
    }
}

fn display_found_plugin(value: &Option<FoundPlugin>) -> &str {
    value
        .as_ref()
        .map(|plugin| plugin.name.as_str())
        .unwrap_or("<none>")
}

fn update_channels(
    corodaw_state: NonSend<Rc<RefCell<CorodawState>>>,
    new_channels: Query<(Entity, &ChannelState), Added<ChannelState>>,
) {
    let mut corodaw_state = corodaw_state.borrow_mut();

    for (entity, state) in new_channels {
        corodaw_state
            .modules
            .push(Module::new(entity, state.gain_value));
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();

    let mut eventloop = EventLoop::<UserEvent>::with_user_event().build()?;

    let mut app = eframe::create_native(
        "Corodaw",
        options,
        Box::new(|_| Ok(Box::new(Corodaw::default()))),
        &eventloop,
    );

    #[allow(clippy::while_let_loop)]
    loop {
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
