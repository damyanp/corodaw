use std::{cell::RefCell, rc::Rc, time::Duration};

use bevy_ecs::{prelude::*, system::RunSystemOnce};
use eframe::{
    UserEvent,
    egui::{self, ComboBox, Ui},
};
use engine::plugins::discovery::{FoundPlugin, get_plugins};
use project::{AddChannel, ChannelState};
use smol::{LocalExecutor, Task};
use winit::event_loop::EventLoop;

use crate::arranger::arranger_ui;

mod arranger;

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
}

impl Default for Corodaw {
    fn default() -> Self {
        let state: Rc<RefCell<CorodawState>> = Rc::default();

        let mut app = project::make_app();
        app.insert_non_send_resource(state.clone());

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

            self.app
                .borrow_mut()
                .world_mut()
                .run_system_once_with(arranger_ui, ui)
                .unwrap();
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

fn display_found_plugin(value: &Option<FoundPlugin>) -> &str {
    value
        .as_ref()
        .map(|plugin| plugin.name.as_str())
        .unwrap_or("<none>")
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
