use audio_graph::{
    GraphConnection, GraphNodeDesc, GraphOutputNode, GraphPlugin, GraphPorts, GraphStateReader,
};
use bevy::prelude::*;
use bevy_app::AppExit;
use bevy_ecs::{message::MessageWriter, system::command, world::CommandQueue};
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};
use bevy_inspector_egui::bevy_inspector;
use egui::{Button, KeyboardShortcut, MenuBar, Modifiers, Ui};
use engine::{
    audio::AudioOutput,
    builtin::{MidiInputOwner, SummerOwner},
    plugins::ClapManager,
};
use project::{
    AvailablePlugin, ChannelGain, ChannelMixerState, ChannelOrder, ChannelPlugin,
    ChannelPluginBinding, EditHistory, EditHistoryPlugin, LoadEvent, ProjectInfo, ProjectPlugin,
    SaveEvent, StableId, UndoRedoEvent, add_available_plugins,
};
use smol::{LocalExecutor, Task, future};

use crate::arranger::arranger_ui;

mod arranger;

#[derive(Default)]
struct AsyncTaskRunner {
    executor: LocalExecutor<'static>,
    current_task: Option<Task<CommandQueue>>,
}

impl AsyncTaskRunner {
    pub fn is_active(&self) -> bool {
        self.current_task.is_some()
    }

    pub fn spawn(&mut self, future: impl Future<Output = CommandQueue> + 'static) {
        assert!(self.current_task.is_none());
        self.current_task = Some(self.executor.spawn(future));
    }
}

fn update_executor_system(world: &mut World) {
    let mut async_task_runner = world.non_send_resource_mut::<AsyncTaskRunner>();
    while async_task_runner.executor.try_tick() {}

    if let Some(task) = &mut async_task_runner.current_task
        && task.is_finished()
    {
        // I couldn't find a nice way to get it so these async tasks could
        // mutate the World. Instead, we allow them to build up a CommandQueue
        // that we can then apply when we're back into a Bevy system.
        let task = async_task_runner.current_task.take().unwrap();

        let mut c = future::block_on(task);
        c.apply(world);
    }
}

fn swap_buffers_system(mut state_reader: NonSendMut<GraphStateReader>) {
    state_reader.swap_buffers();
}

#[derive(Resource, Default)]
struct InspectorEnabled(bool);

fn menu_bar_system(
    mut contexts: EguiContexts,
    async_task_runner: NonSend<AsyncTaskRunner>,
    mut commands: Commands,
    command_manager: NonSendMut<EditHistory>,
    mut app_exit: MessageWriter<AppExit>,
    mut inspector_enabled: ResMut<InspectorEnabled>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    ctx.request_repaint();

    let undo_shortcut = KeyboardShortcut::new(Modifiers::CTRL, egui::Key::Z);
    let redo_shortcut = KeyboardShortcut::new(Modifiers::CTRL, egui::Key::Y);

    if !async_task_runner.is_active() {
        if command_manager.can_undo() && ctx.input_mut(|i| i.consume_shortcut(&undo_shortcut)) {
            commands.trigger(UndoRedoEvent::Undo);
        }
        if command_manager.can_redo() && ctx.input_mut(|i| i.consume_shortcut(&redo_shortcut)) {
            commands.trigger(UndoRedoEvent::Redo);
        }
    }

    egui::TopBottomPanel::top("menu").show(ctx, |ui| {
        if async_task_runner.is_active() {
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
    command_manager: &EditHistory,
    app_exit: &mut MessageWriter<AppExit>,
    inspector_enabled: &mut InspectorEnabled,
) {
    MenuBar::new().ui(ui, |ui| {
        ui.menu_button("File", |ui| {
            if ui.button("Open...").clicked() {
                commands.trigger(FileAction::Open);
            }
            if ui.button("Save...").clicked() {
                commands.trigger(FileAction::Save);
            }
            ui.separator();
            if ui.button("Quit").clicked() {
                app_exit.write(AppExit::Success);
            }
        });
        ui.menu_button("Edit", |ui| {
            if ui
                .add_enabled(
                    command_manager.can_undo(),
                    Button::new("Undo").shortcut_text("Ctrl+Z"),
                )
                .clicked()
            {
                commands.trigger(UndoRedoEvent::Undo);
            }
            if ui
                .add_enabled(
                    command_manager.can_redo(),
                    Button::new("Redo").shortcut_text("Ctrl+Y"),
                )
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
    async_task_runner: NonSend<AsyncTaskRunner>,
    data: arranger::ArrangerData,
) -> Result {
    let ctx = contexts.ctx_mut()?;

    egui::CentralPanel::default().show(ctx, |ui| {
        if async_task_runner.is_active() {
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
pub enum FileAction {
    Open,
    Save,
}

fn on_file_action(command: On<FileAction>, mut async_task_runner: NonSendMut<AsyncTaskRunner>) {
    let command = *command;
    async_task_runner.spawn(async move {
        let mut command_queue = CommandQueue::default();

        match command {
            FileAction::Open => {
                let file = rfd::AsyncFileDialog::new()
                    .add_filter("Corodaw Project", &["corodaw"])
                    .pick_file()
                    .await;

                if let Some(file) = file {
                    command_queue.push(command::trigger(LoadEvent::new(file)));
                }
            }
            FileAction::Save => {
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
    project: Single<&ProjectInfo, Changed<ProjectInfo>>,
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
    let mut app = App::new();

    app.add_plugins((GraphPlugin, ProjectPlugin::new()));

    let audio_graph_worker = app.world_mut().remove_non_send_resource().unwrap();
    let audio = AudioOutput::new(audio_graph_worker).unwrap();

    let midi_input = MidiInputOwner::new(app.world_mut());
    let summer = SummerOwner::new(app.world_mut(), 2);
    app.world_mut()
        .entity_mut(summer.entity)
        .insert(GraphOutputNode);

    app.insert_non_send_resource(ClapManager::default())
        .insert_non_send_resource(midi_input)
        .insert_non_send_resource(summer)
        .insert_non_send_resource(audio)
        .add_plugins((ChannelPlugin::new(), EditHistoryPlugin));

    // Register types for bevy-inspector-egui
    app.register_type::<StableId>()
        .register_type::<ProjectInfo>()
        .register_type::<ChannelOrder>()
        .register_type::<ChannelPluginBinding>()
        .register_type::<ChannelMixerState>()
        .register_type::<ChannelGain>()
        .register_type::<AvailablePlugin>()
        .register_type::<GraphOutputNode>()
        .register_type::<GraphNodeDesc>()
        .register_type::<GraphConnection>()
        .register_type::<GraphPorts>();

    add_available_plugins(app.world_mut());

    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Corodaw".into(),
            resolution: (800u32, 600u32).into(),
            ..default()
        }),
        ..default()
    }));
    app.add_plugins(EguiPlugin::default());
    app.add_plugins(bevy_inspector_egui::DefaultInspectorConfigPlugin);
    app.init_resource::<InspectorEnabled>();

    app.add_systems(Startup, setup_camera);
    app.add_systems(First, update_executor_system);
    app.add_systems(
        EguiPrimaryContextPass,
        (menu_bar_system, swap_buffers_system, arranger_panel_system).chain(),
    );
    app.add_systems(EguiPrimaryContextPass, world_inspector_system);
    app.add_systems(PostUpdate, set_titlebar_system);
    app.insert_non_send_resource(AsyncTaskRunner::default());
    app.add_observer(on_file_action);

    app.run();
}
