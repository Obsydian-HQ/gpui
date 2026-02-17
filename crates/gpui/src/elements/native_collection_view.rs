use refineable::Refineable as _;
use std::ffi::c_void;
use std::rc::Rc;

use crate::{
    AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, SharedString, Style,
    StyleRefinement, Styled, Window, px,
};

use super::native_element_helpers::schedule_native_callback;

/// Event emitted when a collection item (card) is clicked.
#[derive(Clone, Debug)]
pub struct CollectionSelectEvent {
    /// Zero-based item index.
    pub index: usize,
}

/// Visual presentation for `NativeCollectionView` items.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NativeCollectionItemStyle {
    /// Plain list/grid labels with native selection.
    Label,
    /// Card-like cells backed by AppKit `NSBox`.
    #[default]
    Card,
}

/// Creates a native collection view (NSCollectionView) with clickable cards.
pub fn native_collection_view(
    id: impl Into<ElementId>,
    items: &[impl AsRef<str>],
) -> NativeCollectionView {
    NativeCollectionView {
        id: id.into(),
        items: items
            .iter()
            .map(|item| SharedString::from(item.as_ref().to_string()))
            .collect(),
        selected_index: None,
        columns: 2,
        item_height: 72.0,
        spacing: 8.0,
        item_style: NativeCollectionItemStyle::default(),
        on_select: None,
        style: StyleRefinement::default(),
    }
}

/// A native NSCollectionView wrapper for clickable card/grid/list surfaces.
pub struct NativeCollectionView {
    id: ElementId,
    items: Vec<SharedString>,
    selected_index: Option<usize>,
    columns: usize,
    item_height: f64,
    spacing: f64,
    item_style: NativeCollectionItemStyle,
    on_select: Option<Box<dyn Fn(&CollectionSelectEvent, &mut Window, &mut App) + 'static>>,
    style: StyleRefinement,
}

impl NativeCollectionView {
    /// Sets the selected item index.
    pub fn selected_index(mut self, selected_index: Option<usize>) -> Self {
        self.selected_index = selected_index;
        self
    }

    /// Sets how many columns to render in the grid.
    pub fn columns(mut self, columns: usize) -> Self {
        self.columns = columns.max(1);
        self
    }

    /// Sets each card height in pixels.
    pub fn item_height(mut self, item_height: f64) -> Self {
        self.item_height = item_height.max(48.0);
        self
    }

    /// Sets spacing between cards in pixels.
    pub fn spacing(mut self, spacing: f64) -> Self {
        self.spacing = spacing.max(0.0);
        self
    }

    /// Sets how each item is rendered (`Label` or `Card`).
    pub fn item_style(mut self, item_style: NativeCollectionItemStyle) -> Self {
        self.item_style = item_style;
        self
    }

    /// Registers a callback fired when a card is clicked.
    pub fn on_select(
        mut self,
        listener: impl Fn(&CollectionSelectEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Box::new(listener));
        self
    }
}

struct NativeCollectionViewState {
    control_ptr: *mut c_void,
    target_ptr: *mut c_void,
    current_items: Vec<SharedString>,
    current_selected: Option<usize>,
    current_columns: usize,
    current_width: f64,
    current_item_height: f64,
    current_spacing: f64,
    current_item_style: NativeCollectionItemStyle,
    attached: bool,
}

impl Drop for NativeCollectionViewState {
    fn drop(&mut self) {
        if self.attached {
            #[cfg(target_os = "macos")]
            unsafe {
                use crate::platform::native_controls;
                super::native_element_helpers::cleanup_native_control(
                    self.control_ptr,
                    self.target_ptr,
                    native_controls::release_native_collection_target,
                    native_controls::release_native_collection_view,
                );
            }
        }
    }
}

unsafe impl Send for NativeCollectionViewState {}

