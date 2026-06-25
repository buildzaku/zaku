use gpui::{
    AnyElement, App, Bounds, CursorStyle, DispatchPhase, ElementId, Entity, GlobalElementId,
    Hitbox, HitboxBehavior, Hsla, InspectorElementId, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, Pixels, Point, Style, UniformListDecoration, Window, prelude::*,
};
use smallvec::SmallVec;
use std::{cmp::Ordering, ops::Range, rc::Rc};

use theme::ActiveTheme;

#[derive(Debug, Clone)]
pub struct IndentGuideColors {
    pub default: Hsla,
    pub hover: Hsla,
    pub active: Hsla,
}

impl IndentGuideColors {
    pub fn panel(cx: &App) -> Self {
        Self {
            default: cx.theme().colors().panel_indent_guide,
            hover: cx.theme().colors().panel_indent_guide_hover,
            active: cx.theme().colors().panel_indent_guide_active,
        }
    }
}

pub struct RenderIndentGuideParams {
    pub indent_guides: SmallVec<[IndentGuideLayout; 12]>,
    pub indent_size: Pixels,
    pub item_height: Pixels,
}

pub struct RenderedIndentGuide {
    pub bounds: Bounds<Pixels>,
    pub layout: IndentGuideLayout,
    pub is_active: bool,
    pub hitbox: Option<Bounds<Pixels>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct IndentGuideLayout {
    pub offset: Point<usize>,
    pub length: usize,
    pub continues_offscreen: bool,
}

pub struct IndentGuides {
    colors: IndentGuideColors,
    indent_size: Pixels,
    compute_indents_fn:
        Option<Box<dyn Fn(Range<usize>, &mut Window, &mut App) -> SmallVec<[usize; 64]>>>,
    render_fn: Option<
        Box<
            dyn Fn(
                RenderIndentGuideParams,
                &mut Window,
                &mut App,
            ) -> SmallVec<[RenderedIndentGuide; 12]>,
        >,
    >,
    on_click: Option<Rc<dyn Fn(&IndentGuideLayout, &mut Window, &mut App)>>,
}

impl IndentGuides {
    pub fn on_click(
        mut self,
        on_click: impl Fn(&IndentGuideLayout, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(on_click));
        self
    }

    pub fn with_compute_indents_fn<V: Render>(
        mut self,
        entity: Entity<V>,
        compute_indents_fn: impl Fn(
            &mut V,
            Range<usize>,
            &mut Window,
            &mut Context<V>,
        ) -> SmallVec<[usize; 64]>
        + 'static,
    ) -> Self {
        let compute_indents_fn = Box::new(move |range, window: &mut Window, cx: &mut App| {
            entity.update(cx, |this, cx| compute_indents_fn(this, range, window, cx))
        });
        self.compute_indents_fn = Some(compute_indents_fn);
        self
    }

    pub fn with_render_fn<V: Render>(
        mut self,
        entity: Entity<V>,
        render_fn: impl Fn(
            &mut V,
            RenderIndentGuideParams,
            &mut Window,
            &mut App,
        ) -> SmallVec<[RenderedIndentGuide; 12]>
        + 'static,
    ) -> Self {
        let render_fn = move |params, window: &mut Window, cx: &mut App| {
            entity.update(cx, |this, cx| render_fn(this, params, window, cx))
        };
        self.render_fn = Some(Box::new(render_fn));
        self
    }

