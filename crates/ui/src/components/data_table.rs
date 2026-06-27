pub mod table_row;

pub use table_row::{IntoTableRow, TableRow};

use gpui::{
    AbsoluteLength, AnyElement, App, Context, DefiniteLength, Div, DragMoveEvent, ElementId,
    Entity, EntityId, FocusHandle, Length, ListHorizontalSizingBehavior, ListSizingBehavior,
    ListState, Pixels, Point, RenderOnce, ScrollHandle, SharedString, Stateful,
    UniformListScrollHandle, WeakEntity, Window, prelude::*,
};
use std::{ops::Range, rc::Rc};

use theme::ActiveTheme;

use super::{
    ScrollAxes, ScrollableHandle, Scrollbars, WithScrollbar,
    redistributable_columns::{
        DraggedColumn, HeaderResizeInfo, RESIZE_DIVIDER_WIDTH, RedistributableColumnsState,
        TableResizeBehavior, bind_redistributable_columns, render_column_resize_divider,
        render_redistributable_columns_resize_handles,
    },
};

use crate::StyledTypography;

pub type UncheckedTableRow<T> = Vec<T>;

pub struct ResizableColumnsState {
    initial_widths: TableRow<AbsoluteLength>,
    widths: TableRow<AbsoluteLength>,
    resize_behavior: TableRow<TableResizeBehavior>,
}

impl ResizableColumnsState {
    pub fn new(
        cols: usize,
        initial_widths: Vec<impl Into<AbsoluteLength>>,
        resize_behavior: Vec<TableResizeBehavior>,
    ) -> Self {
        let widths: TableRow<AbsoluteLength> = initial_widths
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>()
            .into_table_row(cols);
        Self {
            initial_widths: widths.clone(),
            widths,
            resize_behavior: resize_behavior.into_table_row(cols),
        }
    }

    pub fn cols(&self) -> usize {
        self.widths.cols()
    }

    pub fn resize_behavior(&self) -> &TableRow<TableResizeBehavior> {
        &self.resize_behavior
    }

    pub(crate) fn on_drag_move(
        &mut self,
        drag_event: &DragMoveEvent<DraggedColumn>,
        horizontal_scroll_offset: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let column_index = drag_event.drag(cx).column_index;
        let drag_x =
            drag_event.event.position.x - drag_event.bounds.left() - horizontal_scroll_offset;
        self.drag_to(column_index, drag_x, window.rem_size());
        cx.notify();
    }

    pub(crate) fn drag_to(&mut self, column_index: usize, drag_x: Pixels, rem_size: Pixels) {
        let width_prefix = self
            .widths
            .as_slice()
            .get(..column_index)
            .expect("table column prefix should exist");
        let left_edge: Pixels = width_prefix
            .iter()
            .map(|width| width.to_pixels(rem_size))
            .fold(gpui::px(0.0), |accumulator, width| accumulator + width);

        let new_width = drag_x - left_edge;
        let resize_behavior = *self.resize_behavior.expect_get(column_index);
        let new_width = self.apply_min_size(new_width, resize_behavior, rem_size);

        *self.widths.expect_get_mut(column_index) = AbsoluteLength::Pixels(new_width);
    }

    pub fn set_column_configuration(
        &mut self,
        column_index: usize,
        width: impl Into<AbsoluteLength>,
        resize_behavior: TableResizeBehavior,
    ) {
        let width = width.into();
        *self.initial_widths.expect_get_mut(column_index) = width;
        *self.widths.expect_get_mut(column_index) = width;
        *self.resize_behavior.expect_get_mut(column_index) = resize_behavior;
    }

    pub fn reset_column_to_initial_width(&mut self, column_index: usize) {
        let initial_width = *self.initial_widths.expect_get(column_index);
        *self.widths.expect_get_mut(column_index) = initial_width;
    }

    fn apply_min_size(
        &self,
        width: Pixels,
        behavior: TableResizeBehavior,
        rem_size: Pixels,
    ) -> Pixels {
        match behavior.min_size() {
            Some(min_rems) => {
                let min_pixels = rem_size * min_rems;
                width.max(min_pixels)
            }
            None => width,
        }
    }
}

