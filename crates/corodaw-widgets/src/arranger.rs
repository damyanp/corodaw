use std::f32;

use eframe::{
    egui::{
        Align, Align2, Color32, Context, CursorIcon, Direction, FontId, Id, Layout, NumExt,
        PointerButton, Rect, ScrollArea, Sense, Stroke, TextStyle, Ui, UiBuilder, pos2,
        scroll_area::ScrollBarVisibility, vec2,
    },
    emath::{self},
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
    fn move_channel(&mut self, index: usize, destination: usize);
    fn show_channel_menu(&mut self, index: usize, ui: &mut Ui);
    fn show_strip_menu(&mut self, index: usize, ui: &mut Ui);
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

        let drag_id = id.with("__drag_channel");
        let drag_info = ui.data(|data| data.get_temp::<usize>(drag_id));

        let rect = ui.available_rect_before_wrap();
        let gap = 5.0;
        let timestrip_height = ui.text_style_height(&TextStyle::Heading) * 2.0;

        let width = show_resize_bar(rect, default_width, gap, id, ui);

        let timestrip_rect = Rect::from_min_max(
            pos2(rect.left() + width + gap, rect.top()),
            pos2(rect.right(), rect.top() + timestrip_height),
        );

        let channels_rect = Rect::from_min_max(
            pos2(rect.min.x, timestrip_rect.max.y + gap),
            pos2(rect.min.x + width, rect.max.y),
        );

        let strips_rect = Rect::from_min_max(
            pos2(channels_rect.max.x + gap, channels_rect.min.y),
            pos2(rect.max.x, rect.max.y),
        );

        let r = ScrollArea::both()
            .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
            .scroll_bar_rect(strips_rect)
            .on_hover_cursor(CursorIcon::Grab)
            .on_drag_cursor(CursorIcon::Grabbing)
            .show_viewport(ui, |ui, viewport| {
                show_channels(
                    &mut data,
                    ui,
                    drag_id,
                    drag_info,
                    gap,
                    timestrip_rect,
                    channels_rect,
                    strips_rect,
                    viewport,
                )
            });

        let (drop_target, dropped) = r.inner;

        if let Some((drop_index, drop_y)) = drop_target {
            if dropped {
                let dragged_channel =
                    ui.data_mut(|data| data.remove_temp::<usize>(drag_id).unwrap());

                data.move_channel(dragged_channel, drop_index);
            } else {
                let p = ui.painter();
                let stroke = Stroke::new(2.0, Color32::WHITE);

                p.hline(channels_rect.x_range(), drop_y, stroke);

                let chevron = [vec2(-gap, -gap), vec2(0.0, 0.0), vec2(-gap, gap)];
                let left_chevron: Vec<_> = chevron
                    .iter()
                    .map(|p| pos2(channels_rect.min.x, drop_y) + *p)
                    .collect();
                p.line(left_chevron, stroke);
                let right_chevron: Vec<_> = chevron
                    .iter()
                    .map(|p| pos2(channels_rect.max.x, drop_y) - *p)
                    .collect();
                p.line(right_chevron, stroke);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn show_channels(
    data: &mut impl ArrangerDataProvider,
    ui: &mut Ui,
    drag_id: Id,
    drag_info: Option<usize>,
    gap: f32,
    timestrip_rect: Rect,
    channels_rect: Rect,
    strips_rect: Rect,
    viewport: Rect,
) -> (Option<(usize, f32)>, bool) {
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

    let num_channels = data.num_channels();

    let mut drop_target = None;
    let mut dropped = false;

    let mut y = channels_rect.min.y - viewport.min.y;

    for i in 0..num_channels {
        let channel_height = data.channel_height(i);

        let channel_rect = Rect::from_min_size(
            pos2(channels_rect.min.x, y),
            vec2(channels_rect.width(), channel_height),
        );

        let strip_rect = Rect::from_min_size(
            pos2(strips_rect.min.x - viewport.min.x, y),
            vec2(viewport.width(), channel_height),
        );

        if drag_info.is_some()
            && let Some(pos) = ui.ctx().pointer_hover_pos()
            && channel_rect.contains(pos)
        {
            let rect = channel_rect.expand2(vec2(0.0, gap));

            let t = emath::remap(pos.y, rect.y_range(), 0.0..=1.0);

            if (0.0..0.5).contains(&t) {
                drop_target = Some((i, rect.top() + gap / 2.0));
            } else if t <= 1.0 {
                drop_target = Some((i + 1, rect.bottom() - gap / 2.0));
            }
        }

        let r = ui
            .scope_builder(
                UiBuilder::new()
                    .max_rect(channel_rect)
                    .layout(Layout::centered_and_justified(Direction::TopDown))
                    .sense(Sense::click_and_drag()),
                |ui| {
                    ui.shrink_clip_rect(channels_rect);
                    data.show_channel(i, ui);
                },
            )
            .response;

        if drag_info == Some(i) {
            ui.painter()
                .rect_filled(channel_rect, 0.0, Color32::WHITE.gamma_multiply(0.25));
        }

        if r.drag_started_by(PointerButton::Primary) {
            ui.data_mut(|data| {
                data.insert_temp(drag_id, i);
            });
        } else if r.drag_stopped() {
            dropped = true;
        }

        r.context_menu(|ui| {
            data.show_channel_menu(i, ui);
        });

        let r = ui
            .scope_builder(
                UiBuilder::new()
                    .max_rect(strip_rect)
                    .layout(Layout::top_down(Align::Min)),
                |ui| {
                    ui.shrink_clip_rect(strips_rect);
                    data.show_strip(i, ui);
                },
            )
            .response;

        r.context_menu(|ui| {
            data.show_strip_menu(i, ui);
        });

        y += channel_height + gap;
    }

    ui.scope_builder(
        UiBuilder::new()
            .layout(Layout::top_down(Align::Center))
            .max_rect(Rect::from_min_size(
                pos2(channels_rect.min.x, y),
                vec2(channels_rect.width(), f32::INFINITY),
            ))
            // This sense is to prevent the scroll area from sensing events here
            .sense(Sense::click_and_drag()),
        |ui| {
            if ui.button("+").clicked() {
                data.on_add_channel(num_channels);
            }
        },
    );
    (drop_target, dropped)
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
