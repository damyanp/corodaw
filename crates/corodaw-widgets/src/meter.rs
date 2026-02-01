use eframe::egui::{Color32, Response, Sense, Stroke, StrokeKind, Ui, Widget, remap, vec2};

#[derive(Default)]
pub struct Meter {
    normalized_value: f32,
    width: f32,
    height: f32,
}

impl Meter {
    pub fn new(normalized_value: f32) -> Self {
        Self {
            normalized_value,
            width: 10.0,
            height: 30.0,
        }
    }

    pub fn width(self, width: f32) -> Self {
        Self { width, ..self }
    }

    pub fn height(self, height: f32) -> Self {
        Self { height, ..self }
    }
}

impl Widget for Meter {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, response) =
            ui.allocate_exact_size(vec2(self.width, self.height), Sense::click());

        let p = ui.painter_at(rect);

        let s = if response.hovered() {
            &ui.style().visuals.widgets.hovered
        } else {
            &ui.style().visuals.widgets.inactive
        };

        // background
        p.rect_filled(rect, 0.0, s.bg_fill);

        let vu = normalized_value_to_vu_units(self.normalized_value);

        let vu_range = 3.0..=-20.0;

        let to_y = |vu| remap(vu, vu_range.clone(), rect.y_range());

        let y = to_y(vu);
        let green_min_y = to_y(-3.0);
        let yellow_min_y = to_y(0.0);

        let bar_rect = rect.with_min_y(y);

        if y < yellow_min_y {
            p.rect_filled(bar_rect.with_max_y(yellow_min_y), 0.0, Color32::RED);
        }

        if y < green_min_y {
            p.rect_filled(
                bar_rect
                    .with_max_y(green_min_y)
                    .with_min_y(y.max(yellow_min_y)),
                0.0,
                Color32::YELLOW,
            );
        }

        p.rect_filled(bar_rect.with_min_y(y.max(green_min_y)), 0.0, Color32::GREEN);

        p.hline(rect.x_range(), y, Stroke::new(0.5, Color32::WHITE));
        p.rect_stroke(rect, 0.0, s.fg_stroke, StrokeKind::Inside);

        response
    }
}

fn normalized_value_to_vu_units(value: f32) -> f32 {
    20.0 * value.max(1e-6).log10() + 3.0
}
