use gpui::{
    AnyElement, Bounds, Context, MouseButton, MouseMoveEvent, Pixels, Point, ScrollHandle, Size,
    Task, Window, prelude::*,
};
use std::{
    cell::{Cell, RefCell},
    cmp::Ordering,
    ops::Range,
    rc::Rc,
    time::Duration,
};

use actions::editor::Hover;
use multi_buffer::{Anchor, MultiBufferSnapshot, ToOffset};
use project::project_config_store::VariableScope;
use settings::Settings;
use theme::ActiveTheme;
use ui::{
    Color, CopyButton, Icon, IconAsset, IconSize, ScrollAxes, Scrollbars, StyledExt, Text,
    TextCommon, TextSize, WithScrollbar,
};
use util::ResultExt;

use crate::{
    Editor, EditorSettings, EditorStyle,
    display_map::{DisplayPoint, DisplayRow, DisplaySnapshot, HighlightKey, ToDisplayPoint},
    variable_provider,
};

pub(crate) const MIN_POPOVER_CHARACTER_WIDTH: f32 = 20.0;
pub(crate) const MIN_POPOVER_LINE_HEIGHT: f32 = 4.0;
pub(crate) const POPOVER_RIGHT_OFFSET: Pixels = gpui::px(8.0);
pub(crate) const HOVER_POPOVER_GAP: Pixels = gpui::px(10.0);

pub fn hover(editor: &mut Editor, _: &Hover, window: &mut Window, cx: &mut Context<Editor>) {
    let head = editor.selections.newest_anchor().head();
    show_hover(editor, head, true, window, cx);
}

pub(crate) fn hover_at(
    editor: &mut Editor,
    anchor: Option<Anchor>,
    mouse_position: Option<Point<Pixels>>,
    window: &mut Window,
    cx: &mut Context<Editor>,
) {
    if EditorSettings::get_global(cx).hover_popover.enabled {
        if show_keyboard_hover(editor, window, cx) {
            return;
        }

        if let Some(anchor) = anchor {
            editor.hover_state.hiding_delay_task = None;
            editor.hover_state.closest_mouse_distance = None;
            show_hover(editor, anchor, false, window, cx);
        } else if !editor.hover_state.visible() {
            editor.hover_state.info_task = None;
        } else {
            let mut getting_closer = false;
            if let Some(mouse_position) = mouse_position {
                getting_closer = editor.hover_state.is_mouse_getting_closer(mouse_position);
            }

            if !getting_closer && editor.hover_state.hiding_delay_task.is_some() {
                return;
            }

            let delay =
                Duration::from_millis(EditorSettings::get_global(cx).hover_popover.hiding_delay.0);
            let task = cx.spawn(async move |this, cx| {
                cx.background_executor().timer(delay).await;
                if let Err(error) = this.update(cx, |editor, cx| {
                    hide_hover(editor, cx);
                }) {
                    log::debug!("Failed to hide hover popover: {error:?}");
                }
            });
            editor.hover_state.hiding_delay_task = Some(task);
        }
    }
}

fn show_keyboard_hover(editor: &mut Editor, window: &mut Window, cx: &mut Context<Editor>) -> bool {
    let Some(anchor) = editor
        .hover_state
        .info_popovers
        .iter()
        .find_map(|info_popover| {
            (*info_popover.keyboard_grace.borrow()).then_some(info_popover.anchor)
        })
    else {
        return false;
    };

    show_hover(editor, anchor, false, window, cx);
    true
}

pub(crate) fn hide_hover(editor: &mut Editor, cx: &mut Context<Editor>) -> bool {
    let did_hide = !editor.hover_state.info_popovers.is_empty();
    editor.hover_state.info_popovers.clear();
    editor.hover_state.info_task = None;
    editor.hover_state.hiding_delay_task = None;
    editor.hover_state.closest_mouse_distance = None;

    editor.clear_background_highlights(HighlightKey::HoverState, cx);

    if did_hide {
        cx.notify();
    }

    did_hide
}

