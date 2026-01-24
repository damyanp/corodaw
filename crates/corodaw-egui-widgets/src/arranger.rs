use eframe::egui::{
    Align2, Context, CursorIcon, FontId, Id, NumExt, Rect, Sense, TextStyle, Ui, UiBuilder, pos2,
    vec2,
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

pub trait ArrangerDataProvider {
    fn num_channels(&self) -> usize;
    fn channel_height(&self, index: usize) -> f32;
    fn show_channel(&mut self, index: usize, ui: &mut Ui);
    fn show_strip(&mut self, index: usize, ui: &mut Ui);
    fn on_add_channel(&mut self);
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

    pub fn show(self, mut data: impl ArrangerDataProvider, ui: &mut Ui) {
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
            let resize_rect = Rect::from_x_y_ranges(resize_x..=resize_x, available_rect.y_range())
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

        for i in 0..data.num_channels() {
            let channel_height = data.channel_height(i);

            let mut channel_rect = channels_rect;
            channel_rect.set_height(channel_height);

            let mut strip_rect = strips_rect;
            strip_rect.set_height(channel_height);

            ui.scope_builder(UiBuilder::new().max_rect(channel_rect), |ui| {
                ui.set_max_size(channel_rect.size());
                ui.shrink_clip_rect(channel_rect);
                data.show_channel(i, ui);
            });

            ui.scope_builder(UiBuilder::new().max_rect(strip_rect), |ui| {
                ui.set_max_size(strip_rect.size());
                ui.shrink_clip_rect(strip_rect);
                data.show_strip(i, ui);
            });

            channels_rect.set_top(channel_rect.bottom() + gap);
            strips_rect.set_top(strip_rect.bottom() + gap);
        }

        ui.scope_builder(UiBuilder::new().max_rect(channels_rect), |ui| {
            if ui.button("+").clicked() {
                println!("!");
                data.on_add_channel();
            }
        });
    }
}
