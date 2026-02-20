use std::f32;

use egui::{
    Align, Color32, Context, CursorIcon, Direction, Id, Layout, NumExt, PointerButton, Rect,
    ScrollArea, Sense, Stroke, TextStyle, Ui, UiBuilder, Vec2, emath, pos2,
    scroll_area::{ScrollBarVisibility, ScrollSource},
    vec2,
};

#[derive(Clone, Copy, Debug, Default)]
enum DragAxis {
    #[default]
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, Copy)]
struct State {
    width: f32,
    pixels_per_beat: f32,
}

impl State {
    const DEFAULT_PIXELS_PER_BEAT: f32 = 20.0;
    const MIN_PIXELS_PER_BEAT: f32 = 5.0;
    const MAX_PIXELS_PER_BEAT: f32 = 200.0;

    fn load(ctx: &Context, id: Id) -> Option<Self> {
        ctx.data(|d| d.get_temp(id))
    }

    fn store(self, ctx: &Context, id: Id) {
        ctx.data_mut(|d| d.insert_temp(id, self));
    }

    /// Load–modify–store: updates only the fields touched by `f`.
    fn update(ctx: &Context, id: Id, f: impl FnOnce(&mut Self)) {
        let mut state = Self::load(ctx, id).unwrap_or_default();
        f(&mut state);
        state.store(ctx, id);
    }
}

impl Default for State {
    fn default() -> Self {
        Self {
            width: 300.0,
            pixels_per_beat: Self::DEFAULT_PIXELS_PER_BEAT,
        }
    }
}

pub trait ArrangerDataProvider {
    fn num_channels(&self) -> usize;
    fn channel_height(&self, index: usize) -> f32;
    fn show_channel(&mut self, index: usize, ui: &mut Ui);
    fn show_strip(&mut self, index: usize, ui: &mut Ui, pixels_per_beat: f32);
    fn show_timestrip(&mut self, ui: &mut Ui, pixels_per_beat: f32);
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

        let mut pixels_per_beat = State::load(ui.ctx(), id)
            .map(|s| s.pixels_per_beat)
            .unwrap_or(State::DEFAULT_PIXELS_PER_BEAT);

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

        // Handle ctrl+wheel zoom over the strips/timestrip area
        let zoom_rect = Rect::from_min_max(
            pos2(strips_rect.min.x, timestrip_rect.min.y),
            strips_rect.max,
        );
        let (ctrl_scrolling, zoom_mouse_x) = ui.ctx().input(|i| {
            let hovering = i.pointer.hover_pos().filter(|p| zoom_rect.contains(*p));
            let is_zooming = i.modifiers.ctrl && i.raw_scroll_delta.y != 0.0 && hovering.is_some();
            (is_zooming, hovering.map(|p| p.x))
        });
        let old_pixels_per_beat = pixels_per_beat;
        if ctrl_scrolling {
            let scroll_delta = ui.ctx().input(|i| i.raw_scroll_delta.y);
            let factor = (scroll_delta / 120.0).exp2();
            pixels_per_beat = (pixels_per_beat * factor)
                .clamp(State::MIN_PIXELS_PER_BEAT, State::MAX_PIXELS_PER_BEAT);
        }

        State::update(ui.ctx(), id, |s| s.pixels_per_beat = pixels_per_beat);

        // Disable mouse wheel scrolling when ctrl is held (zoom takes priority)
        let scroll_source = if ctrl_scrolling {
            ScrollSource::SCROLL_BAR
        } else {
            ScrollSource::SCROLL_BAR | ScrollSource::MOUSE_WHEEL
        };

        // Compute zoom-adjusted scroll offset using the public API.
        // We read the current offset from scroll_area::State (requires a known id_salt),
        // then pass the adjusted offset via .horizontal_scroll_offset() so it applies
        // before the scroll area renders — no flicker.
        let scroll_id_salt = id.with("__scroll");
        let scroll_id = ui.make_persistent_id(Id::new(scroll_id_salt));
        let zoom_offset = if ctrl_scrolling
            && let Some(mouse_x) = zoom_mouse_x
            && let Some(state) = egui::scroll_area::State::load(ui.ctx(), scroll_id)
        {
            let cursor_in_strip = mouse_x - strips_rect.min.x;
            let beat = (state.offset.x + cursor_in_strip) / old_pixels_per_beat;
            Some((beat * pixels_per_beat - cursor_in_strip).max(0.0))
        } else {
            None
        };