struct UniformListData {
    render_list_of_rows_fn:
        Box<dyn Fn(Range<usize>, &mut Window, &mut App) -> Vec<UncheckedTableRow<AnyElement>>>,
    element_id: ElementId,
    row_count: usize,
}

struct VariableRowHeightListData {
    render_row_fn: Box<dyn Fn(usize, &mut Window, &mut App) -> UncheckedTableRow<AnyElement>>,
    list_state: ListState,
    row_count: usize,
}

enum TableContents {
    Vec(Vec<TableRow<AnyElement>>),
    UniformList(UniformListData),
    VariableRowHeightList(VariableRowHeightListData),
}

impl TableContents {
    fn rows_mut(&mut self) -> Option<&mut Vec<TableRow<AnyElement>>> {
        match self {
            TableContents::Vec(rows) => Some(rows),
            TableContents::UniformList(_) | TableContents::VariableRowHeightList(_) => None,
        }
    }

    fn len(&self) -> usize {
        match self {
            TableContents::Vec(rows) => rows.len(),
            TableContents::UniformList(data) => data.row_count,
            TableContents::VariableRowHeightList(data) => data.row_count,
        }
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub struct TableInteractionState {
    pub focus_handle: FocusHandle,
    pub scroll_handle: UniformListScrollHandle,
    pub horizontal_scroll_handle: ScrollHandle,
    pub custom_scrollbar: Option<Scrollbars>,
}

impl TableInteractionState {
    pub fn new(cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            scroll_handle: UniformListScrollHandle::new(),
            horizontal_scroll_handle: ScrollHandle::new(),
            custom_scrollbar: None,
        }
    }

    pub fn with_custom_scrollbar(mut self, custom_scrollbar: Scrollbars) -> Self {
        self.custom_scrollbar = Some(custom_scrollbar);
        self
    }

    pub fn scroll_offset(&self) -> Point<Pixels> {
        self.scroll_handle.offset()
    }

    pub fn set_scroll_offset(&self, offset: Point<Pixels>) {
        self.scroll_handle.set_offset(offset);
    }

    pub fn listener<E: ?Sized>(
        this: &Entity<Self>,
        handler: impl Fn(&mut Self, &E, &mut Window, &mut Context<Self>) + 'static,
    ) -> impl Fn(&E, &mut Window, &mut App) + 'static {
        let view = this.downgrade();
        move |event: &E, window: &mut Window, cx: &mut App| {
            if let Err(error) = view.update(cx, |view, cx| handler(view, event, window, cx)) {
                log::trace!("Failed to handle table interaction state event: {error:?}");
            }
        }
    }
}

pub enum StaticColumnWidths {
    Auto,
    Explicit(TableRow<DefiniteLength>),
}

pub enum ColumnWidthConfig {
    Static {
        widths: StaticColumnWidths,
        table_width: Option<DefiniteLength>,
    },
    Redistributable {
        columns_state: Entity<RedistributableColumnsState>,
        table_width: Option<DefiniteLength>,
    },
    Resizable(Entity<ResizableColumnsState>),
}

impl ColumnWidthConfig {
    pub fn auto() -> Self {
        ColumnWidthConfig::Static {
            widths: StaticColumnWidths::Auto,
            table_width: None,
        }
    }

    pub fn redistributable(columns_state: Entity<RedistributableColumnsState>) -> Self {
        ColumnWidthConfig::Redistributable {
            columns_state,
            table_width: None,
        }
    }

    pub fn auto_with_table_width(width: impl Into<DefiniteLength>) -> Self {
        ColumnWidthConfig::Static {
            widths: StaticColumnWidths::Auto,
            table_width: Some(width.into()),
        }
    }

    pub fn explicit<T: Into<DefiniteLength>>(widths: Vec<T>) -> Self {
        let cols = widths.len();
        ColumnWidthConfig::Static {
            widths: StaticColumnWidths::Explicit(
                widths
                    .into_iter()
                    .map(Into::into)
                    .collect::<Vec<_>>()
                    .into_table_row(cols),
            ),
            table_width: None,
        }
    }

