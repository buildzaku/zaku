pub mod table_row;

pub use table_row::{IntoTableRow, TableRow};

use gpui::{
    AbsoluteLength, Anchor, AnyElement, App, Bounds, ClipboardItem, Context, CursorStyle,
    DefiniteLength, DismissEvent, Div, DragMoveEvent, Element, ElementId, Entity, EntityId,
    FocusHandle, Focusable, FontWeight, GlobalElementId, Hitbox, InspectorElementId, LayoutId,
    Length, ListHorizontalSizingBehavior, ListSizingBehavior, ListState, MouseButton, Pixels,
    Point, RenderOnce, ScrollHandle, SharedString, Stateful, StyledText, Subscription,
    UniformListScrollHandle, WeakEntity, Window, prelude::*,
};
use std::{ops::Range, rc::Rc};

use theme::{ActiveTheme, ThemeSettings};

use super::{
    ScrollAxes, ScrollableHandle, Scrollbars, WithScrollbar,
    redistributable_columns::{
        DraggedColumn, HeaderResizeInfo, RESIZE_DIVIDER_WIDTH, RedistributableColumnsState,
        TableResizeBehavior, bind_redistributable_columns, render_column_resize_divider,
        render_redistributable_columns_resize_handles,
    },
};

use crate::{
    Color, ContextMenu, LineHeightStyle, StyledTypography, TextSelectionPoint, TextSelectionState,
    TextSize, insert_text_hitboxes, paint_text_selection,
};

pub type UncheckedTableRow<T> = Vec<T>;

pub trait IntoTableCell {
    fn into_table_cell(self) -> TableCell;
}

#[derive(Clone)]
pub struct TableTextCell {
    text: SharedString,
    size: TextSize,
    color: Color,
    alpha: Option<f32>,
    weight: Option<FontWeight>,
    line_height_style: LineHeightStyle,
}

impl TableTextCell {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            size: TextSize::Default,
            color: Color::Default,
            alpha: None,
            weight: None,
            line_height_style: LineHeightStyle::default(),
        }
    }

    pub fn size(mut self, size: TextSize) -> Self {
        self.size = size;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn alpha(mut self, alpha: f32) -> Self {
        self.alpha = Some(alpha);
        self
    }

    pub fn weight(mut self, weight: FontWeight) -> Self {
        self.weight = Some(weight);
        self
    }

    pub fn line_height_style(mut self, line_height_style: LineHeightStyle) -> Self {
        self.line_height_style = line_height_style;
        self
    }
}

impl IntoTableCell for TableTextCell {
    fn into_table_cell(self) -> TableCell {
        TableCell::Text(self)
    }
}

impl FluentBuilder for TableTextCell {}

pub enum TableCell {
    Element(AnyElement),
    Text(TableTextCell),
}

impl TableCell {
    pub fn text(text: impl Into<SharedString>) -> TableTextCell {
        TableTextCell::new(text)
    }
}

impl IntoTableCell for TableCell {
    fn into_table_cell(self) -> TableCell {
        self
    }
}

