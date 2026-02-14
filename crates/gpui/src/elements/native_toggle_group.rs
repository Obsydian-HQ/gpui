use refineable::Refineable as _;
use std::ffi::c_void;
use std::rc::Rc;

use crate::{
    AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, SharedString, Style,
    StyleRefinement, Styled, Window, px,
};

use super::native_element_helpers::schedule_native_callback;

// =============================================================================
// Event type
// =============================================================================

/// Event emitted when a segment is selected in a NativeToggleGroup.
#[derive(Clone, Debug)]
pub struct SegmentSelectEvent {
    /// The index of the selected segment.
    pub index: usize,
}

// =============================================================================
// Style enum
// =============================================================================

/// Visual style for the segmented control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeSegmentedStyle {
    /// Automatic (system decides, typically rounded).
    #[default]
    Automatic,
    /// Rounded segments.
    Rounded,
    /// Round-rect segments.
    RoundRect,
    /// Capsule-shaped segments.
    Capsule,
    /// Separated individual segments.
    Separated,
}

impl NativeSegmentedStyle {
    fn to_ns_style(self) -> i64 {
        match self {
            NativeSegmentedStyle::Automatic => 0,
            NativeSegmentedStyle::Rounded => 1,
            NativeSegmentedStyle::RoundRect => 3,
            NativeSegmentedStyle::Capsule => 5,
            NativeSegmentedStyle::Separated => 8,
        }
    }
}

// =============================================================================
// Public constructor
// =============================================================================

/// Creates a native segmented control (NSSegmentedControl on macOS).
///
/// Each label becomes a segment. The control participates in GPUI's Taffy layout.
pub fn native_toggle_group(
    id: impl Into<ElementId>,
    labels: &[impl AsRef<str>],
) -> NativeToggleGroup {
    NativeToggleGroup {
        id: id.into(),
        labels: labels
            .iter()
            .map(|l| SharedString::from(l.as_ref().to_string()))
            .collect(),
        selected_index: 0,
        on_select: None,
        style: StyleRefinement::default(),
        segment_style: NativeSegmentedStyle::default(),
    }
}

// =============================================================================
// Element struct
// =============================================================================

/// A native segmented control (NSSegmentedControl) positioned by GPUI's Taffy layout.
pub struct NativeToggleGroup {
    id: ElementId,
    labels: Vec<SharedString>,
    selected_index: usize,
    on_select: Option<Box<dyn Fn(&SegmentSelectEvent, &mut Window, &mut App) + 'static>>,
    style: StyleRefinement,
    segment_style: NativeSegmentedStyle,
}

impl NativeToggleGroup {
    /// Set which segment is initially selected.
    pub fn selected_index(mut self, index: usize) -> Self {
        self.selected_index = index;
        self
    }

    /// Register a callback invoked when a segment is selected.
    /// The event contains the newly selected index.
    pub fn on_select(
        mut self,
        listener: impl Fn(&SegmentSelectEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Box::new(listener));
        self
    }

    /// Set the visual style of the segmented control.
    pub fn segment_style(mut self, style: NativeSegmentedStyle) -> Self {
        self.segment_style = style;
        self
    }
}

// =============================================================================
// Persisted element state
// =============================================================================

struct NativeToggleGroupState {
    control_ptr: *mut c_void,
    target_ptr: *mut c_void,
    current_selected: usize,
    current_labels: Vec<SharedString>,
    current_segment_style: NativeSegmentedStyle,
    attached: bool,
}

impl Drop for NativeToggleGroupState {
    fn drop(&mut self) {
        if self.attached {
            #[cfg(target_os = "macos")]
            unsafe {
                use crate::platform::native_controls;
                super::native_element_helpers::cleanup_native_control(
                    self.control_ptr,
                    self.target_ptr,
                    native_controls::release_native_segmented_target,
                    native_controls::release_native_segmented_control,
                );
            }
        }
    }
}

unsafe impl Send for NativeToggleGroupState {}

// =============================================================================
// Element trait impl
// =============================================================================

impl IntoElement for NativeToggleGroup {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeToggleGroup {
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
            // Estimate width: ~70px per segment
            let width = (self.labels.len() as f32 * 70.0).max(140.0);
            style.size.width =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(width))));
        }
        if matches!(style.size.height, Length::Auto) {
            style.size.height =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(24.0))));
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

            let on_select = self.on_select.take();
            let labels = self.labels.clone();
            let selected_index = self.selected_index;
            let segment_style = self.segment_style;

            let next_frame_callbacks = window.next_frame_callbacks.clone();
            let invalidator = window.invalidator.clone();

            window.with_optional_element_state::<NativeToggleGroupState, _>(
                id,
                |prev_state, window| {
                    // If style changed, destroy old control so we recreate it.
                    // NSSegmentedControl doesn't reliably redraw on setSegmentStyle:.
                    let prev_state = match prev_state {
                        Some(Some(mut state)) if state.current_segment_style != segment_style => {
                            unsafe {
                                super::native_element_helpers::cleanup_native_control(
                                    state.control_ptr,
                                    state.target_ptr,
                                    native_controls::release_native_segmented_target,
                                    native_controls::release_native_segmented_control,
                                );
                            }
                            state.attached = false; // Prevent Drop from double-freeing
                            Some(None) // Fall through to creation path
                        }
                        other => other,
                    };

                    let state = if let Some(Some(mut state)) = prev_state {
                        // Normal update: style hasn't changed
                        unsafe {
                            native_controls::set_native_view_frame(
                                state.control_ptr as cocoa::base::id,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );
                            if state.current_selected != selected_index {
                                native_controls::set_native_segmented_selected(
                                    state.control_ptr as cocoa::base::id,
                                    selected_index,
                                );
                                state.current_selected = selected_index;
                            }
                        }

                        // Update callback
                        if let Some(on_select) = on_select {
                            unsafe {
                                native_controls::release_native_segmented_target(state.target_ptr);
                            }
                            let nfc = next_frame_callbacks.clone();
                            let inv = invalidator.clone();
                            let on_select = Rc::new(on_select);
                            let callback = schedule_native_callback(
                                on_select,
                                |index| SegmentSelectEvent { index },
                                nfc,
                                inv,
                            );
                            unsafe {
                                state.target_ptr = native_controls::set_native_segmented_action(
                                    state.control_ptr as cocoa::base::id,
                                    callback,
                                );
                            }
                        }

                        state
                    } else {
                        // Creation path: new control or style changed
                        let (control_ptr, target_ptr) = unsafe {
                            let label_strs: Vec<&str> = labels.iter().map(|s| s.as_ref()).collect();
                            let control = native_controls::create_native_segmented_control(
                                &label_strs,
                                selected_index,
                            );

                            native_controls::set_native_segmented_style(
                                control,
                                segment_style.to_ns_style(),
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

                            let target = if let Some(on_select) = on_select {
                                let nfc = next_frame_callbacks.clone();
                                let inv = invalidator.clone();
                                let on_select = Rc::new(on_select);
                                let callback = schedule_native_callback(
                                    on_select,
                                    |index| SegmentSelectEvent { index },
                                    nfc,
                                    inv,
                                );
                                native_controls::set_native_segmented_action(control, callback)
                            } else {
                                std::ptr::null_mut()
                            };

                            (control as *mut c_void, target)
                        };

                        NativeToggleGroupState {
                            control_ptr,
                            target_ptr,
                            current_selected: selected_index,
                            current_labels: labels,
                            current_segment_style: segment_style,
                            attached: true,
                        }
                    };

                    ((), Some(state))
                },
            );
        }
    }
}

impl Styled for NativeToggleGroup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