    pub fn widths_to_render(&self, cx: &App) -> Option<TableRow<Length>> {
        match self {
            ColumnWidthConfig::Static {
                widths: StaticColumnWidths::Auto,
                ..
            } => None,
            ColumnWidthConfig::Static {
                widths: StaticColumnWidths::Explicit(widths),
                ..
            } => Some(widths.map_cloned(Length::Definite)),
            ColumnWidthConfig::Redistributable {
                columns_state: entity,
                ..
            } => Some(entity.read(cx).widths_to_render()),
            ColumnWidthConfig::Resizable(entity) => {
                let state = entity.read(cx);
                Some(state.widths.map_cloned(|absolute_length| {
                    Length::Definite(DefiniteLength::Absolute(absolute_length))
                }))
            }
        }
    }

    pub fn table_width(&self, window: &Window, cx: &App) -> Option<Length> {
        match self {
            ColumnWidthConfig::Static { table_width, .. }
            | ColumnWidthConfig::Redistributable { table_width, .. } => {
                table_width.map(Length::Definite)
            }
            ColumnWidthConfig::Resizable(entity) => {
                let state = entity.read(cx);
                let rem_size = window.rem_size();
                let total: Pixels = state
                    .widths
                    .as_slice()
                    .iter()
                    .map(|absolute_length| absolute_length.to_pixels(rem_size))
                    .fold(gpui::px(0.0), |accumulator, width| accumulator + width);
                Some(Length::Definite(DefiniteLength::Absolute(
                    AbsoluteLength::Pixels(total),
                )))
            }
        }
    }

    pub fn list_horizontal_sizing(
        &self,
        window: &Window,
        cx: &App,
    ) -> ListHorizontalSizingBehavior {
        match self {
            ColumnWidthConfig::Resizable(_) => ListHorizontalSizingBehavior::FitList,
            _ => match self.table_width(window, cx) {
                Some(_) => ListHorizontalSizingBehavior::Unconstrained,
                None => ListHorizontalSizingBehavior::FitList,
            },
        }
    }
}

#[derive(IntoElement)]
pub struct Table {
    striped: bool,
    show_row_borders: bool,
    show_row_hover: bool,
    headers: Option<TableRow<AnyElement>>,
    rows: TableContents,
    interaction_state: Option<WeakEntity<TableInteractionState>>,
    column_width_config: ColumnWidthConfig,
    map_row: Option<Rc<dyn Fn((usize, Stateful<Div>), &mut Window, &mut App) -> AnyElement>>,
    use_ui_font: bool,
    empty_table_callback: Option<Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>>,
    cols: usize,
    disable_base_cell_style: bool,
}

impl Table {
    pub fn new(cols: usize) -> Self {
        Self {
            cols,
            striped: false,
            show_row_borders: true,
            show_row_hover: true,
            headers: None,
            rows: TableContents::Vec(Vec::new()),
            interaction_state: None,
            map_row: None,
            use_ui_font: true,
            empty_table_callback: None,
            disable_base_cell_style: false,
            column_width_config: ColumnWidthConfig::auto(),
        }
    }

    pub fn disable_base_style(mut self) -> Self {
        self.disable_base_cell_style = true;
        self
    }

    pub fn uniform_list(
        mut self,
        id: impl Into<ElementId>,
        row_count: usize,
        render_item_fn: impl Fn(
            Range<usize>,
            &mut Window,
            &mut App,
        ) -> Vec<UncheckedTableRow<AnyElement>>
        + 'static,
    ) -> Self {
        self.rows = TableContents::UniformList(UniformListData {
            element_id: id.into(),
            row_count,
            render_list_of_rows_fn: Box::new(render_item_fn),
        });
        self
    }

    pub fn variable_row_height_list(
        mut self,
        row_count: usize,
        list_state: ListState,
        render_row_fn: impl Fn(usize, &mut Window, &mut App) -> UncheckedTableRow<AnyElement> + 'static,
    ) -> Self {
        self.rows = TableContents::VariableRowHeightList(VariableRowHeightListData {
            render_row_fn: Box::new(render_row_fn),
            list_state,
            row_count,
        });
        self
    }

    pub fn striped(mut self) -> Self {
        self.striped = true;
        self
    }

    pub fn hide_row_borders(mut self) -> Self {
        self.show_row_borders = false;
        self
    }