impl<T: IntoElement> IntoTableCell for T {
    fn into_table_cell(self) -> TableCell {
        TableCell::Element(self.into_any_element())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct TableTextId {
    row_index: usize,
    column_index: usize,
}

impl TableTextId {
    fn new(row_index: usize, column_index: usize) -> Self {
        Self {
            row_index,
            column_index,
        }
    }
}

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
        Box<dyn Fn(Range<usize>, &mut Window, &mut App) -> Vec<UncheckedTableRow<TableCell>>>,
    element_id: ElementId,
    row_count: usize,
}

struct VariableRowHeightListData {
    render_row_fn: Box<dyn Fn(usize, &mut Window, &mut App) -> UncheckedTableRow<TableCell>>,
    list_state: ListState,
    row_count: usize,
}

enum TableContents {
    Vec(Vec<TableRow<TableCell>>),
    UniformList(UniformListData),
    VariableRowHeightList(VariableRowHeightListData),
}

impl TableContents {
    fn rows_mut(&mut self) -> Option<&mut Vec<TableRow<TableCell>>> {
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
    text_selection: TextSelectionState<TableTextId>,
    context_menu: Option<(Entity<ContextMenu>, Point<Pixels>, Subscription)>,
}

impl TableInteractionState {
    pub fn new(cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            scroll_handle: UniformListScrollHandle::new(),
            horizontal_scroll_handle: ScrollHandle::new(),
            custom_scrollbar: None,
            text_selection: TextSelectionState::new(),
            context_menu: None,
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

    pub fn clear_text_selection(&mut self) {
        self.text_selection.clear();
        self.context_menu.take();
    }

    fn clear_text_layouts(&mut self) {
        self.text_selection.clear_layouts();
    }

    fn selected_range_for_cell(
        &self,
        row_index: usize,
        column_index: usize,
        text: &str,
    ) -> Option<Range<usize>> {
        self.text_selection
            .selected_range_for_id(TableTextId::new(row_index, column_index), text)
    }

    fn begin_text_selection(
        &mut self,
        row_index: usize,
        column_index: usize,
        position: Point<Pixels>,
        click_count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        self.context_menu.take();

        if self.text_selection.begin_selection(
            TableTextId::new(row_index, column_index),
            position,
            click_count,
        ) {
            cx.notify();
        }
    }

    fn update_text_selection(
        &mut self,
        row_index: usize,
        column_index: usize,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self
            .text_selection
            .update_selection(TableTextId::new(row_index, column_index), position)
        {
            cx.notify();
        }
    }

    fn end_text_selection(
        &mut self,
        row_index: usize,
        column_index: usize,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self
            .text_selection
            .end_selection(TableTextId::new(row_index, column_index), position)
        {
            cx.notify();
        }
    }

    fn selected_text(
        &self,
        row_count: usize,
        column_count: usize,
        text_for_selection: &dyn Fn(usize, usize, &mut Window, &mut App) -> Option<SharedString>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<String> {
        if !self.text_selection.has_non_empty_selection() {
            return None;
        }

        let mut selected_rows = Vec::new();

        for row_index in 0..row_count {
            let mut selected_cells = Vec::new();
            for column_index in 0..column_count {
                let Some(text) = text_for_selection(row_index, column_index, window, cx) else {
                    continue;
                };
                let text: &str = text.as_ref();
                let Some(range) = self
                    .text_selection
                    .selected_range_for_id(TableTextId::new(row_index, column_index), text)
                else {
                    continue;
                };

                if let Some(selected_text) = text.get(range) {
                    selected_cells.push(selected_text.to_string());
                }
            }

            if !selected_cells.is_empty() {
                selected_rows.push(selected_cells.join("\t"));
            }
        }

        let selected_text = selected_rows.join("\n");
        (!selected_text.is_empty()).then_some(selected_text)
    }

    fn copy_selected_text(
        &mut self,
        row_count: usize,
        column_count: usize,
        text_for_selection: &dyn Fn(usize, usize, &mut Window, &mut App) -> Option<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selected_text) =
            self.selected_text(row_count, column_count, text_for_selection, window, cx)
        else {
            return;
        };

        cx.write_to_clipboard(ClipboardItem::new_string(selected_text));
    }

    fn select_all_text(&mut self, row_count: usize, cx: &mut Context<Self>) {
        if row_count == 0 || !self.text_selection.has_registered_layouts() {
            return;
        }

        self.text_selection.select_all(
            TextSelectionPoint::new(TableTextId::new(0, 0), 0),
            TextSelectionPoint::new(TableTextId::new(row_count, 0), 0),
        );
        cx.notify();
    }

    fn deploy_text_context_menu(
        &mut self,
        row_count: usize,
        column_count: usize,
        text_for_selection: &dyn Fn(usize, usize, &mut Window, &mut App) -> Option<SharedString>,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);

        let has_selected_text = self
            .selected_text(row_count, column_count, text_for_selection, window, cx)
            .is_some();
        let focus_handle = self.focus_handle.clone();
        let context_menu = ContextMenu::build(window, cx, move |menu, _, _| {
            menu.context(focus_handle)
                .action_disabled_when(!has_selected_text, "Copy", Box::new(actions::editor::Copy))
                .action("Select All", Box::new(actions::editor::SelectAll))
        });

        window.focus(&context_menu.focus_handle(cx), cx);
        let subscription = cx.subscribe(&context_menu, |this, _, _: &DismissEvent, cx| {
            this.context_menu.take();
            cx.notify();
        });
        self.context_menu = Some((context_menu, position, subscription));
        cx.notify();
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
    column_count: usize,
    rows: TableContents,
    header: Option<TableRow<AnyElement>>,
    column_width_config: ColumnWidthConfig,
    interaction_state: Option<WeakEntity<TableInteractionState>>,
    row_mapper: Option<Rc<dyn Fn((usize, Stateful<Div>), &mut Window, &mut App) -> AnyElement>>,
    text_for_selection:
        Option<Rc<dyn Fn(usize, usize, &mut Window, &mut App) -> Option<SharedString>>>,
    empty_table_callback: Option<Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>>,
    striped: bool,
    show_row_borders: bool,
    show_row_hover: bool,
    use_ui_font: bool,
    disable_base_cell_style: bool,
}

impl Table {
    pub fn new(column_count: usize) -> Self {
        Self {
            column_count,
            rows: TableContents::Vec(Vec::new()),
            header: None,
            column_width_config: ColumnWidthConfig::auto(),
            interaction_state: None,
            row_mapper: None,
            text_for_selection: None,
            empty_table_callback: None,
            striped: false,
            show_row_borders: true,
            show_row_hover: true,
            use_ui_font: true,
            disable_base_cell_style: false,
        }
    }

