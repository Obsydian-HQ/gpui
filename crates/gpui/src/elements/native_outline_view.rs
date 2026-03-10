use refineable::Refineable as _;
use std::rc::Rc;

use crate::platform::native_controls::{NativeControlState, NativeOutlineNodeData, OutlineViewConfig};
use crate::{
    AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, SharedString, Style,
    StyleRefinement, Styled, Window, px,
};

use super::native_element_helpers::schedule_native_callback;

/// A node in a native outline tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeOutlineNode {
    /// Label shown for this row.
    pub title: SharedString,
    /// Child nodes under this row.
    pub children: Vec<NativeOutlineNode>,
}

impl NativeOutlineNode {
    /// Creates a leaf node with no children.
    pub fn leaf(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            children: Vec::new(),
        }
    }

    /// Creates a node with children.
    pub fn branch(title: impl Into<SharedString>, children: Vec<NativeOutlineNode>) -> Self {
        Self {
            title: title.into(),
            children,
        }
    }
}

/// Event emitted when a row is selected in the outline.
#[derive(Clone, Debug)]
pub struct OutlineRowSelectEvent {
    /// Zero-based row index in the currently visible outline rows.
    pub index: usize,
    /// Title of the selected row.
    pub title: SharedString,
}

/// Selection highlight style for the outline view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeOutlineHighlight {
    /// Standard blue highlight (default).
    #[default]
    Regular,
    /// Source-list style highlight (translucent, adapts to vibrancy).
    SourceList,
    /// No selection highlight at all.
    None,
}

impl NativeOutlineHighlight {
    fn to_ns_style(self) -> i64 {
        match self {
            NativeOutlineHighlight::Regular => 0,
            NativeOutlineHighlight::SourceList => 1,
            NativeOutlineHighlight::None => -1,
        }
    }
}

/// Creates a native outline view (NSOutlineView) for tree/expandable lists.
pub fn native_outline_view(
    id: impl Into<ElementId>,
    nodes: &[NativeOutlineNode],
) -> NativeOutlineView {
    NativeOutlineView {
        id: id.into(),
        nodes: nodes.to_vec(),
        selected_row: None,
        row_height: 22.0,
        expand_all: true,
        highlight: NativeOutlineHighlight::default(),
        on_select: None,
        style: StyleRefinement::default(),
    }
}

/// A native NSOutlineView wrapper for expandable hierarchical data.
pub struct NativeOutlineView {
    id: ElementId,
    nodes: Vec<NativeOutlineNode>,
    selected_row: Option<usize>,
    row_height: f64,
    expand_all: bool,
    highlight: NativeOutlineHighlight,
    on_select: Option<Box<dyn Fn(&OutlineRowSelectEvent, &mut Window, &mut App) + 'static>>,
    style: StyleRefinement,
}

impl NativeOutlineView {
    /// Sets the selected visible row.
    pub fn selected_row(mut self, selected_row: Option<usize>) -> Self {
        self.selected_row = selected_row;
        self
    }

    /// Sets row height in pixels.
    pub fn row_height(mut self, row_height: f64) -> Self {
        self.row_height = row_height.max(16.0);
        self
    }

    /// Enables or disables expanding all nodes after reload.
    pub fn expand_all(mut self, expand_all: bool) -> Self {
        self.expand_all = expand_all;
        self
    }

    /// Sets the selection highlight style.
    pub fn highlight(mut self, highlight: NativeOutlineHighlight) -> Self {
        self.highlight = highlight;
        self
    }

    /// Registers a callback fired when a row is selected.
    pub fn on_select(
        mut self,
        listener: impl Fn(&OutlineRowSelectEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Box::new(listener));
        self
    }
}

fn map_nodes(nodes: &[NativeOutlineNode]) -> Vec<NativeOutlineNodeData> {
    fn convert(node: &NativeOutlineNode) -> NativeOutlineNodeData {
        NativeOutlineNodeData {
            title: node.title.to_string(),
            children: node.children.iter().map(convert).collect(),
        }
    }

    nodes.iter().map(convert).collect()
}

impl IntoElement for NativeOutlineView {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeOutlineView {
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
        let nodes = self.nodes.clone();
        let selected_row = self.selected_row;
        let row_height = self.row_height;
        let expand_all = self.expand_all;
        let highlight = self.highlight;

        let next_frame_callbacks = window.next_frame_callbacks.clone();
        let invalidator = window.invalidator.clone();

        window.with_optional_element_state::<NativeControlState, _>(id, |prev_state, window| {
            let mut state = prev_state.flatten().unwrap_or_default();

            let on_select_fn = on_select.map(|handler| {
                let handler = Rc::new(handler);
                schedule_native_callback(
                    handler,
                    |(index, title): (usize, String)| OutlineRowSelectEvent {
                        index,
                        title: SharedString::from(title),
                    },
                    next_frame_callbacks.clone(),
                    invalidator.clone(),
                )
            });

            let mapped = map_nodes(&nodes);
            let scale = window.scale_factor();
            let nc = window.native_controls();
            nc.update_outline_view(
                &mut state,
                parent,
                bounds,
                scale,
                OutlineViewConfig {
                    nodes: &mapped,
                    selected_row,
                    expand_all,
                    highlight_style: Some(highlight.to_ns_style()),
                    row_height: Some(row_height),
                    on_select: on_select_fn,
                },
            );

            ((), Some(state))
        });
    }
}

impl Styled for NativeOutlineView {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
