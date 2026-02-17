use refineable::Refineable as _;
use std::ffi::c_void;
use std::rc::Rc;

use crate::{
    AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, SharedString, Style,
    StyleRefinement, Styled, Window, px,
};

use super::native_element_helpers::schedule_native_callback;

/// Event emitted when a table row is selected.
#[derive(Clone, Debug)]
pub struct TableRowSelectEvent {
    /// Zero-based selected row index.
    pub index: usize,
}

/// AppKit `NSTableViewStyle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeTableStyle {
    /// `NSTableViewStyleAutomatic`.
    #[default]
    Automatic,
    /// `NSTableViewStyleFullWidth`.
    FullWidth,
    /// `NSTableViewStyleInset`.
    Inset,
    /// `NSTableViewStyleSourceList`.
    SourceList,
    /// `NSTableViewStylePlain`.
    Plain,
}

impl NativeTableStyle {
    fn to_ns_style(self) -> i64 {
        match self {
            NativeTableStyle::Automatic => 0,
            NativeTableStyle::FullWidth => 1,
            NativeTableStyle::Inset => 2,
            NativeTableStyle::SourceList => 3,
            NativeTableStyle::Plain => 4,
        }
    }
}

/// AppKit `NSTableViewRowSizeStyle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeTableRowSizeStyle {
    /// `NSTableViewRowSizeStyleDefault`.
    Default,
    /// `NSTableViewRowSizeStyleCustom`.
    Custom,
    /// `NSTableViewRowSizeStyleSmall`.
    Small,
    /// `NSTableViewRowSizeStyleMedium`.
    Medium,
    /// `NSTableViewRowSizeStyleLarge`.
    Large,
}

impl Default for NativeTableRowSizeStyle {
    fn default() -> Self {
        Self::Custom
    }
}

impl NativeTableRowSizeStyle {
    fn to_ns_style(self) -> i64 {
        match self {
            NativeTableRowSizeStyle::Default => -1,
            NativeTableRowSizeStyle::Custom => 0,
            NativeTableRowSizeStyle::Small => 1,
            NativeTableRowSizeStyle::Medium => 2,
            NativeTableRowSizeStyle::Large => 3,
        }
    }
}

/// AppKit `NSTableViewSelectionHighlightStyle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeTableSelectionHighlightStyle {
    /// `NSTableViewSelectionHighlightStyleRegular`.
    #[default]
    Regular,
    /// `NSTableViewSelectionHighlightStyleNone`.
    None,
}

impl NativeTableSelectionHighlightStyle {
    fn to_ns_style(self) -> i64 {
        match self {
            NativeTableSelectionHighlightStyle::Regular => 0,
            NativeTableSelectionHighlightStyle::None => -1,
        }
    }
}

/// AppKit `NSTableViewGridLineStyle` bitmask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeTableGridMask(u64);

impl NativeTableGridMask {
    /// `NSTableViewGridNone`.
    pub const NONE: Self = Self(0);
    /// `NSTableViewSolidVerticalGridLineMask`.
    pub const SOLID_VERTICAL: Self = Self(1 << 0);
    /// `NSTableViewSolidHorizontalGridLineMask`.
    pub const SOLID_HORIZONTAL: Self = Self(1 << 1);
    /// `NSTableViewDashedHorizontalGridLineMask`.
    pub const DASHED_HORIZONTAL: Self = Self(1 << 3);

    /// Returns a new mask combining two grid styles.
    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    fn bits(self) -> u64 {
        self.0
    }
}

impl Default for NativeTableGridMask {
    fn default() -> Self {
        Self::NONE
    }
}

/// Creates a native table view (NSTableView) for dense row/list UIs.
pub fn native_table_view(id: impl Into<ElementId>, items: &[impl AsRef<str>]) -> NativeTableView {
    NativeTableView {
        id: id.into(),
        items: items
            .iter()
            .map(|item| SharedString::from(item.as_ref().to_string()))
            .collect(),
        selected_index: None,
        row_height: 22.0,
        column_title: SharedString::from("Value"),
        show_header: false,
        alternating_rows: true,
        allows_multiple_selection: false,
        table_style: NativeTableStyle::default(),
        row_size_style: NativeTableRowSizeStyle::default(),
        selection_highlight_style: NativeTableSelectionHighlightStyle::default(),
        grid_mask: NativeTableGridMask::default(),
        on_select: None,
        style: StyleRefinement::default(),
    }
}

