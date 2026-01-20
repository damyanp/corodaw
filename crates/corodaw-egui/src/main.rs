use std::{cell::RefCell, rc::Rc, time::Duration};

use bevy_ecs::{
    entity::Entity,
    query::Added,
    system::{NonSend, Query},
    world::Mut,
};
use eframe::{
    UserEvent,
    egui::{self, ComboBox},
};
use engine::plugins::discovery::{FoundPlugin, get_plugins};
use project::{ChannelState, Project};
use winit::event_loop::EventLoop;

use crate::module::Module;

mod module;

struct Corodaw {
    project: Project,
    found_plugins: Vec<FoundPlugin>,
}

#[derive(Default)]
struct CorodawState {
    selected_plugin: Option<FoundPlugin>,
    modules: Rc<RefCell<Vec<Module>>>,
}

impl Default for Corodaw {
    fn default() -> Self {
        let mut project = Project::default();
        project.add_systems(update_channels);
        project
            .get_world_mut()
            .insert_non_send_resource(CorodawState::default());

        Self {
            found_plugins: get_plugins(),
            project,
        }
    }
}

impl Corodaw {
    fn state(&self) -> &CorodawState {
        self.project
            .get_world()
            .get_non_send_resource::<CorodawState>()
            .unwrap()
    }

    fn state_mut(&mut self) -> Mut<'_, CorodawState> {
        self.project
            .get_world_mut()
            .get_non_send_resource_mut()
            .unwrap()
    }

    fn add_module(&mut self) {
        let found_plugin = self.state().selected_plugin.clone().unwrap();
        self.project.add_channel(&found_plugin);
    }
}

impl eframe::App for Corodaw {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.project.update();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add_enabled_ui(self.state().selected_plugin.is_some(), |ui| {
                    if ui.button("Add Module").clicked() {
                        self.add_module();
                    }
                });
                ComboBox::from_id_salt("Plugin")
                    .width(ui.available_width())
                    .selected_text(display_found_plugin(&self.state().selected_plugin).to_string())
                    .show_ui(ui, |ui| {
                        let mut selected_plugin = self.state().selected_plugin.clone();
                        for plugin in &self.found_plugins {
                            ui.selectable_value(
                                &mut selected_plugin,
                                Some(plugin.clone()),
                                plugin.name.to_owned(),
                            );
                        }
                        self.state_mut().selected_plugin = selected_plugin;
                    });
            });

            let modules = self.state().modules.clone();
            let modules = modules.borrow();

            for module in modules.iter() {
                module.add_to_ui(&mut self.project, ui);
            }
        });
    }
}

fn display_found_plugin(value: &Option<FoundPlugin>) -> &str {
    value
        .as_ref()
        .map(|plugin| plugin.name.as_str())
        .unwrap_or("<none>")
}

fn update_channels(
    corodaw_state: NonSend<CorodawState>,
    new_channels: Query<(Entity, &ChannelState), Added<ChannelState>>,
) {
    let mut modules = corodaw_state.modules.borrow_mut();

    for (entity, state) in new_channels {
        modules.push(Module::new(entity, state.gain_value));
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