fn show_hover(
    editor: &mut Editor,
    anchor: Anchor,
    ignore_timeout: bool,
    window: &mut Window,
    cx: &mut Context<Editor>,
) -> Option<()> {
    if editor.has_visible_completions_menu() {
        return None;
    }

    let snapshot = editor.buffer_snapshot(cx);
    let (buffer_position, _) = snapshot.anchor_to_buffer_anchor(anchor)?;
    let buffer = editor.buffer.read(cx).buffer(buffer_position.buffer_id)?;
    let provider = editor.semantics_provider.clone()?;

    editor.hover_state.hiding_delay_task = None;
    editor.hover_state.closest_mouse_distance = None;

    if !ignore_timeout {
        if same_info_hover(editor, &snapshot, anchor) {
            return None;
        }
        hide_hover(editor, cx);
    }

    let hover_popover_delay = EditorSettings::get_global(cx).hover_popover.delay.0;
    let task = cx.spawn_in(window, async move |this, cx| {
        async move {
            let delay = if ignore_timeout {
                None
            } else {
                let request_early = hover_popover_delay / 2;
                cx.background_executor()
                    .timer(Duration::from_millis(hover_popover_delay - request_early))
                    .await;
                Some(
                    cx.background_executor()
                        .timer(Duration::from_millis(request_early)),
                )
            };

            let hover_request = cx.update(|_, cx| provider.hover(&buffer, buffer_position, cx))?;
            if let Some(delay) = delay {
                delay.await;
            }
            let hovers = match hover_request {
                Some(hover_request) => hover_request.await.unwrap_or_default(),
                None => Vec::new(),
            };

            this.update_in(cx, |editor, window, cx| {
                let snapshot = editor.buffer_snapshot(cx);
                let mut hover_highlights = Vec::new();
                let mut info_popovers = Vec::new();
                for hover in hovers {
                    let Some(range) =
                        snapshot.buffer_anchor_range_to_anchor_range(hover.range.clone())
                    else {
                        continue;
                    };
                    hover_highlights.push(range.clone());
                    info_popovers.push(InfoPopover {
                        symbol_range: range,
                        hover,
                        scroll_handle: ScrollHandle::new(),
                        keyboard_grace: Rc::new(RefCell::new(ignore_timeout)),
                        anchor,
                        last_bounds: Rc::new(Cell::new(None)),
                    });
                }

                if hover_highlights.is_empty() {
                    editor.clear_background_highlights(HighlightKey::HoverState, cx);
                } else {
                    editor.highlight_background(
                        HighlightKey::HoverState,
                        &hover_highlights,
                        |_, theme| theme.colors().element_hover,
                        cx,
                    );
                }

                editor.hover_state.info_popovers = info_popovers;
                cx.notify();
                window.refresh();
            })?;

            anyhow::Ok(())
        }
        .await
        .log_err()
    });

    editor.hover_state.info_task = Some(task);
    None
}

fn same_info_hover(editor: &Editor, snapshot: &MultiBufferSnapshot, anchor: Anchor) -> bool {
    let offset = anchor.to_offset(snapshot);
    editor.hover_state.info_popovers.iter().any(|info_popover| {
        let range = &info_popover.symbol_range;
        (range.start.to_offset(snapshot)..range.end.to_offset(snapshot)).contains(&offset)
    })
}

#[derive(Default)]
pub struct HoverState {
    pub info_popovers: Vec<InfoPopover>,
    pub(crate) info_task: Option<Task<Option<()>>>,
    pub(crate) closest_mouse_distance: Option<Pixels>,
    pub(crate) hiding_delay_task: Option<Task<()>>,
}

impl HoverState {
    pub(crate) fn visible(&self) -> bool {
        !self.info_popovers.is_empty()
    }

    fn is_mouse_getting_closer(&mut self, mouse_position: Point<Pixels>) -> bool {
        if !self.visible() {
            return false;
        }

        let popover_bounds = self
            .info_popovers
            .iter()
            .filter_map(|info_popover| info_popover.last_bounds.get())
            .collect::<Vec<_>>();
        if popover_bounds.is_empty() {
            return false;
        }

        let distance = popover_bounds
            .iter()
            .map(|bounds| Self::distance_from_point_to_bounds(mouse_position, *bounds))
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal))
            .unwrap_or(gpui::px(f32::MAX));

        if let Some(closest_distance) = self.closest_mouse_distance
            && distance > closest_distance + gpui::px(4.0)
        {
            return false;
        }

        self.closest_mouse_distance =
            Some(distance.min(self.closest_mouse_distance.unwrap_or(distance)));
        true
    }

    fn distance_from_point_to_bounds(point: Point<Pixels>, bounds: Bounds<Pixels>) -> Pixels {
        let center_x = bounds.origin.x + bounds.size.width / 2.0;
        let center_y = bounds.origin.y + bounds.size.height / 2.0;
        let dx: f32 = ((point.x - center_x).abs() - bounds.size.width / 2.0)
            .max(gpui::px(0.0))
            .into();
        let dy: f32 = ((point.y - center_y).abs() - bounds.size.height / 2.0)
            .max(gpui::px(0.0))
            .into();
        gpui::px((dx.powi(2) + dy.powi(2)).sqrt())
    }

    pub(crate) fn render(
        &self,
        display_snapshot: &DisplaySnapshot,
        visible_rows: Range<DisplayRow>,
        max_size: Size<Pixels>,
        style: &EditorStyle,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> Option<(DisplayPoint, Vec<AnyElement>)> {
        let info_popover = self.info_popovers.first()?;
        let point = info_popover
            .symbol_range
            .start
            .to_display_point(display_snapshot);
        if !visible_rows.contains(&point.row()) {
            return None;
        }

        let elements = self
            .info_popovers
            .iter()
            .map(|info_popover| info_popover.render(max_size, style, window, cx))
            .collect();
        Some((point, elements))
    }
}

