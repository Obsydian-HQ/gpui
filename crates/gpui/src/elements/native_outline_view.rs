use refineable::Refineable as _;
use std::ffi::c_void;
use std::rc::Rc;

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

    /// Registers a callback fired when a row is selected.
    pub fn on_select(
        mut self,
        listener: impl Fn(&OutlineRowSelectEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Box::new(listener));
        self
    }
}

struct NativeOutlineViewState {
    control_ptr: *mut c_void,
    target_ptr: *mut c_void,
    current_nodes: Vec<NativeOutlineNode>,
    current_selected_row: Option<usize>,
    current_row_height: f64,
    current_expand_all: bool,
    attached: bool,
}

impl Drop for NativeOutlineViewState {
    fn drop(&mut self) {
        if self.attached {
            #[cfg(target_os = "macos")]
            unsafe {
                use crate::platform::native_controls;
                super::native_element_helpers::cleanup_native_control(
                    self.control_ptr,
                    self.target_ptr,
                    native_controls::release_native_outline_target,
                    native_controls::release_native_outline_view,
                );
            }
        }
    }
}

unsafe impl Send for NativeOutlineViewState {}

#[cfg(target_os = "macos")]
fn map_nodes(
    nodes: &[NativeOutlineNode],
) -> Vec<crate::platform::native_controls::NativeOutlineNodeData> {
    fn convert(
        node: &NativeOutlineNode,
    ) -> crate::platform::native_controls::NativeOutlineNodeData {
        crate::platform::native_controls::NativeOutlineNodeData {
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

        if matches!(style.size.width, Length::Auto) {
            style.size.width =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(380.0))));
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
            let nodes = self.nodes.clone();
            let selected_row = self.selected_row;
            let row_height = self.row_height;
            let expand_all = self.expand_all;

            let next_frame_callbacks = window.next_frame_callbacks.clone();
            let invalidator = window.invalidator.clone();

            window.with_optional_element_state::<NativeOutlineViewState, _>(
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

                        if state.current_row_height != row_height {
                            unsafe {
                                native_controls::set_native_outline_row_height(
                                    state.control_ptr as cocoa::base::id,
                                    row_height,
                                );
                            }
                            state.current_row_height = row_height;
                        }

                        let needs_rebind = state.current_nodes != nodes
                            || state.current_selected_row != selected_row
                            || state.current_expand_all != expand_all
                            || on_select.is_some();
                        if needs_rebind {
                            unsafe {
                                native_controls::release_native_outline_target(state.target_ptr);
                            }

                            let callback = on_select.take().map(|handler| {
                                let nfc = next_frame_callbacks.clone();
                                let inv = invalidator.clone();
                                let handler = Rc::new(handler);
                                schedule_native_callback(
                                    handler,
                                    |(index, title): (usize, String)| OutlineRowSelectEvent {
                                        index,
                                        title: SharedString::from(title),
                                    },
                                    nfc,
                                    inv,
                                )
                            });

                            let mapped = map_nodes(&nodes);
                            unsafe {
                                state.target_ptr = native_controls::set_native_outline_items(
                                    state.control_ptr as cocoa::base::id,
                                    &mapped,
                                    selected_row,
                                    expand_all,
                                    callback,
                                );
                            }

                            state.current_nodes = nodes.clone();
                            state.current_selected_row = selected_row;
                            state.current_expand_all = expand_all;
                        }

                        state
                    } else {
                        let callback = on_select.take().map(|handler| {
                            let nfc = next_frame_callbacks.clone();
                            let inv = invalidator.clone();
                            let handler = Rc::new(handler);
                            schedule_native_callback(
                                handler,
                                |(index, title): (usize, String)| OutlineRowSelectEvent {
                                    index,
                                    title: SharedString::from(title),
                                },
                                nfc,
                                inv,
                            )
                        });

                        let mapped = map_nodes(&nodes);

                        let (control_ptr, target_ptr) = unsafe {
                            let control = native_controls::create_native_outline_view();
                            native_controls::set_native_outline_row_height(control, row_height);

                            let target = native_controls::set_native_outline_items(
                                control,
                                &mapped,
                                selected_row,
                                expand_all,
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

                        NativeOutlineViewState {
                            control_ptr,
                            target_ptr,
                            current_nodes: nodes,
                            current_selected_row: selected_row,
                            current_row_height: row_height,
                            current_expand_all: expand_all,
                            attached: true,
                        }
                    };

                    ((), Some(state))
                },
            );
        }
    }
}

impl Styled for NativeOutlineView {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