    pub fn width(mut self, width: impl Into<DefiniteLength>) -> Self {
        self.column_width_config = ColumnWidthConfig::auto_with_table_width(width);
        self
    }

    pub fn width_config(mut self, config: ColumnWidthConfig) -> Self {
        self.column_width_config = config;
        self
    }

    pub fn interactable(mut self, interaction_state: &Entity<TableInteractionState>) -> Self {
        self.interaction_state = Some(interaction_state.downgrade());
        self
    }

    pub fn header(mut self, headers: UncheckedTableRow<impl IntoElement>) -> Self {
        self.headers = Some(
            headers
                .into_table_row(self.cols)
                .map(IntoElement::into_any_element),
        );
        self
    }

    pub fn row(mut self, items: UncheckedTableRow<impl IntoElement>) -> Self {
        if let Some(rows) = self.rows.rows_mut() {
            rows.push(
                items
                    .into_table_row(self.cols)
                    .map(IntoElement::into_any_element),
            );
        }
        self
    }

    pub fn no_ui_font(mut self) -> Self {
        self.use_ui_font = false;
        self
    }

    pub fn map_row(
        mut self,
        callback: impl Fn((usize, Stateful<Div>), &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.map_row = Some(Rc::new(callback));
        self
    }

    pub fn hide_row_hover(mut self) -> Self {
        self.show_row_hover = false;
        self
    }

    pub fn empty_table_callback(
        mut self,
        callback: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.empty_table_callback = Some(Rc::new(callback));
        self
    }
}

fn base_cell_style(width: Option<Length>) -> Div {
    gpui::div()
        .px_1p5()
        .when_some(width, |this, width| this.w(width))
        .when(width.is_none(), |this| this.flex_1())
        .whitespace_nowrap()
        .text_ellipsis()
        .overflow_hidden()
}

fn base_cell_style_text(width: Option<Length>, use_ui_font: bool, cx: &App) -> Div {
    base_cell_style(width).when(use_ui_font, |element| element.text_ui(cx))
}

fn render_cell(width: Option<Length>, cell: AnyElement, ctx: &TableRenderContext, cx: &App) -> Div {
    if ctx.disable_base_cell_style {
        gpui::div()
            .when_some(width, |this, width| this.w(width))
            .when(width.is_none(), |this| this.flex_1())
            .overflow_hidden()
            .child(cell)
    } else {
        base_cell_style_text(width, ctx.use_ui_font, cx)
            .px_1()
            .py_0p5()
            .child(cell)
    }
}

fn render_header_cell(
    header: AnyElement,
    width: Option<Length>,
    header_index: usize,
    shared_element_id: &SharedString,
    resize_info: Option<&HeaderResizeInfo>,
    use_ui_font: bool,
    cx: &App,
) -> Stateful<Div> {
    base_cell_style_text(width, use_ui_font, cx)
        .px_1()
        .py_0p5()
        .child(header)
        .id(ElementId::NamedInteger(
            shared_element_id.clone(),
            header_index as u64,
        ))
        .when_some(resize_info.cloned(), |this, info| {
            if info.resize_behavior.expect_get(header_index).is_resizable() {
                this.on_click(move |event, window, cx| {
                    if event.click_count() > 1 {
                        info.reset_column(header_index, window, cx);
                    }
                })
            } else {
                this
            }
        })
}

pub fn render_table_row(
    row_index: usize,
    items: TableRow<impl IntoElement>,
    table_context: TableRenderContext,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let is_striped = table_context.striped;
    let is_last = row_index == table_context.total_row_count - 1;
    let background = if row_index % 2 == 1 && is_striped {
        Some(cx.theme().colors().text.opacity(0.05))
    } else {
        None
    };
    let columns = items.cols();
    let column_widths = match &table_context.column_widths {
        Some(widths) => widths.clone().map(Some),
        None => vec![None; columns].into_table_row(columns),
    };

    let row = gpui::div()
        .flex()
        .flex_row()
        .id(("table_row", row_index))
        .size_full()
        .when_some(background, |row, background| row.bg(background))
        .when(table_context.show_row_hover, |row| {
            row.hover(|style| style.bg(cx.theme().colors().element_hover.opacity(0.6)))
        })
        .when(!is_striped && table_context.show_row_borders, |row| {
            row.border_b_1()
                .border_color(gpui::transparent_black())
                .when(!is_last, |row| row.border_color(cx.theme().colors().border))
        })
        .children(
            items
                .map(IntoElement::into_any_element)
                .into_vec()
                .into_iter()
                .zip(column_widths.into_vec())
                .map(|(cell, width)| render_cell(width, cell, &table_context, cx)),
        );

    let row = if let Some(map_row) = table_context.map_row {
        map_row((row_index, row), window, cx)
    } else {
        row.into_any_element()
    };

    gpui::div().size_full().child(row).into_any_element()
}

pub fn render_table_header(
    headers: TableRow<impl IntoElement>,
    table_context: TableRenderContext,
    resize_info: Option<&HeaderResizeInfo>,
    entity_id: Option<EntityId>,
    cx: &mut App,
) -> AnyElement {
    let columns = headers.cols();
    let column_widths = table_context
        .column_widths
        .map_or(vec![None; columns].into_table_row(columns), |widths| {
            widths.map(Some)
        });

    let element_id = entity_id
        .map(|entity| entity.to_string())
        .unwrap_or_default();

    let shared_element_id: SharedString = format!("table-{element_id}").into();
    let use_ui_font = table_context.use_ui_font;
    let resize_info_ref = resize_info;

    gpui::div()
        .flex()
        .flex_row()
        .items_center()
        .w_full()
        .border_b_1()
        .border_color(cx.theme().colors().border)
        .children(
            headers
                .into_vec()
                .into_iter()
                .enumerate()
                .zip(column_widths.into_vec())
                .map(|((header_index, header), width)| {
                    render_header_cell(
                        header.into_any_element(),
                        width,
                        header_index,
                        &shared_element_id,
                        resize_info_ref,
                        use_ui_font,
                        cx,
                    )
                }),
        )
        .into_any_element()
}

#[derive(Clone)]
pub struct TableRenderContext {
    pub striped: bool,
    pub show_row_borders: bool,
    pub show_row_hover: bool,
    pub total_row_count: usize,
    pub column_widths: Option<TableRow<Length>>,
    pub map_row: Option<Rc<dyn Fn((usize, Stateful<Div>), &mut Window, &mut App) -> AnyElement>>,
    pub use_ui_font: bool,
    pub disable_base_cell_style: bool,
}

impl TableRenderContext {
    fn new(table: &Table, cx: &App) -> Self {
        Self {
            striped: table.striped,
            show_row_borders: table.show_row_borders,
            show_row_hover: table.show_row_hover,
            total_row_count: table.rows.len(),
            column_widths: table.column_width_config.widths_to_render(cx),
            map_row: table.map_row.clone(),
            use_ui_font: table.use_ui_font,
            disable_base_cell_style: table.disable_base_cell_style,
        }
    }

