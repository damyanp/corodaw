use corodaw_widgets::arranger::{ArrangerDataProvider, ArrangerWidget};
use eframe::egui::{self, Align2, CollapsingHeader, Color32, FontId, Ui, Vec2};

fn main() {
    let mut native_options = eframe::NativeOptions::default();
    native_options.viewport = native_options
        .viewport
        .with_inner_size(Vec2::new(800.0, 600.0));

    let _ = eframe::run_native(
        "Corodaw widgets",
        native_options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    );
}

#[derive(Default)]
struct App {
    channels: Vec<usize>,
}

impl App {
    fn new(_: &eframe::CreationContext<'_>) -> Self {
        let channels = vec![0, 1];
        Self { channels }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Corodaw widgets");

            CollapsingHeader::new("Arranger")
                .default_open(true)
                .show(ui, |ui| {
                    struct TestArranger<'a>(&'a mut App);
                    impl<'a> ArrangerDataProvider for TestArranger<'a> {
                        fn num_channels(&self) -> usize {
                            self.0.channels.len()
                        }

                        fn channel_height(&self, _: usize) -> f32 {
                            100.0
                        }

                        fn show_channel(&mut self, index: usize, ui: &mut Ui) {
                            let index = self.0.channels[index];

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

                        fn on_add_channel(&mut self, index: usize) {
                            self.0.channels.insert(index, self.0.channels.len());
                        }

                        fn move_channel(&mut self, index: usize, destination: usize) {
                            let channel = self.0.channels.remove(index);
                            let destination = if destination > index {
                                destination - 1
                            } else {
                                destination
                            };
                            self.0.channels.insert(destination, channel);
                        }
                    }

                    ArrangerWidget::new("arranger").show(TestArranger(self), ui);
                });
        });
    }
}