pub struct InfoPopover {
    symbol_range: Range<Anchor>,
    pub hover: project::Hover,
    scroll_handle: ScrollHandle,
    keyboard_grace: Rc<RefCell<bool>>,
    anchor: Anchor,
    pub last_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl InfoPopover {
    fn render(
        &self,
        max_size: Size<Pixels>,
        style: &EditorStyle,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> AnyElement {
        let keyboard_grace = Rc::clone(&self.keyboard_grace);
        let editor = cx.entity().downgrade();
        let bounds_cell = self.last_bounds.clone();
        let theme_colors = cx.theme().colors();
        let variable = self.hover.variable.as_ref().map(|(scope, value)| {
            let value_box = gpui::div()
                .flex()
                .items_center()
                .gap_2()
                .min_h_8()
                .pl_2()
                .pr_1()
                .py_1p5()
                .rounded_md()
                .border_1()
                .border_color(theme_colors.border_variant)
                .bg(theme_colors.editor_background.opacity(0.3))
                .font(style.text.font())
                .line_height(gpui::relative(1.2))
                .map(|this| {
                    if value.is_empty() {
                        this.child(Text::new("Empty").color(Color::Muted))
                    } else {
                        this.child(
                            gpui::div()
                                .flex_1()
                                .min_w_0()
                                .child(Text::new(value.clone())),
                        )
                        .child(
                            gpui::div()
                                .my_neg_0p5()
                                .child(CopyButton::new("hover-copy-value", value.clone())),
                        )
                    }
                });
            let icon = match scope {
                VariableScope::Project => Icon::new(IconAsset::SquareInset)
                    .size(IconSize::XSmall)
                    .color(Color::Muted),
                VariableScope::Environment { color, .. } => {
                    crate::environment_icon(*color, Color::Muted)
                }
                VariableScope::Folder(_) => Icon::new(IconAsset::Folder)
                    .size(IconSize::XSmall)
                    .color(Color::Muted),
            };
            let scope_row = gpui::div()
                .flex()
                .items_center()
                .gap_1p5()
                .child(icon)
                .child(
                    Text::new(variable_provider::scope_label(scope))
                        .size(TextSize::Small)
                        .color(Color::Muted),
                );
            gpui::div()
                .flex()
                .flex_col()
                .gap_2()
                .child(value_box)
                .child(scope_row)
        });

        let content = gpui::div().p_2().map(|this| match variable {
            Some(variable) => this.child(variable),
            None => this.child(Text::new("Not found").color(Color::Muted)),
        });

        gpui::div()
            .id("info_popover")
            .debug_selector(|| "POPOVER-info_popover".to_string())
            .occlude()
            .elevation_2(cx)
            .child(
                gpui::canvas(
                    move |bounds, _, _| {
                        bounds_cell.set(Some(bounds));
                    },
                    |_, (), _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .on_mouse_move(move |_: &MouseMoveEvent, _, cx| {
                if let Err(error) = editor.update(cx, |editor, _| {
                    editor.hover_state.closest_mouse_distance = Some(gpui::px(0.0));
                    editor.hover_state.hiding_delay_task = None;
                }) {
                    log::debug!("Failed to update hover popover state: {error:?}");
                }
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                *keyboard_grace.borrow_mut() = false;
                cx.stop_propagation();
            })
            .child(
                gpui::div()
                    .relative()
                    .child(
                        gpui::div()
                            .id("info-content-container")
                            .overflow_y_scroll()
                            .min_w(
                                gpui::rems(16.0)
                                    .to_pixels(window.rem_size())
                                    .min(max_size.width),
                            )
                            .max_w(max_size.width)
                            .max_h(max_size.height)
                            .track_scroll(&self.scroll_handle)
                            .child(content),
                    )
                    .custom_scrollbars(
                        Scrollbars::new(ScrollAxes::Vertical)
                            .tracked_scroll_handle(&self.scroll_handle)
                            .width_xs(),
                        window,
                        cx,
                    ),
            )
            .into_any_element()
    }
}