    pub fn for_column_widths(column_widths: Option<TableRow<Length>>, use_ui_font: bool) -> Self {
        Self {
            striped: false,
            show_row_borders: true,
            show_row_hover: true,
            total_row_count: 0,
            column_widths,
            map_row: None,
            use_ui_font,
            disable_base_cell_style: false,
        }
    }
}

fn build_resize_dividers(
    columns_state: &Entity<ResizableColumnsState>,
    widths: &TableRow<AbsoluteLength>,
    resize_behavior: &TableRow<TableResizeBehavior>,
    range: Range<usize>,
    interactive: bool,
    rem_size: Pixels,
    window: &mut Window,
    cx: &mut App,
) -> Vec<AnyElement> {
    let entity_id = columns_state.entity_id();
    let last = range.end.saturating_sub(1);
    let mut dividers = Vec::with_capacity(range.end - range.start);
    let mut accumulated = gpui::px(0.0);

    for column_index in range {
        accumulated += widths.expect_get(column_index).to_pixels(rem_size);

        let divider_left = if column_index == last {
            accumulated - gpui::px(RESIZE_DIVIDER_WIDTH)
        } else {
            accumulated
        };
        let divider = gpui::div()
            .id(column_index)
            .absolute()
            .top_0()
            .left(divider_left);
        let on_reset: Rc<dyn Fn(&mut Window, &mut App)> = {
            let columns_state = columns_state.clone();
            Rc::new(move |_window, cx| {
                columns_state.update(cx, |state, cx| {
                    state.reset_column_to_initial_width(column_index);
                    cx.notify();
                });
            })
        };
        let is_resizable = interactive && resize_behavior[column_index].is_resizable();
        dividers.push(render_column_resize_divider(
            divider,
            column_index,
            is_resizable,
            entity_id,
            on_reset,
            None,
            window,
            cx,
        ));
    }
    dividers
}

fn render_resize_handles_resizable(
    columns_state: &Entity<ResizableColumnsState>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let state = columns_state.read(cx);
    let widths = state.widths.clone();
    let resize_behavior = state.resize_behavior().clone();

    let rem_size = window.rem_size();
    let columns = widths.cols();
    let dividers = build_resize_dividers(
        columns_state,
        &widths,
        &resize_behavior,
        0..columns,
        true,
        rem_size,
        window,
        cx,
    );

    gpui::div()
        .id("resize-handles")
        .absolute()
        .inset_0()
        .w_full()
        .children(dividers)
        .into_any_element()
}

impl RenderOnce for Table {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let interaction_state = self
            .interaction_state
            .clone()
            .and_then(|state| state.upgrade());

