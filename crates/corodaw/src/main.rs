use std::{cell::RefCell, rc::Rc, time::Duration};

use audio_graph::StateReader;
use bevy_app::prelude::*;
use bevy_ecs::{prelude::*, system::RunSystemOnce};
use eframe::{
    UserEvent,
    egui::{self, Ui, vec2},
};
use project::{Project, SaveEvent};
use smol::{LocalExecutor, Task};
use winit::event_loop::EventLoop;

use crate::arranger::arranger_ui_system;

mod arranger;

struct Corodaw {
    app: Rc<RefCell<bevy_app::App>>,
    executor: LocalExecutor<'static>,
    current_task: Option<Task<()>>,
}

impl Default for Corodaw {
    fn default() -> Self {
        let mut app = project::make_app();
        app.add_systems(PostUpdate, set_titlebar_system);

        Self {
            app: Rc::new(RefCell::new(app)),
            executor: LocalExecutor::new(),
            current_task: None,
        }
    }
}

impl eframe::App for Corodaw {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.update_logic(ctx);

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

            let mut app = self.app.borrow_mut();
            let world = app.world_mut();

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

        while self.executor.try_tick() {}

        if let Some(task) = &self.current_task
            && task.is_finished()
        {
            self.current_task = None;
        }
    }

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

        let app = self.app.clone();
        let task = self.executor.spawn(async move {
            let file = rfd::AsyncFileDialog::new()
                .add_filter("Corodaw Project", &["corodaw"])
                .save_file()
                .await;

            if let Some(file) = file {
                app.borrow_mut().world_mut().trigger(SaveEvent::new(file));
            }
        });

        self.current_task = Some(task);
    }
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
