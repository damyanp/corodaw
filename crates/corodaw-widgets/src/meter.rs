use eframe::egui::{Color32, Response, Sense, Stroke, StrokeKind, Ui, Widget, remap, vec2};

#[derive(Default)]
pub struct Meter<'a> {
    values: &'a [f32],
    width: f32,
    height: f32,
}

impl<'a> Meter<'a> {
    pub fn new(normalized_value: &'a [f32]) -> Self {
        Self {
            values: normalized_value,
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

impl Widget for Meter<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, response) =
            ui.allocate_exact_size(vec2(self.width, self.height), Sense::click());

        let p = ui.painter_at(rect);

        let s = if response.hovered() {
            &ui.style().visuals.widgets.hovered
        } else {
            &ui.style().visuals.widgets.inactive
        };

        let num_values = self.values.len();

        // background
        p.rect_filled(rect, 0.0, s.bg_fill);

        if num_values > 0 {
            let bar_width = rect.width() / num_values as f32;

            for i in 0..num_values {
                let rect = rect
                    .with_min_x(rect.min.x + bar_width * i as f32 + 1.0)
                    .with_max_x(rect.min.x + bar_width * (i + 1) as f32 - 1.0);

                let value = self.values[i];
                let vu = normalized_value_to_vu_units(value);

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

                if i > 0 {
                    p.vline(
                        rect.min.x - 0.5,
                        rect.y_range(),
                        Stroke::new(1.0, Color32::BLACK),
                    );
                }
            }
        }

        p.rect_stroke(rect, 0.0, s.fg_stroke, StrokeKind::Inside);

        response
    }
}

fn normalized_value_to_vu_units(value: f32) -> f32 {
    20.0 * value.max(1e-6).log10() + 3.0
}
