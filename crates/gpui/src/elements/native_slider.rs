use refineable::Refineable as _;
use std::ffi::c_void;
use std::rc::Rc;

use crate::{
    AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, Style, StyleRefinement, Styled,
    Window, px,
};

use super::native_element_helpers::schedule_native_callback;

// =============================================================================
// Event type
// =============================================================================

/// Event emitted when the slider value changes.
#[derive(Clone, Debug)]
pub struct SliderChangeEvent {
    /// The new slider value.
    pub value: f64,
}

// =============================================================================
// Public constructor
// =============================================================================

/// Creates a native slider (NSSlider on macOS).
pub fn native_slider(id: impl Into<ElementId>) -> NativeSlider {
    NativeSlider {
        id: id.into(),
        min: 0.0,
        max: 1.0,
        value: 0.0,
        continuous: true,
        tick_marks: None,
        snap_to_ticks: false,
        on_change: None,
        disabled: false,
        style: StyleRefinement::default(),
    }
}

// =============================================================================
// Element struct
// =============================================================================

/// A native slider element positioned by GPUI's Taffy layout.
pub struct NativeSlider {
    id: ElementId,
    min: f64,
    max: f64,
    value: f64,
    continuous: bool,
    tick_marks: Option<usize>,
    snap_to_ticks: bool,
    on_change: Option<Box<dyn Fn(&SliderChangeEvent, &mut Window, &mut App) + 'static>>,
    disabled: bool,
    style: StyleRefinement,
}

impl NativeSlider {
    /// Sets the slider's minimum and maximum range.
    pub fn range(mut self, min: f64, max: f64) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    /// Sets the slider minimum.
    pub fn min(mut self, min: f64) -> Self {
        self.min = min;
        self
    }

    /// Sets the slider maximum.
    pub fn max(mut self, max: f64) -> Self {
        self.max = max;
        self
    }

    /// Sets the slider value.
    pub fn value(mut self, value: f64) -> Self {
        self.value = value;
        self
    }

    /// Sets whether the slider emits changes continuously while dragging.
    pub fn continuous(mut self, continuous: bool) -> Self {
        self.continuous = continuous;
        self
    }

    /// Sets the number of tick marks.
    pub fn tick_marks(mut self, count: usize) -> Self {
        self.tick_marks = Some(count);
        self
    }

    /// Clears all tick marks.
    pub fn no_tick_marks(mut self) -> Self {
        self.tick_marks = None;
        self
    }

    /// Sets whether values should snap to tick marks.
    pub fn snap_to_ticks(mut self, snap: bool) -> Self {
        self.snap_to_ticks = snap;
        self
    }

    /// Registers a callback invoked when the slider value changes.
    pub fn on_change(
        mut self,
        listener: impl Fn(&SliderChangeEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Box::new(listener));
        self
    }