    pub fn disable_base_style(mut self) -> Self {
        self.disable_base_cell_style = true;
        self
    }

    pub fn uniform_list<T>(
        mut self,
        id: impl Into<ElementId>,
        row_count: usize,
        render_item_fn: impl Fn(Range<usize>, &mut Window, &mut App) -> Vec<UncheckedTableRow<T>>
        + 'static,
    ) -> Self
    where
        T: IntoTableCell + 'static,
    {
        self.rows = TableContents::UniformList(UniformListData {
            element_id: id.into(),
            row_count,
            render_list_of_rows_fn: Box::new(move |range, window, cx| {
                render_item_fn(range, window, cx)
                    .into_iter()
                    .map(|row| {
                        row.into_iter()
                            .map(IntoTableCell::into_table_cell)
                            .collect()
                    })
                    .collect()
            }),
        });
        self
    }

    pub fn variable_row_height_list<T>(
        mut self,
        row_count: usize,
        list_state: ListState,
        render_row_fn: impl Fn(usize, &mut Window, &mut App) -> UncheckedTableRow<T> + 'static,
    ) -> Self
    where
        T: IntoTableCell + 'static,
    {
        self.rows = TableContents::VariableRowHeightList(VariableRowHeightListData {
            render_row_fn: Box::new(move |row_index, window, cx| {
                render_row_fn(row_index, window, cx)
                    .into_iter()
                    .map(IntoTableCell::into_table_cell)
                    .collect()
            }),
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

    pub fn header(mut self, header: UncheckedTableRow<impl IntoElement>) -> Self {
        self.header = Some(
            header
                .into_table_row(self.column_count)
                .map(IntoElement::into_any_element),
        );
        self
    }

    pub fn row(mut self, items: UncheckedTableRow<impl IntoTableCell>) -> Self {
        if let Some(rows) = self.rows.rows_mut() {
            rows.push(
                items
                    .into_table_row(self.column_count)
                    .map(IntoTableCell::into_table_cell),
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
        mapper: impl Fn((usize, Stateful<Div>), &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.row_mapper = Some(Rc::new(mapper));
        self
    }

    pub fn text_for_selection(
        mut self,
        text_for_selection: impl Fn(usize, usize, &mut Window, &mut App) -> Option<SharedString>
        + 'static,
    ) -> Self {
        self.text_for_selection = Some(Rc::new(text_for_selection));
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

fn text_cell_style(base: Div, text_cell: &TableTextCell, cx: &App) -> Div {
    let mut color = text_cell.color.color(cx);
    if let Some(alpha) = text_cell.alpha {
        color.fade_out(1.0 - alpha);
    }

    base.text_ui_size(text_cell.size, cx)
        .when(
            text_cell.line_height_style == LineHeightStyle::UiLabel,
            |this| this.line_height(gpui::relative(1.0)),
        )
        .text_color(color)
        .font_weight(
            text_cell
                .weight
                .unwrap_or(ThemeSettings::get_global(cx).ui_font.weight),
        )
}

fn render_cell_base(width: Option<Length>, table_cx: &TableRenderContext) -> Div {
    if table_cx.disable_base_cell_style {
        gpui::div()
            .when_some(width, |this, width| this.w(width))
            .when(width.is_none(), |this| this.flex_1())
            .overflow_hidden()
    } else {
        base_cell_style(width)
    }
}

fn render_element_cell(
    width: Option<Length>,
    cell: AnyElement,
    table_cx: &TableRenderContext,
    cx: &App,
) -> Div {
    if table_cx.disable_base_cell_style {
        render_cell_base(width, table_cx).child(cell)
    } else {
        base_cell_style_text(width, table_cx.use_ui_font, cx)
            .px_1()
            .py_0p5()
            .child(cell)
    }
}

fn render_text_cell(
    row_index: usize,
    column_index: usize,
    width: Option<Length>,
    text_cell: &TableTextCell,
    table_cx: &TableRenderContext,
    cx: &App,
) -> Div {
    let selected_range = table_cx.interaction_state.as_ref().and_then(|state| {
        state
            .read(cx)
            .selected_range_for_cell(row_index, column_index, text_cell.text.as_ref())
    });
    let element = TableTextElement {
        interaction_state: table_cx.interaction_state.as_ref().map(Entity::downgrade),
        id: TableTextId::new(row_index, column_index),
        text: text_cell.text.clone(),
        styled_text: StyledText::new(text_cell.text.clone()),
        selected_range,
    };

    let base = render_cell_base(width, table_cx)
        .when(table_cx.disable_base_cell_style, |this| this.px_2().py_1())
        .when(!table_cx.disable_base_cell_style, |this| {
            this.px_1().py_0p5()
        });
    let mut cell = text_cell_style(base, text_cell, cx).child(element);

    if let Some(interaction_state) = table_cx.interaction_state.as_ref() {
        cell = cell
            .on_mouse_down(MouseButton::Left, {
                let interaction_state = interaction_state.clone();
                move |event, window, cx| {
                    interaction_state.update(cx, |state, cx| {
                        state.begin_text_selection(
                            row_index,
                            column_index,
                            event.position,
                            event.click_count,
                            window,
                            cx,
                        );
                    });
                    cx.stop_propagation();
                    window.prevent_default();
                }
            })
            .on_mouse_move({
                let interaction_state = interaction_state.clone();
                move |event, _, cx| {
                    interaction_state.update(cx, |state, cx| {
                        state.update_text_selection(row_index, column_index, event.position, cx);
                    });
                }
            })
            .on_mouse_up(MouseButton::Left, {
                let interaction_state = interaction_state.clone();
                move |event, _, cx| {
                    interaction_state.update(cx, |state, cx| {
                        state.end_text_selection(row_index, column_index, event.position, cx);
                    });
                }
            });

        if let Some(text_for_selection) = table_cx.text_for_selection.clone() {
            let row_count = table_cx.total_row_count;
            let column_count = table_cx.column_count;
            cell = cell.on_mouse_down(MouseButton::Right, {
                let interaction_state = interaction_state.clone();
                move |event, window, cx| {
                    interaction_state.update(cx, |state, cx| {
                        state.deploy_text_context_menu(
                            row_count,
                            column_count,
                            text_for_selection.as_ref(),
                            event.position,
                            window,
                            cx,
                        );
                    });
                    cx.stop_propagation();
                    window.prevent_default();
                }
            });
        }
    }

    cell
}

fn render_cell(
    row_index: usize,
    column_index: usize,
    width: Option<Length>,
    cell: TableCell,
    table_cx: &TableRenderContext,
    cx: &App,
) -> Div {
    match cell {
        TableCell::Element(element) => render_element_cell(width, element, table_cx, cx),
        TableCell::Text(text_cell) => {
            render_text_cell(row_index, column_index, width, &text_cell, table_cx, cx)
        }
    }
}

struct TableTextElement {
    interaction_state: Option<WeakEntity<TableInteractionState>>,
    id: TableTextId,
    text: SharedString,
    styled_text: StyledText,
    selected_range: Option<Range<usize>>,
}

impl Element for TableTextElement {
    type RequestLayoutState = ();
    type PrepaintState = Vec<Hitbox>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.styled_text
            .request_layout(None, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.styled_text
            .prepaint(None, inspector_id, bounds, state, window, cx);
        insert_text_hitboxes(self.styled_text.layout(), window)
    }

    fn paint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        hitboxes: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let text_layout = self.styled_text.layout().clone();
        for hitbox in hitboxes.as_slice() {
            window.set_cursor_style(CursorStyle::IBeam, hitbox);
        }

        if let Some(interaction_state) = self.interaction_state.as_ref()
            && let Err(error) = interaction_state.update(cx, |state, _| {
                state.text_selection.register_layout(
                    self.id,
                    self.text.clone(),
                    text_layout.clone(),
                );
            })
        {
            log::trace!("Failed to register table text layout: {error:?}");
        }

        if let Some(selected_range) = self.selected_range.clone() {
            paint_text_selection(
                selected_range,
                &text_layout,
                cx.theme().colors().element_selection_background,
                window,
            );
        }

        self.styled_text
            .paint(None, inspector_id, bounds, &mut (), &mut (), window, cx);
    }
}

impl IntoElement for TableTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
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
    items: TableRow<TableCell>,
    table_cx: TableRenderContext,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let is_striped = table_cx.striped;
    let is_last = row_index == table_cx.total_row_count - 1;
    let background = if row_index % 2 == 1 && is_striped {
        Some(cx.theme().colors().text.opacity(0.05))
    } else {
        None
    };
    let columns = items.cols();
    let column_widths = match &table_cx.column_widths {
        Some(widths) => widths.clone().map(Some),
        None => vec![None; columns].into_table_row(columns),
    };

    let row = gpui::div()
        .flex()
        .flex_row()
        .id(("table_row", row_index))
        .size_full()
        .when_some(background, |row, background| row.bg(background))
        .when(table_cx.show_row_hover, |row| {
            row.hover(|style| style.bg(cx.theme().colors().element_hover.opacity(0.6)))
        })
        .when(!is_striped && table_cx.show_row_borders, |row| {
            row.border_b_1()
                .border_color(gpui::transparent_black())
                .when(!is_last, |row| row.border_color(cx.theme().colors().border))
        })
        .children(
            items
                .into_vec()
                .into_iter()
                .enumerate()
                .zip(column_widths.into_vec())
                .map(|((column_index, cell), width)| {
                    render_cell(row_index, column_index, width, cell, &table_cx, cx)
                }),
        );

    let row = if let Some(row_mapper) = table_cx.row_mapper {
        row_mapper((row_index, row), window, cx)
    } else {
        row.into_any_element()
    };

    gpui::div().size_full().child(row).into_any_element()
}

pub fn render_table_header(
    header_row: TableRow<impl IntoElement>,
    table_cx: TableRenderContext,
    resize_info: Option<&HeaderResizeInfo>,
    entity_id: Option<EntityId>,
    cx: &mut App,
) -> AnyElement {
    let columns = header_row.cols();
    let column_widths = table_cx
        .column_widths
        .map_or(vec![None; columns].into_table_row(columns), |widths| {
            widths.map(Some)
        });

    let element_id = entity_id
        .map(|entity| entity.to_string())
        .unwrap_or_default();

    let shared_element_id: SharedString = format!("table-{element_id}").into();
    let use_ui_font = table_cx.use_ui_font;
    let resize_info_ref = resize_info;

    gpui::div()
        .flex()
        .flex_row()
        .items_center()
        .w_full()
        .border_b_1()
        .border_color(cx.theme().colors().border)
        .children(
            header_row
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
    pub column_count: usize,
    pub total_row_count: usize,
    pub column_widths: Option<TableRow<Length>>,
    pub row_mapper: Option<Rc<dyn Fn((usize, Stateful<Div>), &mut Window, &mut App) -> AnyElement>>,
    interaction_state: Option<Entity<TableInteractionState>>,
    text_for_selection:
        Option<Rc<dyn Fn(usize, usize, &mut Window, &mut App) -> Option<SharedString>>>,
    pub striped: bool,
    pub show_row_borders: bool,
    pub show_row_hover: bool,
    pub use_ui_font: bool,
    pub disable_base_cell_style: bool,
}

impl TableRenderContext {
    fn new(
        table: &Table,
        interaction_state: Option<Entity<TableInteractionState>>,
        cx: &App,
    ) -> Self {
        Self {
            column_count: table.column_count,
            total_row_count: table.rows.len(),
            column_widths: table.column_width_config.widths_to_render(cx),
            row_mapper: table.row_mapper.clone(),
            interaction_state,
            text_for_selection: table.text_for_selection.clone(),
            striped: table.striped,
            show_row_borders: table.show_row_borders,
            show_row_hover: table.show_row_hover,
            use_ui_font: table.use_ui_font,
            disable_base_cell_style: table.disable_base_cell_style,
        }
    }

    pub fn for_column_widths(column_widths: Option<TableRow<Length>>, use_ui_font: bool) -> Self {
        Self {
            column_count: column_widths.as_ref().map_or(0, TableRow::cols),
            total_row_count: 0,
            column_widths,
            row_mapper: None,
            interaction_state: None,
            text_for_selection: None,
            striped: false,
            show_row_borders: true,
            show_row_hover: true,
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
        let is_resizable = interactive && resize_behavior.expect_get(column_index).is_resizable();
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
        let focus_handle = interaction_state
            .as_ref()
            .map(|state| state.read(cx).focus_handle.clone());
        let context_menu = interaction_state.as_ref().and_then(|state| {
            state
                .read(cx)
                .context_menu
                .as_ref()
                .map(|(menu, position, _)| (menu.clone(), *position))
        });
        if let Some(interaction_state) = interaction_state.as_ref() {
            interaction_state.update(cx, |state, _| {
                state.clear_text_layouts();
            });
        }

        let text_for_selection = self.text_for_selection.clone();
        let row_count = self.rows.len();
        let column_count = self.column_count;
        let table_cx = TableRenderContext::new(&self, interaction_state.clone(), cx);

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
            .relative()
            .when_some(focus_handle.as_ref(), |this, focus_handle| {
                this.track_focus(focus_handle)
            })
            .when_some(
                interaction_state.as_ref().zip(text_for_selection),
                |this, (interaction_state, text_for_selection)| {
                    this.key_context("Text")
                        .on_action({
                            let interaction_state = interaction_state.clone();
                            let text_for_selection = text_for_selection.clone();

                            move |_: &actions::editor::Copy, window: &mut Window, cx: &mut App| {
                                interaction_state.update(cx, |state, cx| {
                                    state.copy_selected_text(
                                        row_count,
                                        column_count,
                                        text_for_selection.as_ref(),
                                        window,
                                        cx,
                                    );
                                });
                            }
                        })
                        .on_action({
                            let interaction_state = interaction_state.clone();

                            move |_: &actions::editor::SelectAll, _: &mut Window, cx: &mut App| {
                                interaction_state.update(cx, |state, cx| {
                                    state.select_all_text(row_count, cx);
                                });
                            }
                        })
                        .on_mouse_up(MouseButton::Left, {
                            let interaction_state = interaction_state.clone();

                            move |_, _, cx| {
                                interaction_state.update(cx, |state, cx| {
                                    if state.text_selection.end_selection_drag() {
                                        cx.notify();
                                    }
                                });
                            }
                        })
                },
            )
            .when_some(table_width, |this, width| this.w(width))
            .h_full()
            .flex()
            .flex_col()
            .when_some(self.header.take(), |this, header| {
                this.child(render_table_header(
                    header,
                    table_cx.clone(),
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
                                    table_cx.clone(),
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
                                            .map(|raw_row| {
                                                raw_row.into_table_row(self.column_count)
                                            })
                                            .collect::<Vec<_>>();
                                        elements
                                            .into_iter()
                                            .zip(range)
                                            .map(|(row, row_index)| {
                                                render_table_row(
                                                    row_index,
                                                    row,
                                                    table_cx.clone(),
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
                                        .into_table_row(self.column_count);
                                    render_table_row(row_index, row, table_cx.clone(), window, cx)
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
            )
            .when(context_menu.is_some(), |this| {
                this.child(
                    gpui::div()
                        .absolute()
                        .top_0()
                        .right_0()
                        .bottom_0()
                        .left_0()
                        .occlude(),
                )
            })
            .children(context_menu.as_ref().map(|(menu, position)| {
                gpui::deferred(
                    gpui::anchored()
                        .position(*position)
                        .anchor(Anchor::TopLeft)
                        .child(menu.clone()),
                )
                .with_priority(3)
            }));

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