/// A native NSTableView wrapper with a single text column.
pub struct NativeTableView {
    id: ElementId,
    items: Vec<SharedString>,
    selected_index: Option<usize>,
    row_height: f64,
    column_title: SharedString,
    show_header: bool,
    alternating_rows: bool,
    allows_multiple_selection: bool,
    table_style: NativeTableStyle,
    row_size_style: NativeTableRowSizeStyle,
    selection_highlight_style: NativeTableSelectionHighlightStyle,
    grid_mask: NativeTableGridMask,
    on_select: Option<Box<dyn Fn(&TableRowSelectEvent, &mut Window, &mut App) + 'static>>,
    style: StyleRefinement,
}

impl NativeTableView {
    /// Sets the selected row.
    pub fn selected_index(mut self, selected_index: Option<usize>) -> Self {
        self.selected_index = selected_index;
        self
    }

    /// Sets row height in pixels.
    pub fn row_height(mut self, row_height: f64) -> Self {
        self.row_height = row_height.max(16.0);
        self
    }

    /// Sets the single column title (used when header is shown).
    pub fn column_title(mut self, column_title: impl Into<SharedString>) -> Self {
        self.column_title = column_title.into();
        self
    }

    /// Shows or hides the table header.
    pub fn show_header(mut self, show_header: bool) -> Self {
        self.show_header = show_header;
        self
    }

    /// Enables/disables alternating row backgrounds.
    pub fn alternating_rows(mut self, alternating_rows: bool) -> Self {
        self.alternating_rows = alternating_rows;
        self
    }

    /// Enables/disables multiple selection.
    pub fn allows_multiple_selection(mut self, allows_multiple_selection: bool) -> Self {
        self.allows_multiple_selection = allows_multiple_selection;
        self
    }

    /// Sets AppKit table style.
    pub fn table_style(mut self, table_style: NativeTableStyle) -> Self {
        self.table_style = table_style;
        self
    }

    /// Sets AppKit row size style.
    pub fn row_size_style(mut self, row_size_style: NativeTableRowSizeStyle) -> Self {
        self.row_size_style = row_size_style;
        self
    }

    /// Sets AppKit selection highlight style.
    pub fn selection_highlight_style(
        mut self,
        selection_highlight_style: NativeTableSelectionHighlightStyle,
    ) -> Self {
        self.selection_highlight_style = selection_highlight_style;
        self
    }

    /// Sets AppKit grid line mask.
    pub fn grid_mask(mut self, grid_mask: NativeTableGridMask) -> Self {
        self.grid_mask = grid_mask;
        self
    }

    /// Registers a callback fired when the selection changes.
    pub fn on_select(
        mut self,
        listener: impl Fn(&TableRowSelectEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Box::new(listener));
        self
    }
}

struct NativeTableViewState {
    control_ptr: *mut c_void,
    target_ptr: *mut c_void,
    current_items: Vec<SharedString>,
    current_selected: Option<usize>,
    current_row_height: f64,
    current_column_title: SharedString,
    current_show_header: bool,
    current_alternating_rows: bool,
    current_allows_multiple_selection: bool,
    current_table_style: NativeTableStyle,
    current_row_size_style: NativeTableRowSizeStyle,
    current_selection_highlight_style: NativeTableSelectionHighlightStyle,
    current_grid_mask: NativeTableGridMask,
    attached: bool,
}

impl Drop for NativeTableViewState {
    fn drop(&mut self) {
        if self.attached {
            #[cfg(target_os = "macos")]
            unsafe {
                use crate::platform::native_controls;
                super::native_element_helpers::cleanup_native_control(
                    self.control_ptr,
                    self.target_ptr,
                    native_controls::release_native_table_target,
                    native_controls::release_native_table_view,
                );
            }
        }
    }
}

