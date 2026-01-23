use corodaw_egui_widgets::arranger::ArrangerWidget;
use eframe::egui::{self, CollapsingHeader, Vec2};

fn main() {
    let mut native_options = eframe::NativeOptions::default();
    native_options.viewport = native_options
        .viewport
        .with_inner_size(Vec2::new(800.0, 600.0));

    let _ = eframe::run_native(
        "Corodaw egui widgets",
        native_options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    );
}

#[derive(Default)]
struct App {}

impl App {
    fn new(_: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Corodaw egui widgets");

            CollapsingHeader::new("Arranger")
                .default_open(true)
                .show(ui, |ui| {
                    ArrangerWidget::new("arranger").show(ui);
                });
        });
    }
}
