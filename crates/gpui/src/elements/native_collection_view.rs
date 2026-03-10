use refineable::Refineable as _;
use std::rc::Rc;

use crate::platform::native_controls::{CollectionItemStyle, CollectionViewConfig, NativeControlState};
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

impl From<NativeCollectionItemStyle> for CollectionItemStyle {
    fn from(s: NativeCollectionItemStyle) -> Self {
        match s {
            NativeCollectionItemStyle::Label => CollectionItemStyle::Label,
            NativeCollectionItemStyle::Card => CollectionItemStyle::Card,
        }
    }
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
        let parent = window.raw_native_view_ptr();
        if parent.is_null() {
            return;
        }

        let on_select = self.on_select.take();
        let items = std::mem::take(&mut self.items);
        let selected_index = self.selected_index;
        let columns = self.columns;
        let width = bounds.size.width.0 as f64;
        let item_height = self.item_height;
        let spacing = self.spacing;
        let item_style = self.item_style;

        let next_frame_callbacks = window.next_frame_callbacks.clone();
        let invalidator = window.invalidator.clone();

        window.with_optional_element_state::<NativeControlState, _>(id, |prev_state, window| {
            let mut state = prev_state.flatten().unwrap_or_default();

            let on_select_fn: Option<Box<dyn Fn(usize)>> = on_select.map(|handler| {
                let handler = Rc::new(handler);
                schedule_native_callback(
                    handler,
                    |index| CollectionSelectEvent { index },
                    next_frame_callbacks.clone(),
                    invalidator.clone(),
                )
            });

            let item_strs: Vec<&str> = items.iter().map(|s| s.as_ref()).collect();

            let scale = window.scale_factor();
            let nc = window.native_controls();
            nc.update_collection_view(
                &mut state,
                parent,
                bounds,
                scale,
                CollectionViewConfig {
                    width,
                    columns,
                    item_height,
                    spacing,
                    items: &item_strs,
                    selected: selected_index,
                    item_style: item_style.into(),
                    on_select: on_select_fn,
                },
            );

            ((), Some(state))
        });
    }
}

impl Styled for NativeCollectionView {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
