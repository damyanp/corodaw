use eframe::egui::{
    Align2, Button, Context, CursorIcon, Direction, FontId, Id, Layout, NumExt, Rect, Sense,
    TextStyle, Ui, UiBuilder, pos2, vec2,
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
    fn on_add_channel(&mut self, index: usize);
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

        let rect = ui.available_rect_before_wrap();
        let gap = 5.0;
        let timestrip_height = ui.text_style_height(&TextStyle::Heading) * 2.0;

        let width = show_resize_bar(rect, default_width, gap, id, ui);

        let timestrip_rect = Rect::from_min_max(
            pos2(rect.left() + width + gap, rect.top()),
            pos2(rect.right(), rect.top() + timestrip_height),
        );

        let mut channels_rect = Rect::from_min_max(
            pos2(rect.min.x, timestrip_rect.max.y + gap),
            pos2(rect.min.x + width, rect.max.y),
        );

        let mut strips_rect = Rect::from_min_max(
            pos2(channels_rect.max.x + gap, channels_rect.min.y),
            pos2(rect.max.x, rect.max.y),
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

        let mut add_line = None;

        for i in 0..data.num_channels() {
            let channel_height = data.channel_height(i);

            let mut channel_rect = channels_rect;
            channel_rect.set_height(channel_height);

            let mut strip_rect = strips_rect;
            strip_rect.set_height(channel_height);

            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(channel_rect)
                    .layout(Layout::centered_and_justified(Direction::TopDown)),
                |ui| {
                    data.show_channel(i, ui);
                },
            );

            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(strip_rect)
                    .layout(Layout::centered_and_justified(Direction::TopDown)),
                |ui| {
                    data.show_strip(i, ui);
                },
            );

            let mut interact_rect = Rect::from_min_max(
                pos2(0.0, channel_rect.top()),
                pos2(rect.right(), channel_rect.bottom() + gap),
            );

            if i == 0 {
                interact_rect.extend_with_y(channel_rect.min.y - gap);
            }

            if let Some(pos) = ui.ctx().pointer_hover_pos()
                && interact_rect.contains(pos)
            {
                let t = eframe::egui::emath::remap(pos.y, interact_rect.y_range(), 0.0..=1.0);

                if (0.0..0.5).contains(&t) {
                    add_line = Some((i, channel_rect.top() - gap / 2.0));
                } else if t <= 1.0 {
                    add_line = Some((i + 1, channel_rect.bottom() + gap / 2.0));
                }
            }

            channels_rect.set_top(channel_rect.bottom() + gap);
            strips_rect.set_top(strip_rect.bottom() + gap);
        }

        if let Some((index, y)) = add_line {
            let style = &ui.style().visuals.widgets;
            let p = ui.painter();

            let style = ui
                .ctx()
                .pointer_hover_pos()
                .and_then(|pos| {
                    if pos.y >= y - gap && pos.y <= y + gap {
                        Some(style.hovered)
                    } else {
                        None
                    }
                })
                .unwrap_or(style.inactive);

            p.hline(
                channels_rect.left()..=strips_rect.right(),
                y,
                style.fg_stroke,
            );

            let add_rect = Rect::from_center_size(pos2(channels_rect.left(), y), vec2(20.0, 20.0));

            if ui.place(add_rect, Button::new("+")).clicked() {
                data.on_add_channel(index);
            }
        }
    }
}

fn show_resize_bar(rect: Rect, default_width: f32, gap: f32, id: Id, ui: &mut Ui) -> f32 {
    let mut width = default_width;
    if let Some(state) = State::load(ui.ctx(), id) {
        width = state.width;
    }
    width = width.at_most(rect.width());

    let resize_id = id.with("__resize");

    // Grab any interaction values from previous frame
    let mut is_resizing;
    if let Some(resize_response) = ui.ctx().read_response(resize_id) {
        is_resizing = resize_response.dragged();

        if is_resizing && let Some(pointer) = resize_response.interact_pointer_pos() {
            width = pointer.x - rect.left();
        }
    }

    let resize_hover;
    {
        let resize_x = rect.left() + width;
        let resize_rect = Rect::from_x_y_ranges(resize_x..=resize_x, rect.y_range())
            .expand2(vec2(ui.style().interaction.resize_grab_radius_side, 0.0));
        let resize_response = ui.interact(resize_rect, resize_id, Sense::drag());
        resize_hover = resize_response.hovered();
        is_resizing = resize_response.dragged();
    }

    if resize_hover || is_resizing {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
    }

    State { width }.store(ui.ctx(), id);

    // draw the resize bar
    {
        let stroke = if is_resizing {
            ui.style().visuals.widgets.active.fg_stroke
        } else if resize_hover {
            ui.style().visuals.widgets.hovered.fg_stroke
        } else {
            ui.style().visuals.widgets.noninteractive.fg_stroke
        };

        ui.painter()
            .vline(rect.left() + width + gap * 0.5, rect.y_range(), stroke);
    }

    width
}
