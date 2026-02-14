use refineable::Refineable as _;
use std::ffi::c_void;
use std::rc::Rc;

use crate::{
    AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, Style, StyleRefinement, Styled,
    Window, px,
};

use super::native_element_helpers::schedule_native_callback;

/// Event emitted when a native stepper value changes.
#[derive(Clone, Debug)]
pub struct StepperChangeEvent {
    /// The new numeric value.
    pub value: f64,
}

/// Creates a native stepper (NSStepper on macOS).
pub fn native_stepper(id: impl Into<ElementId>) -> NativeStepper {
    NativeStepper {
        id: id.into(),
        min: 0.0,
        max: 100.0,
        value: 0.0,
        increment: 1.0,
        wraps: false,
        autorepeat: true,
        on_change: None,
        disabled: false,
        style: StyleRefinement::default(),
    }
}

/// A native stepper element positioned by GPUI's Taffy layout.
pub struct NativeStepper {
    id: ElementId,
    min: f64,
    max: f64,
    value: f64,
    increment: f64,
    wraps: bool,
    autorepeat: bool,
    on_change: Option<Box<dyn Fn(&StepperChangeEvent, &mut Window, &mut App) + 'static>>,
    disabled: bool,
    style: StyleRefinement,
}

impl NativeStepper {
    /// Sets the stepper range.
    pub fn range(mut self, min: f64, max: f64) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    /// Sets the minimum value.
    pub fn min(mut self, min: f64) -> Self {
        self.min = min;
        self
    }

    /// Sets the maximum value.
    pub fn max(mut self, max: f64) -> Self {
        self.max = max;
        self
    }

    /// Sets the current value.
    pub fn value(mut self, value: f64) -> Self {
        self.value = value;
        self
    }

    /// Sets the step increment.
    pub fn increment(mut self, increment: f64) -> Self {
        self.increment = increment;
        self
    }

    /// Sets whether values wrap at min/max.
    pub fn wraps(mut self, wraps: bool) -> Self {
        self.wraps = wraps;
        self
    }

    /// Sets whether press-and-hold auto-repeats.
    pub fn autorepeat(mut self, autorepeat: bool) -> Self {
        self.autorepeat = autorepeat;
        self
    }

    /// Registers a callback invoked when the value changes.
    pub fn on_change(
        mut self,
        listener: impl Fn(&StepperChangeEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Box::new(listener));
        self
    }

    /// Sets whether this stepper is disabled.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

struct NativeStepperElementState {
    stepper_ptr: *mut c_void,
    target_ptr: *mut c_void,
    current_min: f64,
    current_max: f64,
    current_value: f64,
    current_increment: f64,
    current_wraps: bool,
    current_autorepeat: bool,
    attached: bool,
}

impl Drop for NativeStepperElementState {
    fn drop(&mut self) {
        if self.attached {
            #[cfg(target_os = "macos")]
            unsafe {
                use crate::platform::native_controls;
                super::native_element_helpers::cleanup_native_control(
                    self.stepper_ptr,
                    self.target_ptr,
                    native_controls::release_native_stepper_target,
                    native_controls::release_native_stepper,
                );
            }
        }
    }
}

unsafe impl Send for NativeStepperElementState {}

fn normalize_range(min: f64, max: f64) -> (f64, f64) {
    if min <= max { (min, max) } else { (max, min) }
}

fn clamp_to_range(value: f64, min: f64, max: f64) -> f64 {
    value.max(min).min(max)
}

impl IntoElement for NativeStepper {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeStepper {
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
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(20.0))));
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

            let on_change = self.on_change.take();
            let (min, max) = normalize_range(self.min, self.max);
            let value = clamp_to_range(self.value, min, max);
            let increment = self.increment.max(f64::EPSILON);
            let wraps = self.wraps;
            let autorepeat = self.autorepeat;
            let disabled = self.disabled;

            let next_frame_callbacks = window.next_frame_callbacks.clone();
            let invalidator = window.invalidator.clone();

            window.with_optional_element_state::<NativeStepperElementState, _>(
                id,
                |prev_state, window| {
                    let state = if let Some(Some(mut state)) = prev_state {
                        unsafe {
                            native_controls::set_native_view_frame(
                                state.stepper_ptr as cocoa::base::id,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );
                            if state.current_min != min {
                                native_controls::set_native_stepper_min(
                                    state.stepper_ptr as cocoa::base::id,
                                    min,
                                );
                                state.current_min = min;
                            }
                            if state.current_max != max {
                                native_controls::set_native_stepper_max(
                                    state.stepper_ptr as cocoa::base::id,
                                    max,
                                );
                                state.current_max = max;
                            }
                            if state.current_value != value {
                                native_controls::set_native_stepper_value(
                                    state.stepper_ptr as cocoa::base::id,
                                    value,
                                );
                                state.current_value = value;
                            }
                            if state.current_increment != increment {
                                native_controls::set_native_stepper_increment(
                                    state.stepper_ptr as cocoa::base::id,
                                    increment,
                                );
                                state.current_increment = increment;
                            }
                            if state.current_wraps != wraps {
                                native_controls::set_native_stepper_wraps(
                                    state.stepper_ptr as cocoa::base::id,
                                    wraps,
                                );
                                state.current_wraps = wraps;
                            }
                            if state.current_autorepeat != autorepeat {
                                native_controls::set_native_stepper_autorepeat(
                                    state.stepper_ptr as cocoa::base::id,
                                    autorepeat,
                                );
                                state.current_autorepeat = autorepeat;
                            }
                            native_controls::set_native_control_enabled(
                                state.stepper_ptr as cocoa::base::id,
                                !disabled,
                            );
                        }

                        if let Some(on_change) = on_change {
                            unsafe {
                                native_controls::release_native_stepper_target(state.target_ptr);
                            }
                            let nfc = next_frame_callbacks.clone();
                            let inv = invalidator.clone();
                            let on_change = Rc::new(on_change);
                            let callback = schedule_native_callback(
                                on_change,
                                |value| StepperChangeEvent { value },
                                nfc,
                                inv,
                            );
                            unsafe {
                                state.target_ptr = native_controls::set_native_stepper_action(
                                    state.stepper_ptr as cocoa::base::id,
                                    callback,
                                );
                            }
                        }

                        state
                    } else {
                        let (stepper_ptr, target_ptr) = unsafe {
                            let stepper =
                                native_controls::create_native_stepper(min, max, value, increment);
                            native_controls::set_native_stepper_wraps(stepper, wraps);
                            native_controls::set_native_stepper_autorepeat(stepper, autorepeat);
                            native_controls::set_native_control_enabled(stepper, !disabled);
                            native_controls::attach_native_view_to_parent(
                                stepper,
                                native_view as cocoa::base::id,
                            );
                            native_controls::set_native_view_frame(
                                stepper,
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
                                    |value| StepperChangeEvent { value },
                                    nfc,
                                    inv,
                                );
                                native_controls::set_native_stepper_action(stepper, callback)
                            } else {
                                std::ptr::null_mut()
                            };

                            (stepper as *mut c_void, target)
                        };

                        NativeStepperElementState {
                            stepper_ptr,
                            target_ptr,
                            current_min: min,
                            current_max: max,
                            current_value: value,
                            current_increment: increment,
                            current_wraps: wraps,
                            current_autorepeat: autorepeat,
                            attached: true,
                        }
                    };

                    ((), Some(state))
                },
            );
        }
    }
}

impl Styled for NativeStepper {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
