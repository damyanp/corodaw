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
                    arranger::ArrangerWidget::new("arranger").show(ctx, ui);
                });
        });
    }
}

mod arranger {
    use eframe::egui::{
        Align2, CentralPanel, Color32, Context, CursorIcon, FontId, Id, NumExt, Pos2, Rect, Sense,
        SidePanel, Style, TextStyle, Ui, Vec2, panel::Side, pos2, vec2,
    };

    #[derive(Clone, Debug, Copy)]
    struct State {
        width: f32,
    }

    impl State {
        fn load(ctx: &Context, id: Id) -> Option<Self> {
            ctx.data(|d| d.get_temp(id))
        }

        fn store(self, ctx: &Context, id: Id) {
            ctx.data_mut(|d| d.insert_temp(id, self));
        }
    }

    pub struct ArrangerWidget {
        id: Id,
        default_width: f32,
    }

    impl ArrangerWidget {
        pub fn new(id: impl Into<Id>) -> Self {
            Self {
                id: id.into(),
                default_width: 300.0,
            }
        }

        pub fn show(self, _: &Context, ui: &mut Ui) {
            let Self { id, default_width } = self;

            let available_rect = ui.available_rect_before_wrap();

            let mut width = default_width;
            {
                if let Some(state) = State::load(ui.ctx(), id) {
                    width = state.width;
                }
                width = width.at_most(available_rect.width());
            }

            let resize_id = id.with("__resize");

            // Grab any interaction values from previous frame
            let mut is_resizing;
            if let Some(resize_response) = ui.ctx().read_response(resize_id) {
                is_resizing = resize_response.dragged();

                if is_resizing && let Some(pointer) = resize_response.interact_pointer_pos() {
                    width = pointer.x - available_rect.left();
                }
            }

            let resize_hover;
            {
                let resize_x = available_rect.left() + width;
                let resize_rect =
                    Rect::from_x_y_ranges(resize_x..=resize_x, available_rect.y_range())
                        .expand2(vec2(ui.style().interaction.resize_grab_radius_side, 0.0));
                let resize_response = ui.interact(resize_rect, resize_id, Sense::drag());
                resize_hover = resize_response.hovered();
                is_resizing = resize_response.dragged();
            }

            if resize_hover || is_resizing {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
            }

            State { width }.store(ui.ctx(), id);

            let gap = 5.0;

            // draw the resize bar
            {
                let stroke = if is_resizing {
                    ui.style().visuals.widgets.active.fg_stroke
                } else if resize_hover {
                    ui.style().visuals.widgets.hovered.fg_stroke
                } else {
                    ui.style().visuals.widgets.noninteractive.bg_stroke
                };

                ui.painter().vline(
                    available_rect.left() + width + gap * 0.5,
                    available_rect.y_range(),
                    stroke,
                );
            }

            let timestrip_height = ui.text_style_height(&TextStyle::Heading) * 2.0;
            let timestrip_rect = Rect::from_min_max(
                pos2(available_rect.left() + width + gap, available_rect.top()),
                pos2(
                    available_rect.right(),
                    available_rect.top() + timestrip_height,
                ),
            );

            ui.painter().rect_filled(
                timestrip_rect,
                1.0,
                ui.style().visuals.widgets.active.bg_fill,
            );
            ui.painter().text(
                timestrip_rect.center(),
                Align2::CENTER_CENTER,
                "Time Strip",
                FontId::default(),
                ui.style().visuals.widgets.active.text_color(),
            );

            let num_channels = 10;

            let mut channels_rect = Rect::from_min_max(
                pos2(available_rect.min.x, timestrip_rect.max.y + gap),
                pos2(available_rect.min.x + width, available_rect.max.y),
            );

            let mut strips_rect = Rect::from_min_max(
                pos2(channels_rect.max.x + gap, channels_rect.min.y),
                pos2(available_rect.max.x, available_rect.max.y),
            );

            ui.painter().rect_filled(
                channels_rect,
                1.0,
                ui.style().visuals.widgets.noninteractive.bg_fill,
            );
            ui.painter().rect_filled(
                strips_rect,
                1.0,
                ui.style().visuals.widgets.noninteractive.bg_fill,
            );

            let channel_height = 50.0;

            for i in 0..num_channels {
                let mut channel_rect = channels_rect;
                channel_rect.set_height(channel_height);

                let mut strip_rect = strips_rect;
                strip_rect.set_height(channel_height);

                ui.painter().rect_filled(
                    channel_rect,
                    2.0,
                    ui.style().visuals.widgets.inactive.bg_fill,
                );
                ui.painter().rect_filled(
                    strip_rect,
                    0.0,
                    ui.style().visuals.widgets.inactive.bg_fill,
                );
                ui.painter().text(
                    channel_rect.center(),
                    Align2::CENTER_CENTER,
                    format!("Channel {i}"),
                    FontId::default(),
                    ui.style().visuals.widgets.active.text_color(),
                );

                channels_rect.set_top(channel_rect.bottom() + gap);
                strips_rect.set_top(strip_rect.bottom() + gap);
            }
        }
    }
}
