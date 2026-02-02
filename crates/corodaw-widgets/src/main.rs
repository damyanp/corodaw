use corodaw_widgets::{
    arranger::{ArrangerDataProvider, ArrangerWidget},
    meter::Meter,
};
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

struct App {
    channels: Vec<usize>,
    perlin: Perlin1D,
    perlin_x: f32,
}

impl App {
    fn new(_: &eframe::CreationContext<'_>) -> Self {
        let channels = vec![0, 1];
        let perlin = Perlin1D::new(1337);
        Self {
            channels,
            perlin,
            perlin_x: 0.0,
        }
    }

    fn test_arranger(&mut self, ui: &mut Ui) {
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

            fn show_channel_menu(&mut self, index: usize, ui: &mut Ui) {
                let number = self.0.channels[index];
                ui.label(format!("Channel {number}"));
                ui.separator();
                if ui.button("Add channel").clicked() {
                    self.on_add_channel(index);
                }
            }

            fn show_strip_menu(&mut self, _: usize, ui: &mut Ui) {
                ui.label("Context menu for strip");
            }
        }

        ArrangerWidget::new("arranger").show(TestArranger(self), ui);
    }

    fn test_meters(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            let count = 32;
            let step = 0.15;

            ui.add(Meter::new(&[(self.perlin_x * 0.01) % 1.0]));

            for i in 0..count {
                let x = self.perlin_x + i as f32 * step;
                let v = self.perlin.noise(x);

                if i % 2 == 0 {
                    let v2 = self.perlin.noise(x + 1.0);
                    ui.add(Meter::new(&[v, v2]));
                } else {
                    ui.add(Meter::new(&[v]));
                }
            }
        });

        self.perlin_x += 0.05;
        ui.ctx().request_repaint();
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Corodaw widgets");

            CollapsingHeader::new("Arranger")
                .default_open(true)
                .show(ui, |ui| {
                    self.test_arranger(ui);
                });
            CollapsingHeader::new("Meters")
                .default_open(false)
                .show(ui, |ui| {
                    self.test_meters(ui);
                });
        });
    }
}

struct Perlin1D {
    perm: [u8; 256],
}

impl Perlin1D {
    fn new(seed: u32) -> Self {
        let mut perm = [0u8; 256];
        for (i, v) in perm.iter_mut().enumerate() {
            *v = i as u8;
        }

        let mut state = seed;
        for i in (1..256).rev() {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let j = (state % (i as u32 + 1)) as usize;
            perm.swap(i, j);
        }

        Self { perm }
    }

    fn fade(t: f32) -> f32 {
        t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
    }

    fn grad(hash: u8, x: f32) -> f32 {
        if hash & 1 == 0 { x } else { -x }
    }

    fn noise(&self, x: f32) -> f32 {
        let x0 = x.floor();
        let xi = (x0 as i32 & 255) as usize;
        let xf = x - x0;

        let u = Self::fade(xf);
        let a = self.perm[xi];
        let b = self.perm[(xi + 1) & 255];

        let g1 = Self::grad(a, xf);
        let g2 = Self::grad(b, xf - 1.0);
        let lerp = g1 + u * (g2 - g1);

        (lerp + 1.0) * 0.5
    }
}
