use refineable::Refineable as _;
use std::ffi::c_void;

use crate::{
    AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, Style, StyleRefinement, Styled,
    Window, px,
};

// =============================================================================
// Style enum
// =============================================================================

/// Visual style for a native progress indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeProgressStyle {
    /// Horizontal bar style.
    #[default]
    Bar,
    /// Spinning style.
    Spinner,
}

impl NativeProgressStyle {
    fn to_ns_style(self) -> i64 {
        match self {
            NativeProgressStyle::Bar => 0,
            NativeProgressStyle::Spinner => 1,
        }
    }
}

// =============================================================================
// Public constructor
// =============================================================================

/// Creates a native progress indicator (NSProgressIndicator on macOS).
pub fn native_progress_bar(id: impl Into<ElementId>) -> NativeProgressBar {
    NativeProgressBar {
        id: id.into(),
        value: Some(0.0),
        min: 0.0,
        max: 1.0,
        progress_style: NativeProgressStyle::Bar,
        displayed_when_stopped: true,
        style: StyleRefinement::default(),
    }
}

// =============================================================================
// Element struct
// =============================================================================

/// A native progress indicator element positioned by GPUI's Taffy layout.
pub struct NativeProgressBar {
    id: ElementId,
    value: Option<f64>,
    min: f64,
    max: f64,
    progress_style: NativeProgressStyle,
    displayed_when_stopped: bool,
    style: StyleRefinement,
}

impl NativeProgressBar {
    /// Sets the displayed progress value. Pass `None` for indeterminate mode.
    pub fn maybe_value(mut self, value: Option<f64>) -> Self {
        self.value = value;
        self
    }

    /// Sets a determinate progress value.
    pub fn value(mut self, value: f64) -> Self {
        self.value = Some(value);
        self
    }

    /// Sets indeterminate mode on or off.
    pub fn indeterminate(mut self, indeterminate: bool) -> Self {
        self.value = if indeterminate { None } else { Some(self.min) };
        self
    }

    /// Sets progress range.
    pub fn range(mut self, min: f64, max: f64) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    /// Sets progress minimum value.
    pub fn min(mut self, min: f64) -> Self {
        self.min = min;
        self
    }

    /// Sets progress maximum value.
    pub fn max(mut self, max: f64) -> Self {
        self.max = max;
        self
    }

    /// Sets the visual style.
    pub fn progress_style(mut self, style: NativeProgressStyle) -> Self {
        self.progress_style = style;
        self
    }

    /// Sets whether the control is visible when animation is stopped.
    pub fn displayed_when_stopped(mut self, displayed: bool) -> Self {
        self.displayed_when_stopped = displayed;
        self
    }
}

// =============================================================================
// Persisted element state
// =============================================================================

struct NativeProgressBarElementState {
    indicator_ptr: *mut c_void,
    current_value: Option<f64>,
    current_min: f64,
    current_max: f64,
    current_style: NativeProgressStyle,
    current_indeterminate: bool,
    current_displayed_when_stopped: bool,
    animating: bool,
    attached: bool,
}

impl Drop for NativeProgressBarElementState {
    fn drop(&mut self) {
        if self.attached {
            #[cfg(target_os = "macos")]
            unsafe {
                use crate::platform::native_controls;
                let indicator = self.indicator_ptr as cocoa::base::id;
                native_controls::stop_native_progress_animation(indicator);
                native_controls::remove_native_view_from_parent(indicator);
                native_controls::release_native_progress_indicator(indicator);
            }
        }
    }
}

unsafe impl Send for NativeProgressBarElementState {}

// =============================================================================
// Helpers
// =============================================================================

fn normalize_range(min: f64, max: f64) -> (f64, f64) {
    if min <= max { (min, max) } else { (max, min) }
}

fn clamp_to_range(value: f64, min: f64, max: f64) -> f64 {
    value.max(min).min(max)
}

fn should_animate(style: NativeProgressStyle, indeterminate: bool) -> bool {
    indeterminate || matches!(style, NativeProgressStyle::Spinner)
}