    fn render_from_layout(
        &self,
        layouts: SmallVec<[IndentGuideLayout; 12]>,
        bounds: Bounds<Pixels>,
        item_height: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let mut rendered_guides = if let Some(custom_render) = &self.render_fn {
            let params = RenderIndentGuideParams {
                indent_guides: layouts,
                indent_size: self.indent_size,
                item_height,
            };
            custom_render(params, window, cx)
        } else {
            layouts
                .into_iter()
                .map(|layout| {
                    let guide_x = layout.offset.x * self.indent_size;
                    let guide_y = layout.offset.y * item_height;
                    let guide_height = layout.length * item_height;

                    RenderedIndentGuide {
                        bounds: Bounds::new(
                            gpui::point(guide_x, guide_y),
                            gpui::size(gpui::px(1.0), guide_height),
                        ),
                        layout,
                        is_active: false,
                        hitbox: None,
                    }
                })
                .collect()
        };
        for guide in &mut rendered_guides {
            guide.bounds.origin += bounds.origin;
            if let Some(hitbox) = guide.hitbox.as_mut() {
                hitbox.origin += bounds.origin;
            }
        }

        IndentGuidesElement {
            colors: self.colors.clone(),
            indent_guides: Rc::new(rendered_guides),
            on_hovered_indent_guide_click: self.on_click.clone(),
        }
        .into_any_element()
    }
}

impl UniformListDecoration for IndentGuides {
    fn compute(
        &self,
        mut visible_range: Range<usize>,
        bounds: Bounds<Pixels>,
        _scroll_offset: Point<Pixels>,
        item_height: Pixels,
        item_count: usize,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let includes_trailing_indent = visible_range.end < item_count;
        if includes_trailing_indent {
            visible_range.end += 1;
        }
        let Some(compute_indents_fn) = &self.compute_indents_fn else {
            return gpui::div().into_any_element();
        };
        let indents = compute_indents_fn(visible_range.clone(), window, cx);
        let layouts =
            compute_indent_guides(&indents, visible_range.start, includes_trailing_indent);
        self.render_from_layout(layouts, bounds, item_height, window, cx)
    }
}

struct IndentGuidesElement {
    colors: IndentGuideColors,
    indent_guides: Rc<SmallVec<[RenderedIndentGuide; 12]>>,
    on_hovered_indent_guide_click: Option<Rc<dyn Fn(&IndentGuideLayout, &mut Window, &mut App)>>,
}

enum IndentGuidesElementPrepaintState {
    Static,
    Interactive {
        hitboxes: Rc<SmallVec<[Hitbox; 12]>>,
        on_hovered_indent_guide_click: Rc<dyn Fn(&IndentGuideLayout, &mut Window, &mut App)>,
    },
}

impl Element for IndentGuidesElement {
    type RequestLayoutState = ();
    type PrepaintState = IndentGuidesElementPrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (window.request_layout(Style::default(), None, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        if let Some(on_hovered_indent_guide_click) = self.on_hovered_indent_guide_click.clone() {
            let guide_hitboxes = self
                .indent_guides
                .as_ref()
                .iter()
                .map(|guide| {
                    window
                        .insert_hitbox(guide.hitbox.unwrap_or(guide.bounds), HitboxBehavior::Normal)
                })
                .collect();
            Self::PrepaintState::Interactive {
                hitboxes: Rc::new(guide_hitboxes),
                on_hovered_indent_guide_click,
            }
        } else {
            Self::PrepaintState::Static
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        _cx: &mut App,
    ) {
        let current_view = window.current_view();

        match prepaint {
            IndentGuidesElementPrepaintState::Static => {
                for guide in self.indent_guides.as_ref() {
                    let fill_color = if guide.is_active {
                        self.colors.active
                    } else {
                        self.colors.default
                    };

                    window.paint_quad(gpui::fill(
                        window.pixel_snap_bounds(guide.bounds),
                        fill_color,
                    ));
                }
            }
            IndentGuidesElementPrepaintState::Interactive {
                hitboxes: hitbox_list,
                on_hovered_indent_guide_click,
            } => {
                window.on_mouse_event({
                    let hitbox_list = hitbox_list.clone();
                    let guides = self.indent_guides.clone();
                    let on_hovered_indent_guide_click = on_hovered_indent_guide_click.clone();
                    move |event: &MouseDownEvent, phase, window, cx| {
                        if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
                            return;
                        }

                        let active_hitbox = hitbox_list
                            .iter()
                            .enumerate()
                            .find_map(|(index, region)| region.is_hovered(window).then_some(index));
                        let Some(active_hitbox) = active_hitbox else {
                            return;
                        };

                        let active_layout = &guides
                            .get(active_hitbox)
                            .expect("active hitbox should have indent guide")
                            .layout;
                        on_hovered_indent_guide_click(active_layout, window, cx);

                        cx.stop_propagation();
                        window.prevent_default();
                    }
                });

                let mut hovered_region = None;
                for (index, region) in hitbox_list.iter().enumerate() {
                    window.set_cursor_style(CursorStyle::PointingHand, region);
                    let guide = self
                        .indent_guides
                        .get(index)
                        .expect("hitbox should have indent guide");
                    let fill_color = if region.is_hovered(window) {
                        hovered_region = Some(region.id);
                        self.colors.hover
                    } else if guide.is_active {
                        self.colors.active
                    } else {
                        self.colors.default
                    };

                    window.paint_quad(gpui::fill(
                        window.pixel_snap_bounds(guide.bounds),
                        fill_color,
                    ));
                }

                window.on_mouse_event({
                    let previous_region = hovered_region;
                    let hitbox_list = hitbox_list.clone();
                    move |_: &MouseMoveEvent, phase, window, cx| {
                        if phase != DispatchPhase::Capture {
                            return;
                        }

                        let current_region = hitbox_list
                            .as_ref()
                            .iter()
                            .find_map(|region| region.is_hovered(window).then_some(region.id));

                        match (previous_region, current_region) {
                            (Some(previous_id), Some(current_id)) => {
                                if previous_id != current_id {
                                    cx.notify(current_view);
                                }
                            }
                            (None, Some(_)) | (Some(_), None) => cx.notify(current_view),
                            (None, None) => {}
                        }
                    }
                });
            }
        }
    }
}

impl IntoElement for IndentGuidesElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

pub fn indent_guides(indent_size: Pixels, colors: IndentGuideColors) -> IndentGuides {
    IndentGuides {
        colors,
        indent_size,
        compute_indents_fn: None,
        render_fn: None,
        on_click: None,
    }
}

fn compute_indent_guides(
    indents: &[usize],
    offset: usize,
    includes_trailing_indent: bool,
) -> SmallVec<[IndentGuideLayout; 12]> {
    let mut layouts = SmallVec::<[IndentGuideLayout; 12]>::new();
    let mut stack = SmallVec::<[IndentGuideLayout; 8]>::new();

    let mut min_depth = usize::MAX;
    for (row, &depth) in indents.iter().enumerate() {
        if includes_trailing_indent && row == indents.len() - 1 {
            continue;
        }

        let current_row = row + offset;
        let current_depth = stack.len();
        if depth < min_depth {
            min_depth = depth;
        }

        match depth.cmp(&current_depth) {
            Ordering::Less => {
                for _ in 0..(current_depth - depth) {
                    if let Some(layout) = stack.pop() {
                        layouts.push(layout);
                    }
                }
            }
            Ordering::Greater => {
                for new_depth in current_depth..depth {
                    stack.push(IndentGuideLayout {
                        offset: Point::new(new_depth, current_row),
                        length: current_row,
                        continues_offscreen: false,
                    });
                }
            }
            Ordering::Equal => {}
        }

        for layout in &mut stack {
            layout.length = current_row - layout.offset.y + 1;
        }
    }

    layouts.extend(stack);

    for layout in &mut layouts {
        if includes_trailing_indent
            && layout.offset.y + layout.length == offset + indents.len().saturating_sub(1)
        {
            layout.continues_offscreen = indents
                .last()
                .is_some_and(|last_indent| layout.offset.x < *last_indent);
        }
    }

    layouts
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;

    #[test]
    fn test_compute_indent_guides() {
        let assert_indent_guide_layouts =
            |input: &[usize],
             offset: usize,
             includes_trailing_indent: bool,
             expected: Vec<IndentGuideLayout>| {
                assert_eq!(
                    compute_indent_guides(input, offset, includes_trailing_indent)
                        .into_vec()
                        .into_iter()
                        .collect::<HashSet<_>>(),
                    expected.into_iter().collect::<HashSet<_>>(),
                );
            };

        assert_indent_guide_layouts(
            &[0, 1, 2, 2, 1, 0],
            0,
            false,
            vec![
                IndentGuideLayout {
                    offset: Point::new(0, 1),
                    length: 4,
                    continues_offscreen: false,
                },
                IndentGuideLayout {
                    offset: Point::new(1, 2),
                    length: 2,
                    continues_offscreen: false,
                },
            ],
        );

        assert_indent_guide_layouts(
            &[2, 2, 2, 1, 1],
            0,
            false,
            vec![
                IndentGuideLayout {
                    offset: Point::new(0, 0),
                    length: 5,
                    continues_offscreen: false,
                },
                IndentGuideLayout {
                    offset: Point::new(1, 0),
                    length: 3,
                    continues_offscreen: false,
                },
            ],
        );

        assert_indent_guide_layouts(
            &[1, 2, 3, 2, 1],
            0,
            false,
            vec![
                IndentGuideLayout {
                    offset: Point::new(0, 0),
                    length: 5,
                    continues_offscreen: false,
                },
                IndentGuideLayout {
                    offset: Point::new(1, 1),
                    length: 3,
                    continues_offscreen: false,
                },
                IndentGuideLayout {
                    offset: Point::new(2, 2),
                    length: 1,
                    continues_offscreen: false,
                },
            ],
        );

        assert_indent_guide_layouts(
            &[0, 1, 0],
            0,
            true,
            vec![IndentGuideLayout {
                offset: Point::new(0, 1),
                length: 1,
                continues_offscreen: false,
            }],
        );

        assert_indent_guide_layouts(
            &[0, 1, 1],
            0,
            true,
            vec![IndentGuideLayout {
                offset: Point::new(0, 1),
                length: 1,
                continues_offscreen: true,
            }],
        );

        assert_indent_guide_layouts(
            &[0, 1, 2],
            0,
            true,
            vec![IndentGuideLayout {
                offset: Point::new(0, 1),
                length: 1,
                continues_offscreen: true,
            }],
        );
    }
}
