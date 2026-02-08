use std::{cell::RefCell, rc::Rc, time::Duration};

use audio_graph::StateReader;
use bevy_app::prelude::*;
use bevy_ecs::{
    prelude::*,
    system::{RunSystemOnce, command},
    world::CommandQueue,
};
use eframe::{
    UserEvent,
    egui::{self, Button, MenuBar, Ui, vec2},
};
use project::{CommandManager, LoadEvent, Project, SaveEvent};
use smol::{LocalExecutor, Task, future};
use winit::event_loop::EventLoop;

use crate::arranger::arranger_ui_system;

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

struct Corodaw {
    app: Rc<RefCell<bevy_app::App>>,
}

impl Default for Corodaw {
    fn default() -> Self {
        let mut app = project::make_app();
        app.add_systems(First, update_executor_system);
        app.add_systems(PostUpdate, set_titlebar_system);
        app.insert_non_send_resource(Executor::default());
        app.add_observer(on_menu_command);

        Self {
            app: Rc::new(RefCell::new(app)),
        }
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

impl eframe::App for Corodaw {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.update_logic(ctx);

        let mut app = self.app.borrow_mut();
        let world = app.world_mut();

        let executor: &Executor = world.non_send_resource();
        let executor_is_active = executor.is_active();

        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            if executor_is_active {
                ui.disable();
            }
            world
                .run_system_once_with(Self::menu_bar_system, ui)
                .unwrap();
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if executor_is_active {
                ui.disable();
            }

            world.non_send_resource_mut::<StateReader>().swap_buffers();
            world.run_system_once_with(arranger_ui_system, ui).unwrap();
        });
    }
}

impl Corodaw {
    fn update_logic(&mut self, ctx: &egui::Context) {
        ctx.request_repaint(); // keep repainting so we keep updating logic

        self.app.borrow_mut().insert_non_send_resource(ctx.clone());
        self.app.borrow_mut().update();
        self.app
            .borrow_mut()
            .world_mut()
            .remove_non_send_resource::<egui::Context>();
    }

    fn menu_bar_system(
        mut ui: InMut<Ui>,
        mut commands: Commands,
        command_manager: Single<&CommandManager>,
    ) {
        MenuBar::new().ui(&mut ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open...").clicked() {
                    commands.trigger(MenuCommand::Open);
                }
                if ui.button("Save...").clicked() {
                    commands.trigger(MenuCommand::Open);
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui
                    .add_enabled(command_manager.can_undo(), Button::new("Undo"))
                    .clicked()
                {
                    // TOOD: undo
                }
                if ui
                    .add_enabled(command_manager.can_redo(), Button::new("Redo"))
                    .clicked()
                {
                    // TODO: redo
                }
            });
        });
    }
}

#[derive(Event, Clone, Copy)]
pub enum MenuCommand {
    Open,
    Save,
}

fn on_menu_command(command: On<MenuCommand>, mut executor: NonSendMut<Executor>) {
    let command = *command;
    executor.spawn(async move {
        let mut command_queue = CommandQueue::default();

        match command {
            MenuCommand::Open => {
                let file = rfd::AsyncFileDialog::new()
                    .add_filter("Corodaw Project", &["corodaw"])
                    .pick_file()
                    .await;

                if let Some(file) = file {
                    command_queue.push(command::trigger(LoadEvent::new(file)));
                }
            }

            MenuCommand::Save => {
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

fn set_titlebar_system(ctx: NonSend<egui::Context>, project: Single<&Project, Changed<Project>>) {
    let project_name = if let Some(path) = &project.path {
        path.file_name().unwrap().to_str().unwrap()
    } else {
        "<new project>"
    };

    ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
        "Corodaw: {}",
        project_name
    )));
}

fn main() -> eframe::Result {
    let mut native_options = eframe::NativeOptions::default();
    native_options.viewport = native_options.viewport.with_inner_size(vec2(800.0, 600.0));

    let mut eventloop = EventLoop::<UserEvent>::with_user_event().build()?;

    let mut app = eframe::create_native(
        "Corodaw",
        native_options,
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