// =============================================================================
// Element trait impl
// =============================================================================

impl IntoElement for NativeProgressBar {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeProgressBar {
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
            let width = if matches!(self.progress_style, NativeProgressStyle::Spinner) {
                32.0
            } else {
                200.0
            };
            style.size.width =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(width))));
        }
        if matches!(style.size.height, Length::Auto) {
            let height = if matches!(self.progress_style, NativeProgressStyle::Spinner) {
                32.0
            } else {
                20.0
            };
            style.size.height =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(height))));
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

            let progress_style = self.progress_style;
            let displayed_when_stopped = self.displayed_when_stopped;
            let (min, max) = normalize_range(self.min, self.max);
            let indeterminate =
                self.value.is_none() || matches!(progress_style, NativeProgressStyle::Spinner);
            let value = self
                .value
                .map(|v| clamp_to_range(v, min, max))
                .unwrap_or(min);

            window.with_optional_element_state::<NativeProgressBarElementState, _>(
                id,
                |prev_state, window| {
                    let animate = should_animate(progress_style, indeterminate);

                    let state = if let Some(Some(mut state)) = prev_state {
                        unsafe {
                            let indicator = state.indicator_ptr as cocoa::base::id;
                            native_controls::set_native_view_frame(
                                indicator,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );

                            if state.current_style != progress_style {
                                native_controls::set_native_progress_style(
                                    indicator,
                                    progress_style.to_ns_style(),
                                );
                                state.current_style = progress_style;
                            }

                            if state.current_min != min || state.current_max != max {
                                native_controls::set_native_progress_min_max(indicator, min, max);
                                state.current_min = min;
                                state.current_max = max;
                            }

                            if state.current_displayed_when_stopped != displayed_when_stopped {
                                native_controls::set_native_progress_displayed_when_stopped(
                                    indicator,
                                    displayed_when_stopped,
                                );
                                state.current_displayed_when_stopped = displayed_when_stopped;
                            }

                            if state.current_indeterminate != indeterminate {
                                native_controls::set_native_progress_indeterminate(
                                    indicator,
                                    indeterminate,
                                );
                                state.current_indeterminate = indeterminate;
                            }

                            let next_value = if indeterminate { None } else { Some(value) };
                            if state.current_value != next_value {
                                if let Some(v) = next_value {
                                    native_controls::set_native_progress_value(indicator, v);
                                }
                                state.current_value = next_value;
                            }

                            if animate && !state.animating {
                                native_controls::start_native_progress_animation(indicator);
                                state.animating = true;
                            } else if !animate && state.animating {
                                native_controls::stop_native_progress_animation(indicator);
                                state.animating = false;
                            }
                        }

                        state
                    } else {
                        let indicator_ptr = unsafe {
                            let indicator = native_controls::create_native_progress_indicator();
                            native_controls::set_native_progress_style(
                                indicator,
                                progress_style.to_ns_style(),
                            );
                            native_controls::set_native_progress_min_max(indicator, min, max);
                            native_controls::set_native_progress_displayed_when_stopped(
                                indicator,
                                displayed_when_stopped,
                            );
                            native_controls::set_native_progress_indeterminate(
                                indicator,
                                indeterminate,
                            );
                            if !indeterminate {
                                native_controls::set_native_progress_value(indicator, value);
                            }
                            native_controls::attach_native_view_to_parent(
                                indicator,
                                native_view as cocoa::base::id,
                            );
                            native_controls::set_native_view_frame(
                                indicator,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );
                            if animate {
                                native_controls::start_native_progress_animation(indicator);
                            }
                            indicator as *mut c_void
                        };

                        NativeProgressBarElementState {
                            indicator_ptr,
                            current_value: if indeterminate { None } else { Some(value) },
                            current_min: min,
                            current_max: max,
                            current_style: progress_style,
                            current_indeterminate: indeterminate,
                            current_displayed_when_stopped: displayed_when_stopped,
                            animating: animate,
                            attached: true,
                        }
                    };

                    ((), Some(state))
                },
            );
        }
    }
}

impl Styled for NativeProgressBar {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
