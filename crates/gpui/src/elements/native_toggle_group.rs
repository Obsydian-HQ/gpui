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
// Shape & size enums (macOS 26+ / NSControlBorderShape / NSControlSize)
// =============================================================================

/// Border shape for the segmented control (maps to `NSControlBorderShape`, macOS 26+).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeSegmentedShape {
    /// System decides based on control size and context.
    #[default]
    Automatic,
    /// Capsule (pill) shape.
    Capsule,
    /// Rounded rectangle.
    RoundedRectangle,
    /// Circle.
    Circle,
}

impl NativeSegmentedShape {
    fn to_ns_border_shape(self) -> i64 {
        match self {
            NativeSegmentedShape::Automatic => 0,     // NSControlBorderShapeAutomatic
            NativeSegmentedShape::Capsule => 1,        // NSControlBorderShapeCapsule
            NativeSegmentedShape::RoundedRectangle => 2, // NSControlBorderShapeRoundedRectangle
            NativeSegmentedShape::Circle => 3,         // NSControlBorderShapeCircle
        }
    }
}

/// Control size for the segmented control (maps to `NSControlSize`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeSegmentedSize {
    /// Mini size.
    Mini,
    /// Small size.
    Small,
    /// Regular size (default).
    #[default]
    Regular,
    /// Large size.
    Large,
    /// Extra-large size (macOS 26+).
    ExtraLarge,
}

impl NativeSegmentedSize {
    fn to_ns_control_size(self) -> u64 {
        match self {
            NativeSegmentedSize::Mini => 2,       // NSControlSizeMini
            NativeSegmentedSize::Small => 1,      // NSControlSizeSmall
            NativeSegmentedSize::Regular => 0,    // NSControlSizeRegular
            NativeSegmentedSize::Large => 3,      // NSControlSizeLarge
            NativeSegmentedSize::ExtraLarge => 4, // NSControlSizeExtraLarge
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
        symbols: None,
        selected_index: None,
        on_select: None,
        style: StyleRefinement::default(),
        border_shape: NativeSegmentedShape::default(),
        control_size: NativeSegmentedSize::default(),
    }
}

// =============================================================================
// Element struct
// =============================================================================

/// A native segmented control (NSSegmentedControl) positioned by GPUI's Taffy layout.
pub struct NativeToggleGroup {
    id: ElementId,
    labels: Vec<SharedString>,
    symbols: Option<Vec<SharedString>>,
    selected_index: Option<usize>,
    on_select: Option<Box<dyn Fn(&SegmentSelectEvent, &mut Window, &mut App) + 'static>>,
    style: StyleRefinement,
    border_shape: NativeSegmentedShape,
    control_size: NativeSegmentedSize,
}

impl NativeToggleGroup {
    /// Set which segment is initially selected.
    pub fn selected_index(mut self, index: usize) -> Self {
        self.selected_index = Some(index);
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

    /// Set the border shape of the segmented control (macOS 26+).
    pub fn border_shape(mut self, shape: NativeSegmentedShape) -> Self {
        self.border_shape = shape;
        self
    }

    /// Set the control size of the segmented control.
    pub fn control_size(mut self, size: NativeSegmentedSize) -> Self {
        self.control_size = size;
        self
    }

    /// Set SF Symbol names for each segment, replacing text labels with icons.
    /// The number of symbols should match the number of labels.
    pub fn sf_symbols(mut self, symbols: &[impl AsRef<str>]) -> Self {
        self.symbols = Some(
            symbols
                .iter()
                .map(|s| SharedString::from(s.as_ref().to_string()))
                .collect(),
        );
        self
    }
}

// =============================================================================
// Persisted element state
// =============================================================================

struct NativeToggleGroupState {
    control_ptr: *mut c_void,
    target_ptr: *mut c_void,
    current_selected: Option<usize>,
    current_symbols: Option<Vec<SharedString>>,
    current_border_shape: NativeSegmentedShape,
    current_control_size: NativeSegmentedSize,
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
            let per_segment = if self.symbols.is_some() { 36.0 } else { 70.0 };
            let width = (self.labels.len() as f32 * per_segment).max(72.0);
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
            let symbols = self.symbols.clone();
            let selected_index = self.selected_index;
            let border_shape = self.border_shape;
            let control_size = self.control_size;

            let next_frame_callbacks = window.next_frame_callbacks.clone();
            let invalidator = window.invalidator.clone();

            window.with_optional_element_state::<NativeToggleGroupState, _>(
                id,
                |prev_state, window| {
                    let state = if let Some(Some(mut state)) = prev_state {
                        // Update existing control
                        let control = state.control_ptr as cocoa::base::id;
                        unsafe {
                            native_controls::set_native_view_frame(
                                control,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );
                            if state.current_selected != selected_index {
                                native_controls::set_native_segmented_selected(
                                    control,
                                    selected_index,
                                );
                                state.current_selected = selected_index;
                            }
                            if state.current_border_shape != border_shape {
                                native_controls::set_native_segmented_border_shape(
                                    control,
                                    border_shape.to_ns_border_shape(),
                                );
                                state.current_border_shape = border_shape;
                            }
                            if state.current_control_size != control_size {
                                native_controls::set_native_segmented_control_size(
                                    control,
                                    control_size.to_ns_control_size(),
                                );
                                state.current_control_size = control_size;
                            }
                            if state.current_symbols != symbols {
                                if let Some(ref syms) = symbols {
                                    for (i, sym) in syms.iter().enumerate() {
                                        if !sym.is_empty() {
                                            native_controls::set_native_segmented_image(
                                                control,
                                                i,
                                                sym.as_ref(),
                                            );
                                        }
                                    }
                                }
                                state.current_symbols = symbols.clone();
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
                                    control,
                                    callback,
                                );
                            }
                        }

                        state
                    } else {
                        // Creation path
                        let (control_ptr, target_ptr) = unsafe {
                            let label_strs: Vec<&str> = labels.iter().map(|s| s.as_ref()).collect();
                            let control = native_controls::create_native_segmented_control(
                                &label_strs,
                                selected_index,
                            );

                            native_controls::set_native_segmented_border_shape(
                                control,
                                border_shape.to_ns_border_shape(),
                            );
                            native_controls::set_native_segmented_control_size(
                                control,
                                control_size.to_ns_control_size(),
                            );

                            if let Some(ref syms) = symbols {
                                for (i, sym) in syms.iter().enumerate() {
                                    if !sym.is_empty() {
                                        native_controls::set_native_segmented_image(
                                            control, i, sym.as_ref(),
                                        );
                                    }
                                }
                            }

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
                            current_symbols: symbols,
                            current_border_shape: border_shape,
                            current_control_size: control_size,
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
