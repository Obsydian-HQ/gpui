use cocoa::{
    base::{id, nil},
    foundation::{NSPoint, NSRect, NSSize},
};
use objc::{class, msg_send, sel, sel_impl};

// =============================================================================
// NSProgressIndicator — creation & lifecycle
// =============================================================================

/// Creates a new NSProgressIndicator.
pub(crate) unsafe fn create_native_progress_indicator() -> id {
    unsafe {
        let indicator: id = msg_send![class!(NSProgressIndicator), alloc];
        let indicator: id = msg_send![indicator, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(200.0, 20.0),
        )];
        let _: () = msg_send![indicator, setWantsLayer: 1i8];
        let _: () = msg_send![indicator, setAutoresizingMask: 0u64];
        indicator
    }
}

/// Sets indicator style.
/// 0 = bar, 1 = spinner.
pub(crate) unsafe fn set_native_progress_style(indicator: id, style: i64) {
    unsafe {
        let _: () = msg_send![indicator, setStyle: style];
    }
}

/// Sets whether the progress indicator is indeterminate.
pub(crate) unsafe fn set_native_progress_indeterminate(indicator: id, indeterminate: bool) {
    unsafe {
        let _: () = msg_send![indicator, setIndeterminate: indeterminate as i8];
    }
}

/// Sets the progress value.
pub(crate) unsafe fn set_native_progress_value(indicator: id, value: f64) {
    unsafe {
        let _: () = msg_send![indicator, setDoubleValue: value];
    }
}

/// Sets minimum and maximum bounds for determinate progress.
pub(crate) unsafe fn set_native_progress_min_max(indicator: id, min: f64, max: f64) {
    unsafe {
        let _: () = msg_send![indicator, setMinValue: min];
        let _: () = msg_send![indicator, setMaxValue: max];
    }
}

/// Starts progress animation (used for indeterminate modes/spinner).
pub(crate) unsafe fn start_native_progress_animation(indicator: id) {
    unsafe {
        let _: () = msg_send![indicator, startAnimation: nil];
    }
}

/// Stops progress animation.
pub(crate) unsafe fn stop_native_progress_animation(indicator: id) {
    unsafe {
        let _: () = msg_send![indicator, stopAnimation: nil];
    }
}

/// Sets whether the indicator remains visible when stopped.
pub(crate) unsafe fn set_native_progress_displayed_when_stopped(indicator: id, displayed: bool) {
    unsafe {
        let _: () = msg_send![indicator, setDisplayedWhenStopped: displayed as i8];
    }
}

/// Releases a progress indicator.
pub(crate) unsafe fn release_native_progress_indicator(indicator: id) {
    unsafe {
        let _: () = msg_send![indicator, release];
    }
}
