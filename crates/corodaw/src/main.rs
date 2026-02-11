use audio_graph::StateReader;
use bevy::prelude::*;
use bevy_app::AppExit;
use bevy_ecs::{
    message::MessageWriter,
    system::command,
    world::CommandQueue,
};
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};
use bevy_inspector_egui::bevy_inspector;
use egui::{Button, MenuBar, Ui};
use project::{CommandManager, LoadEvent, Project, SaveEvent, UndoRedoEvent};
use smol::{LocalExecutor, Task, future};

use crate::arranger::arranger_ui;

mod arranger;

#[derive(Default)]
struct Executor {
    executor: LocalExecutor<'static>,
    current_task: Option<Task<CommandQueue>>,
}

impl Executor {
    pub fn is_active(&self) -> bool {
        self.current_task.is_some()
    }

    pub fn spawn(&mut self, future: impl Future<Output = CommandQueue> + 'static) {
        assert!(self.current_task.is_none());
        self.current_task = Some(self.executor.spawn(future));
    }
}

fn update_executor_system(world: &mut World) {
    let mut executor = world.non_send_resource_mut::<Executor>();
    while executor.executor.try_tick() {}

    if let Some(task) = &mut executor.current_task
        && task.is_finished()
    {
        // I couldn't find a nice way to get it so these async tasks could
        // mutate the World. Instead, we allow them to build up a CommandQueue
        // that we can then apply when we're back into a Bevy system.
        let task = executor.current_task.take().unwrap();

        let mut c = future::block_on(task);
        c.apply(world);
    }
}

fn swap_buffers_system(mut state_reader: NonSendMut<StateReader>) {
    state_reader.swap_buffers();
}

#[derive(Resource, Default)]
struct InspectorEnabled(bool);

fn menu_bar_system(
    mut contexts: EguiContexts,
    executor: NonSend<Executor>,
    mut commands: Commands,
    command_manager: NonSendMut<CommandManager>,
    mut app_exit: MessageWriter<AppExit>,
    mut inspector_enabled: ResMut<InspectorEnabled>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    ctx.request_repaint();

    egui::TopBottomPanel::top("menu").show(ctx, |ui| {
        if executor.is_active() {
            ui.disable();
        }
        menu_bar_ui(
            ui,
            &mut commands,
            &command_manager,
            &mut app_exit,
            &mut inspector_enabled,
        );
    });

    Ok(())
}

fn menu_bar_ui(
    ui: &mut Ui,
    commands: &mut Commands,
    command_manager: &CommandManager,
    app_exit: &mut MessageWriter<AppExit>,
    inspector_enabled: &mut InspectorEnabled,
) {
    MenuBar::new().ui(ui, |ui| {
        ui.menu_button("File", |ui| {
            if ui.button("Open...").clicked() {
                commands.trigger(FileCommand::Open);
            }
            if ui.button("Save...").clicked() {
                commands.trigger(FileCommand::Save);
            }
            ui.separator();
            if ui.button("Quit").clicked() {
                app_exit.write(AppExit::Success);
            }
        });
        ui.menu_button("Edit", |ui| {
            if ui
                .add_enabled(command_manager.can_undo(), Button::new("Undo"))
                .clicked()
            {
                commands.trigger(UndoRedoEvent::Undo);
            }
            if ui
                .add_enabled(command_manager.can_redo(), Button::new("Redo"))
                .clicked()
            {
                commands.trigger(UndoRedoEvent::Redo);
            }
        });
        ui.menu_button("View", |ui| {
            if ui.checkbox(&mut inspector_enabled.0, "Inspector").clicked() {
                ui.close();
            }
        });
    });
}

fn arranger_panel_system(
    mut contexts: EguiContexts,
    executor: NonSend<Executor>,
    data: arranger::ArrangerData,
) -> Result {
    let ctx = contexts.ctx_mut()?;

    egui::CentralPanel::default().show(ctx, |ui| {
        if executor.is_active() {
            ui.disable();
        }
        arranger_ui(data, ui);
    });

    Ok(())
}

fn world_inspector_system(world: &mut World) {
    let enabled = world.resource::<InspectorEnabled>().0;
    if !enabled {
        return;
    }

    let egui_context = world
        .query_filtered::<&mut bevy_egui::EguiContext, With<bevy_egui::PrimaryEguiContext>>()
        .single(world);

    let Ok(egui_context) = egui_context else {
        return;
    };
    let mut egui_context = egui_context.clone();

    let mut open = true;
    egui::Window::new("World Inspector")
        .default_size((320.0, 160.0))
        .open(&mut open)
        .show(egui_context.get_mut(), |ui| {
            egui::ScrollArea::both().show(ui, |ui| {
                bevy_inspector::ui_for_world(world, ui);
                ui.allocate_space(ui.available_size());
            });
        });

    if !open {
        world.resource_mut::<InspectorEnabled>().0 = false;
    }
}

#[derive(Event, Clone, Copy)]
pub enum FileCommand {
    Open,
    Save,
}

fn on_file_command(command: On<FileCommand>, mut executor: NonSendMut<Executor>) {
    let command = *command;
    executor.spawn(async move {
        let mut command_queue = CommandQueue::default();

        match command {
            FileCommand::Open => {
                let file = rfd::AsyncFileDialog::new()
                    .add_filter("Corodaw Project", &["corodaw"])
                    .pick_file()
                    .await;

                if let Some(file) = file {
                    command_queue.push(command::trigger(LoadEvent::new(file)));
                }
            }
            FileCommand::Save => {
                let file = rfd::AsyncFileDialog::new()
                    .add_filter("Corodaw Project", &["corodaw"])
                    .save_file()
                    .await;

                if let Some(file) = file {
                    command_queue.push(command::trigger(SaveEvent::new(file)));
                }
            }
        }
        command_queue
    });
}

fn set_titlebar_system(
    mut window: Single<&mut Window>,
    project: Single<&Project, Changed<Project>>,
) {
    let project_name = if let Some(path) = &project.path {
        path.file_name().unwrap().to_str().unwrap()
    } else {
        "<new project>"
    };

    window.title = format!("Corodaw: {}", project_name);
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn main() {
    let mut app = project::make_app();

    app.add_plugins(
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Corodaw".into(),
                resolution: (800u32, 600u32).into(),
                ..default()
            }),
            ..default()
        }),
    );
    app.add_plugins(EguiPlugin::default());
    app.add_plugins(bevy_inspector_egui::DefaultInspectorConfigPlugin);
    app.init_resource::<InspectorEnabled>();

    app.add_systems(Startup, setup_camera);
    app.add_systems(First, update_executor_system);
    app.add_systems(
        EguiPrimaryContextPass,
        (
            menu_bar_system,
            swap_buffers_system,
            arranger_panel_system,
        )
            .chain(),
    );
    app.add_systems(EguiPrimaryContextPass, world_inspector_system);
    app.add_systems(PostUpdate, set_titlebar_system);
    app.insert_non_send_resource(Executor::default());
    app.add_observer(on_file_command);

    app.run();
}