        let mut scroll_area = ScrollArea::both()
            .id_salt(scroll_id_salt)
            .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
            .scroll_bar_rect(strips_rect)
            .scroll_source(scroll_source);
        if let Some(offset) = zoom_offset {
            scroll_area = scroll_area.horizontal_scroll_offset(offset);
        }
        let r = scroll_area.show_viewport(ui, |ui, viewport| {
            show_channels(
                &mut data,
                ui,
                id,
                drag_id,
                drag_info,
                gap,
                pixels_per_beat,
                timestrip_rect,
                channels_rect,
                strips_rect,
                viewport,
            )
        });

        let ((drop_target, dropped), strip_drag_delta) = r.inner;

        // Apply drag-to-pan from the strip area
        if strip_drag_delta != Vec2::ZERO
            && let Some(mut state) = egui::scroll_area::State::load(ui.ctx(), r.id)
        {
            state.offset -= strip_drag_delta;
            state.store(ui.ctx(), r.id);
        }

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
    id: Id,
    drag_id: Id,
    drag_info: Option<usize>,
    gap: f32,
    pixels_per_beat: f32,
    timestrip_rect: Rect,
    channels_rect: Rect,
    strips_rect: Rect,
    viewport: Rect,
) -> ((Option<(usize, f32)>, bool), Vec2) {
    ui.painter().rect_filled(
        timestrip_rect,
        1.0,
        ui.style().visuals.widgets.active.bg_fill,
    );

    // Render the time strip with horizontal scroll offset matching the strips
    let timestrip_content_rect = Rect::from_min_size(
        pos2(timestrip_rect.min.x - viewport.min.x, timestrip_rect.min.y),
        vec2(viewport.width(), timestrip_rect.height()),
    );
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(timestrip_content_rect)
            .layout(Layout::top_down(Align::Min)),
        |ui| {
            ui.shrink_clip_rect(timestrip_rect);
            data.show_timestrip(ui, pixels_per_beat);
        },
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
    let mut strip_drag_delta = Vec2::ZERO;
    let strip_drag_axis_id = id.with("__strip_drag_axis");

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
                    .layout(Layout::top_down(Align::Min))
                    .sense(Sense::click_and_drag()),
                |ui| {
                    ui.shrink_clip_rect(strips_rect);
                    data.show_strip(i, ui, pixels_per_beat);
                },
            )
            .response;

        if r.drag_started_by(PointerButton::Primary) {
            ui.ctx()
                .data_mut(|d| d.remove_temp::<DragAxis>(strip_drag_axis_id));
        }

        if r.dragged() {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
            let delta = r.drag_delta();
            let axis: Option<DragAxis> = ui.ctx().data(|d| d.get_temp(strip_drag_axis_id));
            let axis = axis.unwrap_or_else(|| {
                let a = if delta.x.abs() >= delta.y.abs() {
                    DragAxis::Horizontal
                } else {
                    DragAxis::Vertical
                };
                ui.ctx().data_mut(|d| d.insert_temp(strip_drag_axis_id, a));
                a
            });
            strip_drag_delta += match axis {
                DragAxis::Horizontal => vec2(delta.x, 0.0),
                DragAxis::Vertical => vec2(0.0, delta.y),
            };
        } else if r.drag_stopped() {
            ui.ctx()
                .data_mut(|d| d.remove_temp::<DragAxis>(strip_drag_axis_id));
        } else if r.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
        }

        r.context_menu(|ui| {
            data.show_strip_menu(i, ui);
        });

        y += channel_height + gap;
    }

    let add_button_scope = ui.scope_builder(
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

    // Ensure content height fills at least the visible area so the horizontal
    // scrollbar appears at the bottom of the window rather than after the last channel.
    let content_bottom = add_button_scope.response.rect.bottom();
    let min_bottom = channels_rect.min.y - viewport.min.y + channels_rect.height();
    if content_bottom < min_bottom {
        ui.allocate_rect(
            Rect::from_min_max(
                pos2(channels_rect.min.x, content_bottom),
                pos2(channels_rect.max.x, min_bottom),
            ),
            Sense::empty(),
        );
    }

    ((drop_target, dropped), strip_drag_delta)
}

fn show_resize_bar(rect: Rect, default_width: f32, gap: f32, id: Id, ui: &mut Ui) -> f32 {
    let mut width = State::load(ui.ctx(), id)
        .map(|s| s.width)
        .unwrap_or(default_width);
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

    State::update(ui.ctx(), id, |s| s.width = width);

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
