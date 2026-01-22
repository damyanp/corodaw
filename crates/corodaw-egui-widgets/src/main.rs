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
                    arranger::ArrangerWidget::default().show(ctx, ui);
                });
        });
    }
}

mod arranger {
    use eframe::egui::{
        Align2, CentralPanel, Color32, Context, FontId, Pos2, Rect, SidePanel, Style, TextStyle,
        Ui, Vec2, panel::Side,
    };

    #[derive(Default)]
    pub struct ArrangerWidget {}

    impl ArrangerWidget {
        pub fn show(self, _: &Context, ui: &mut Ui) {
            let style = Style::default();

            let timestrip_height = ui.text_style_height(&TextStyle::Heading);
            let gap = 5.0;

            SidePanel::new(Side::Left, "channel_column")
                .default_width(300.0)
                .resizable(true)
                .show_inside(ui, |ui| {
                    let channel_column = Rect::from_min_max(
                        ui.clip_rect().min + Vec2::new(0.0, timestrip_height + gap),
                        ui.clip_rect().max,
                    );

                    let p = ui.painter();
                    p.rect_filled(channel_column, 0.0, Color32::GREEN);
                    p.text(
                        channel_column.center(),
                        Align2::CENTER_CENTER,
                        "Channel",
                        FontId::default(),
                        Color32::BLACK,
                    );

                    ui.take_available_space();
                });

            CentralPanel::default().show_inside(ui, |ui| {
                let rect = ui.clip_rect();

                let timestrip = Rect::from_min_max(
                    rect.min,
                    Pos2::new(rect.max.x, rect.min.y + timestrip_height),
                );

                let main =
                    Rect::from_min_max(Pos2::new(timestrip.min.x, timestrip.max.y + gap), rect.max);

                let p = ui.painter();
                p.rect_filled(timestrip, 0.0, Color32::RED);
                p.text(
                    timestrip.center(),
                    Align2::CENTER_CENTER,
                    "Timestrip",
                    FontId::default(),
                    Color32::BLACK,
                );

                p.rect_filled(main, 0.0, style.visuals.widgets.inactive.bg_fill);
                p.text(
                    main.center(),
                    Align2::CENTER_CENTER,
                    "Main arrangement grid",
                    FontId::default(),
                    Color32::BLACK,
                );
            });
        }
    }
}