    /// Sets whether this slider is disabled.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

// =============================================================================
// Persisted element state
// =============================================================================

struct NativeSliderElementState {
    slider_ptr: *mut c_void,
    target_ptr: *mut c_void,
    current_min: f64,
    current_max: f64,
    current_value: f64,
    current_continuous: bool,
    current_tick_marks: Option<usize>,
    current_snap_to_ticks: bool,
    attached: bool,
}

impl Drop for NativeSliderElementState {
    fn drop(&mut self) {
        if self.attached {
            #[cfg(target_os = "macos")]
            unsafe {
                use crate::platform::native_controls;
                super::native_element_helpers::cleanup_native_control(
                    self.slider_ptr,
                    self.target_ptr,
                    native_controls::release_native_slider_target,
                    native_controls::release_native_slider,
                );
            }
        }
    }
}

unsafe impl Send for NativeSliderElementState {}

// =============================================================================
// Helpers
// =============================================================================

fn normalize_range(min: f64, max: f64) -> (f64, f64) {
    if min <= max { (min, max) } else { (max, min) }
}

fn clamp_to_range(value: f64, min: f64, max: f64) -> f64 {
    value.max(min).min(max)
}

// =============================================================================
// Element trait impl
// =============================================================================

impl IntoElement for NativeSlider {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeSlider {
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
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(200.0))));
        }
        if matches!(style.size.height, Length::Auto) {
            style.size.height =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(22.0))));
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

            let on_change = self.on_change.take();
            let (min, max) = normalize_range(self.min, self.max);
            let value = clamp_to_range(self.value, min, max);
            let continuous = self.continuous;
            let tick_marks = self.tick_marks;
            let snap_to_ticks = self.snap_to_ticks;
            let disabled = self.disabled;

            let next_frame_callbacks = window.next_frame_callbacks.clone();
            let invalidator = window.invalidator.clone();

            window.with_optional_element_state::<NativeSliderElementState, _>(
                id,
                |prev_state, window| {
                    let state = if let Some(Some(mut state)) = prev_state {
                        unsafe {
                            native_controls::set_native_view_frame(
                                state.slider_ptr as cocoa::base::id,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );
                            if state.current_min != min {
                                native_controls::set_native_slider_min(
                                    state.slider_ptr as cocoa::base::id,
                                    min,
                                );
                                state.current_min = min;
                            }
                            if state.current_max != max {
                                native_controls::set_native_slider_max(
                                    state.slider_ptr as cocoa::base::id,
                                    max,
                                );
                                state.current_max = max;
                            }
                            if state.current_value != value {
                                native_controls::set_native_slider_value(
                                    state.slider_ptr as cocoa::base::id,
                                    value,
                                );
                                state.current_value = value;
                            }
                            if state.current_continuous != continuous {
                                native_controls::set_native_slider_continuous(
                                    state.slider_ptr as cocoa::base::id,
                                    continuous,
                                );
                                state.current_continuous = continuous;
                            }
                            if state.current_tick_marks != tick_marks
                                || state.current_snap_to_ticks != snap_to_ticks
                            {
                                let tick_count = tick_marks.map(|v| v as i64).unwrap_or(0);
                                native_controls::set_native_slider_tick_marks(
                                    state.slider_ptr as cocoa::base::id,
                                    tick_count,
                                    snap_to_ticks && tick_marks.is_some(),
                                );
                                state.current_tick_marks = tick_marks;
                                state.current_snap_to_ticks = snap_to_ticks;
                            }
                            native_controls::set_native_control_enabled(
                                state.slider_ptr as cocoa::base::id,
                                !disabled,
                            );
                        }

                        if let Some(on_change) = on_change {
                            unsafe {
                                native_controls::release_native_slider_target(state.target_ptr);
                            }
                            let nfc = next_frame_callbacks.clone();
                            let inv = invalidator.clone();
                            let on_change = Rc::new(on_change);
                            let callback = schedule_native_callback(
                                on_change,
                                |value| SliderChangeEvent { value },
                                nfc,
                                inv,
                            );
                            unsafe {
                                state.target_ptr = native_controls::set_native_slider_action(
                                    state.slider_ptr as cocoa::base::id,
                                    callback,
                                );
                            }
                        }

                        state
                    } else {
                        let (slider_ptr, target_ptr) = unsafe {
                            let slider = native_controls::create_native_slider(min, max, value);
                            native_controls::set_native_slider_continuous(slider, continuous);
                            let tick_count = tick_marks.map(|v| v as i64).unwrap_or(0);
                            native_controls::set_native_slider_tick_marks(
                                slider,
                                tick_count,
                                snap_to_ticks && tick_marks.is_some(),
                            );
                            native_controls::set_native_control_enabled(slider, !disabled);
                            native_controls::attach_native_view_to_parent(
                                slider,
                                native_view as cocoa::base::id,
                            );
                            native_controls::set_native_view_frame(
                                slider,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );

                            let target = if let Some(on_change) = on_change {
                                let nfc = next_frame_callbacks.clone();
                                let inv = invalidator.clone();
                                let on_change = Rc::new(on_change);
                                let callback = schedule_native_callback(
                                    on_change,
                                    |value| SliderChangeEvent { value },
                                    nfc,
                                    inv,
                                );
                                native_controls::set_native_slider_action(slider, callback)
                            } else {
                                std::ptr::null_mut()
                            };

                            (slider as *mut c_void, target)
                        };

                        NativeSliderElementState {
                            slider_ptr,
                            target_ptr,
                            current_min: min,
                            current_max: max,
                            current_value: value,
                            current_continuous: continuous,
                            current_tick_marks: tick_marks,
                            current_snap_to_ticks: snap_to_ticks,
                            attached: true,
                        }
                    };

                    ((), Some(state))
                },
            );
        }
    }
}

impl Styled for NativeSlider {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