        let table_context = TableRenderContext::new(&self, cx);

        let header_resize_info =
            interaction_state
                .as_ref()
                .and_then(|_| match &self.column_width_config {
                    ColumnWidthConfig::Redistributable { columns_state, .. } => {
                        Some(HeaderResizeInfo::from_redistributable(columns_state, cx))
                    }
                    ColumnWidthConfig::Resizable(entity) => {
                        Some(HeaderResizeInfo::from_resizable(entity, cx))
                    }
                    ColumnWidthConfig::Static { .. } => None,
                });

        let table_width = self.column_width_config.table_width(window, cx);
        let horizontal_sizing = self.column_width_config.list_horizontal_sizing(window, cx);

        let no_rows_rendered = self.rows.is_empty();
        let variable_list_state = if let TableContents::VariableRowHeightList(data) = &self.rows {
            Some(data.list_state.clone())
        } else {
            None
        };

        let (redistributable_entity, resizable_entity, resize_handles) =
            if interaction_state.is_some() {
                match &self.column_width_config {
                    ColumnWidthConfig::Redistributable { columns_state, .. } => (
                        Some(columns_state.clone()),
                        None,
                        Some(render_redistributable_columns_resize_handles(
                            columns_state,
                            window,
                            cx,
                        )),
                    ),
                    ColumnWidthConfig::Resizable(entity) => (
                        None,
                        Some(entity.clone()),
                        Some(render_resize_handles_resizable(entity, window, cx)),
                    ),
                    ColumnWidthConfig::Static { .. } => (None, None, None),
                }
            } else {
                (None, None, None)
            };

        let is_resizable = resizable_entity.is_some();