unsafe impl Send for NativeTableViewState {}

impl IntoElement for NativeTableView {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeTableView {
    type RequestLayoutState = ();
    type PrepaintState = Bounds<Pixels>;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
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
        let mut style = Style::default();
        style.refine(&self.style);

        if matches!(style.size.width, Length::Auto) {
            style.size.width =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(360.0))));
        }
        if matches!(style.size.height, Length::Auto) {
            style.size.height =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(240.0))));
        }

        let layout_id = window.request_layout(style, [], cx);
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Bounds<Pixels> {
        bounds
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        _cx: &mut App,
    ) {
        #[cfg(target_os = "macos")]
        {
            use crate::platform::native_controls;

            let native_view = window.raw_native_view_ptr();
            if native_view.is_null() {
                return;
            }

            let mut on_select = self.on_select.take();
            let items = self.items.clone();
            let selected_index = self.selected_index;
            let row_height = self.row_height;
            let column_title = self.column_title.clone();
            let show_header = self.show_header;
            let alternating_rows = self.alternating_rows;
            let allows_multiple_selection = self.allows_multiple_selection;
            let table_style = self.table_style;
            let row_size_style = self.row_size_style;
            let selection_highlight_style = self.selection_highlight_style;
            let grid_mask = self.grid_mask;

            let next_frame_callbacks = window.next_frame_callbacks.clone();
            let invalidator = window.invalidator.clone();

            window.with_optional_element_state::<NativeTableViewState, _>(
                id,
                |prev_state, window| {
                    let state = if let Some(Some(mut state)) = prev_state {
                        unsafe {
                            native_controls::set_native_view_frame(
                                state.control_ptr as cocoa::base::id,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );
                            native_controls::set_native_table_column_width(
                                state.control_ptr as cocoa::base::id,
                                bounds.size.width.0 as f64,
                            );
                        }

                        if state.current_row_height != row_height {
                            unsafe {
                                native_controls::set_native_table_row_height(
                                    state.control_ptr as cocoa::base::id,
                                    row_height,
                                );
                            }
                            state.current_row_height = row_height;
                        }

                        if state.current_row_size_style != row_size_style {
                            unsafe {
                                native_controls::set_native_table_row_size_style(
                                    state.control_ptr as cocoa::base::id,
                                    row_size_style.to_ns_style(),
                                );
                            }
                            state.current_row_size_style = row_size_style;
                        }

                        if state.current_table_style != table_style {
                            unsafe {
                                native_controls::set_native_table_style(
                                    state.control_ptr as cocoa::base::id,
                                    table_style.to_ns_style(),
                                );
                            }
                            state.current_table_style = table_style;
                        }

                        if state.current_selection_highlight_style != selection_highlight_style {
                            unsafe {
                                native_controls::set_native_table_selection_highlight_style(
                                    state.control_ptr as cocoa::base::id,
                                    selection_highlight_style.to_ns_style(),
                                );
                            }
                            state.current_selection_highlight_style = selection_highlight_style;
                        }

                        if state.current_grid_mask != grid_mask {
                            unsafe {
                                native_controls::set_native_table_grid_style(
                                    state.control_ptr as cocoa::base::id,
                                    grid_mask.bits(),
                                );
                            }
                            state.current_grid_mask = grid_mask;
                        }

                        if state.current_alternating_rows != alternating_rows {
                            unsafe {
                                native_controls::set_native_table_uses_alternating_rows(
                                    state.control_ptr as cocoa::base::id,
                                    alternating_rows,
                                );
                            }
                            state.current_alternating_rows = alternating_rows;
                        }

                        if state.current_show_header != show_header {
                            unsafe {
                                native_controls::set_native_table_show_header(
                                    state.control_ptr as cocoa::base::id,
                                    show_header,
                                );
                            }
                            state.current_show_header = show_header;
                        }

                        if state.current_allows_multiple_selection != allows_multiple_selection {
                            unsafe {
                                native_controls::set_native_table_allows_multiple_selection(
                                    state.control_ptr as cocoa::base::id,
                                    allows_multiple_selection,
                                );
                            }
                            state.current_allows_multiple_selection = allows_multiple_selection;
                        }

                        if state.current_column_title != column_title {
                            unsafe {
                                native_controls::set_native_table_column_title(
                                    state.control_ptr as cocoa::base::id,
                                    &column_title,
                                );
                            }
                            state.current_column_title = column_title.clone();
                        }

                        let needs_rebind = state.current_items != items
                            || state.current_selected != selected_index
                            || on_select.is_some();
                        if needs_rebind {
                            unsafe {
                                native_controls::release_native_table_target(state.target_ptr);
                            }

                            let callback = on_select.take().map(|handler| {
                                let nfc = next_frame_callbacks.clone();
                                let inv = invalidator.clone();
                                let handler = Rc::new(handler);
                                schedule_native_callback(
                                    handler,
                                    |index| TableRowSelectEvent { index },
                                    nfc,
                                    inv,
                                )
                            });

                            let item_strs: Vec<&str> =
                                items.iter().map(|item| item.as_ref()).collect();
                            unsafe {
                                state.target_ptr = native_controls::set_native_table_items(
                                    state.control_ptr as cocoa::base::id,
                                    &item_strs,
                                    selected_index,
                                    callback,
                                );
                            }
                            state.current_items = items.clone();
                            state.current_selected = selected_index;
                        }

                        state
                    } else {
                        let callback = on_select.take().map(|handler| {
                            let nfc = next_frame_callbacks.clone();
                            let inv = invalidator.clone();
                            let handler = Rc::new(handler);
                            schedule_native_callback(
                                handler,
                                |index| TableRowSelectEvent { index },
                                nfc,
                                inv,
                            )
                        });

                        let (control_ptr, target_ptr) = unsafe {
                            let control = native_controls::create_native_table_view();
                            native_controls::set_native_table_column_width(
                                control,
                                bounds.size.width.0 as f64,
                            );
                            native_controls::set_native_table_row_height(control, row_height);
                            native_controls::set_native_table_row_size_style(
                                control,
                                row_size_style.to_ns_style(),
                            );
                            native_controls::set_native_table_style(
                                control,
                                table_style.to_ns_style(),
                            );
                            native_controls::set_native_table_selection_highlight_style(
                                control,
                                selection_highlight_style.to_ns_style(),
                            );
                            native_controls::set_native_table_grid_style(control, grid_mask.bits());
                            native_controls::set_native_table_uses_alternating_rows(
                                control,
                                alternating_rows,
                            );
                            native_controls::set_native_table_show_header(control, show_header);
                            native_controls::set_native_table_allows_multiple_selection(
                                control,
                                allows_multiple_selection,
                            );
                            native_controls::set_native_table_column_title(control, &column_title);

                            let item_strs: Vec<&str> =
                                items.iter().map(|item| item.as_ref()).collect();
                            let target = native_controls::set_native_table_items(
                                control,
                                &item_strs,
                                selected_index,
                                callback,
                            );

                            native_controls::attach_native_view_to_parent(
                                control,
                                native_view as cocoa::base::id,
                            );
                            native_controls::set_native_view_frame(
                                control,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );

                            (control as *mut c_void, target)
                        };

                        NativeTableViewState {
                            control_ptr,
                            target_ptr,
                            current_items: items,
                            current_selected: selected_index,
                            current_row_height: row_height,
                            current_column_title: column_title,
                            current_show_header: show_header,
                            current_alternating_rows: alternating_rows,
                            current_allows_multiple_selection: allows_multiple_selection,
                            current_table_style: table_style,
                            current_row_size_style: row_size_style,
                            current_selection_highlight_style: selection_highlight_style,
                            current_grid_mask: grid_mask,
                            attached: true,
                        }
                    };

                    ((), Some(state))
                },
            );
        }
    }
}

impl Styled for NativeTableView {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
