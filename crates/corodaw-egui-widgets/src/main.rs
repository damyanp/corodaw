use corodaw_egui_widgets::arranger::{ArrangerDataProvider, ArrangerWidget};
use eframe::egui::{self, Align2, CollapsingHeader, Color32, FontId, Ui, Vec2};

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
                    struct TestArranger;
                    impl ArrangerDataProvider for TestArranger {
                        fn num_channels(&self) -> usize {
                            10
                        }

                        fn channel_height(&self, _: usize) -> f32 {
                            100.0
                        }

                        fn show_channel(&mut self, index: usize, ui: &mut Ui) {
                            let r = ui.available_rect_before_wrap();
                            let p = ui.painter();
                            p.rect_filled(r, 2.0, Color32::DARK_BLUE);

                            p.text(
                                r.center(),
                                Align2::CENTER_CENTER,
                                format!("Channel {index}"),
                                FontId::default(),
                                ui.style().visuals.text_color(),
                            );

                            p.text(
                                r.left_top(),
                                Align2::LEFT_TOP,
                                format!("{},{}", r.left(), r.top()),
                                FontId::default(),
                                ui.style().visuals.text_color(),
                            );
                        }

                        fn show_strip(&mut self, _: usize, ui: &mut Ui) {
                            let r = ui.available_rect_before_wrap();
                            let p = ui.painter();
                            p.rect_filled(r, 2.0, Color32::DARK_RED);
                        }
                    }

                    ArrangerWidget::new("arranger").show(TestArranger, ui);
                });
        });
    }
}