        let table = gpui::div()
            .when_some(table_width, |this, width| this.w(width))
            .h_full()
            .flex()
            .flex_col()
            .when_some(self.headers.take(), |this, headers| {
                this.child(render_table_header(
                    headers,
                    table_context.clone(),
                    header_resize_info.as_ref(),
                    interaction_state.as_ref().map(Entity::entity_id),
                    cx,
                ))
            })
            .when_some(redistributable_entity, |this, widths| {
                bind_redistributable_columns(this, widths)
            })
            .when_some(resizable_entity, |this, entity| {
                this.on_drag_move::<DraggedColumn>(move |event, window, cx| {
                    if event.drag(cx).state_id != entity.entity_id() {
                        return;
                    }
                    entity.update(cx, |state, cx| {
                        state.on_drag_move(event, gpui::px(0.0), window, cx);
                    });
                })
            })
            .child({
                let content = gpui::div()
                    .flex_grow_1()
                    .w_full()
                    .relative()
                    .overflow_hidden()
                    .map(|parent| match self.rows {
                        TableContents::Vec(items) => {
                            parent.children(items.into_iter().enumerate().map(|(index, row)| {
                                gpui::div().child(render_table_row(
                                    index,
                                    row,
                                    table_context.clone(),
                                    window,
                                    cx,
                                ))
                            }))
                        }
                        TableContents::UniformList(uniform_list_data) => parent.child(
                            gpui::uniform_list(
                                uniform_list_data.element_id,
                                uniform_list_data.row_count,
                                {
                                    let render_item_fn = uniform_list_data.render_list_of_rows_fn;
                                    move |range: Range<usize>, window, cx| {
                                        let elements = render_item_fn(range.clone(), window, cx)
                                            .into_iter()
                                            .map(|raw_row| raw_row.into_table_row(self.cols))
                                            .collect::<Vec<_>>();
                                        elements
                                            .into_iter()
                                            .zip(range)
                                            .map(|(row, row_index)| {
                                                render_table_row(
                                                    row_index,
                                                    row,
                                                    table_context.clone(),
                                                    window,
                                                    cx,
                                                )
                                            })
                                            .collect()
                                    }
                                },
                            )
                            .size_full()
                            .flex_grow_1()
                            .with_sizing_behavior(ListSizingBehavior::Auto)
                            .with_horizontal_sizing_behavior(horizontal_sizing)
                            .when_some(
                                interaction_state.as_ref(),
                                |this, state| {
                                    this.track_scroll(
                                        &state
                                            .read_with(cx, |state, _| state.scroll_handle.clone()),
                                    )
                                },
                            ),
                        ),
                        TableContents::VariableRowHeightList(variable_list_data) => parent.child(
                            gpui::list(variable_list_data.list_state.clone(), {
                                let render_item_fn = variable_list_data.render_row_fn;
                                move |row_index: usize, window: &mut Window, cx: &mut App| {
                                    let row = render_item_fn(row_index, window, cx)
                                        .into_table_row(self.cols);
                                    render_table_row(
                                        row_index,
                                        row,
                                        table_context.clone(),
                                        window,
                                        cx,
                                    )
                                }
                            })
                            .size_full()
                            .flex_grow_1()
                            .with_sizing_behavior(ListSizingBehavior::Auto),
                        ),
                    })
                    .when_some(resize_handles, |parent, handles| parent.child(handles));

                content.into_any_element()
            })
            .when_some(
                no_rows_rendered
                    .then_some(self.empty_table_callback)
                    .flatten(),
                |this, callback| {
                    this.child(
                        gpui::div()
                            .flex()
                            .flex_row()
                            .size_full()
                            .p_3()
                            .items_start()
                            .justify_center()
                            .child(callback(window, cx)),
                    )
                },
            );

        if let Some(state) = interaction_state.as_ref() {
            let content = if is_resizable {
                let mut horizontal_scroll_container = gpui::div()
                    .id("table-horizontal-scroll")
                    .overflow_x_scroll()
                    .flex_grow_1()
                    .h_full()
                    .track_scroll(&state.read(cx).horizontal_scroll_handle)
                    .child(table);
                horizontal_scroll_container.style().restrict_scroll_to_axis = Some(true);
                gpui::div().size_full().child(horizontal_scroll_container)
            } else {
                table
            };

            let scrollbars = state
                .read(cx)
                .custom_scrollbar
                .clone()
                .unwrap_or_else(|| Scrollbars::new(ScrollAxes::Both));
            let mut content = if let Some(list_state) = variable_list_state {
                content.custom_scrollbars(scrollbars.tracked_scroll_handle(&list_state), window, cx)
            } else {
                content.custom_scrollbars(
                    scrollbars.tracked_scroll_handle(&state.read(cx).scroll_handle),
                    window,
                    cx,
                )
            };

            if is_resizable {
                content = content.custom_scrollbars(
                    Scrollbars::new(ScrollAxes::Horizontal)
                        .tracked_scroll_handle(&state.read(cx).horizontal_scroll_handle),
                    window,
                    cx,
                );
            }
            content.style().restrict_scroll_to_axis = Some(true);

            content
                .track_focus(&state.read(cx).focus_handle)
                .id(("table", state.entity_id()))
                .into_any_element()
        } else {
            table.into_any_element()
        }
    }
}