impl IntoElement for NativeCollectionView {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeCollectionView {
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
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(420.0))));
        }
        if matches!(style.size.height, Length::Auto) {
            style.size.height =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(260.0))));
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
            let columns = self.columns;
            let width = bounds.size.width.0 as f64;
            let item_height = self.item_height;
            let spacing = self.spacing;
            let item_style = self.item_style;

            let next_frame_callbacks = window.next_frame_callbacks.clone();
            let invalidator = window.invalidator.clone();

            window.with_optional_element_state::<NativeCollectionViewState, _>(
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
                        }

                        if state.current_columns != columns
                            || (state.current_width - width).abs() > f64::EPSILON
                            || state.current_item_height != item_height
                            || state.current_spacing != spacing
                        {
                            unsafe {
                                native_controls::set_native_collection_layout(
                                    state.control_ptr as cocoa::base::id,
                                    width,
                                    columns,
                                    item_height,
                                    spacing,
                                );
                            }
                            state.current_columns = columns;
                            state.current_width = width;
                            state.current_item_height = item_height;
                            state.current_spacing = spacing;
                        }

                        let needs_rebind = state.current_items != items
                            || state.current_selected != selected_index
                            || state.current_item_style != item_style
                            || on_select.is_some();
                        if needs_rebind {
                            unsafe {
                                native_controls::release_native_collection_target(state.target_ptr);
                            }

                            let callback = on_select.take().map(|handler| {
                                let nfc = next_frame_callbacks.clone();
                                let inv = invalidator.clone();
                                let handler = Rc::new(handler);
                                schedule_native_callback(
                                    handler,
                                    |index| CollectionSelectEvent { index },
                                    nfc,
                                    inv,
                                )
                            });

                            let item_strs: Vec<&str> =
                                items.iter().map(|item| item.as_ref()).collect();
                            unsafe {
                                state.target_ptr =
                                    native_controls::set_native_collection_data_source(
                                        state.control_ptr as cocoa::base::id,
                                        &item_strs,
                                        selected_index,
                                        match item_style {
                                            NativeCollectionItemStyle::Label => {
                                                native_controls::NativeCollectionItemStyleData::Label
                                            }
                                            NativeCollectionItemStyle::Card => {
                                                native_controls::NativeCollectionItemStyleData::Card
                                            }
                                        },
                                        callback,
                                    );
                            }
                            state.current_items = items.clone();
                            state.current_selected = selected_index;
                            state.current_item_style = item_style;
                        }

                        state
                    } else {
                        let callback = on_select.take().map(|handler| {
                            let nfc = next_frame_callbacks.clone();
                            let inv = invalidator.clone();
                            let handler = Rc::new(handler);
                            schedule_native_callback(
                                handler,
                                |index| CollectionSelectEvent { index },
                                nfc,
                                inv,
                            )
                        });

                        let (control_ptr, target_ptr) = unsafe {
                            let control = native_controls::create_native_collection_view();
                            native_controls::set_native_collection_layout(
                                control,
                                width,
                                columns,
                                item_height,
                                spacing,
                            );

                            let item_strs: Vec<&str> =
                                items.iter().map(|item| item.as_ref()).collect();
                            let target = native_controls::set_native_collection_data_source(
                                control,
                                &item_strs,
                                selected_index,
                                match item_style {
                                    NativeCollectionItemStyle::Label => {
                                        native_controls::NativeCollectionItemStyleData::Label
                                    }
                                    NativeCollectionItemStyle::Card => {
                                        native_controls::NativeCollectionItemStyleData::Card
                                    }
                                },
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

                        NativeCollectionViewState {
                            control_ptr,
                            target_ptr,
                            current_items: items,
                            current_selected: selected_index,
                            current_columns: columns,
                            current_width: width,
                            current_item_height: item_height,
                            current_spacing: spacing,
                            current_item_style: item_style,
                            attached: true,
                        }
                    };

                    ((), Some(state))
                },
            );
        }
    }
}

impl Styled for NativeCollectionView {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
