use super::{id, ns_string};
use objc::{class, msg_send, sel, sel_impl};

/// Creates a new UIProgressView (bar style).
pub(crate) unsafe fn create_native_progress_indicator() -> id {
    unsafe {
        // UIProgressViewStyleDefault = 0
        let progress: id = msg_send![class!(UIProgressView), alloc];
        let progress: id = msg_send![progress, initWithProgressViewStyle: 0i64];

        // UIAccessibility — UIProgressView already has correct default traits
        let _: () = msg_send![progress, setAccessibilityLabel: ns_string("Progress")];
        let _: () = msg_send![progress, setAccessibilityValue: ns_string("0%")];

        progress
    }
}

/// Sets progress style. On iOS: 0 = default bar, 1 = bar (same).
/// For indeterminate, we use UIActivityIndicatorView instead.
pub(crate) unsafe fn set_native_progress_style(_indicator: id, _style: i64) {
    // UIProgressView only has bar style on iOS. No-op for style changes.
}

/// Sets whether the progress is indeterminate.
/// iOS UIProgressView doesn't support indeterminate — we handle this
/// by toggling the progress value visibility.
pub(crate) unsafe fn set_native_progress_indeterminate(_indicator: id, _indeterminate: bool) {
    // No direct equivalent. The element layer should use UIActivityIndicatorView
    // for indeterminate progress. For now, no-op.
}

/// Sets the progress value (0.0 to 1.0).
pub(crate) unsafe fn set_native_progress_value(indicator: id, value: f64) {
    unsafe {
        let _: () = msg_send![indicator, setProgress: value as f32 animated: true as i8];
        let pct = (value * 100.0) as i32;
        let pct_str = format!("{}%", pct);
        let _: () = msg_send![indicator, setAccessibilityValue: ns_string(&pct_str)];
    }
}

/// Sets min/max values. UIProgressView is always 0.0-1.0, so we normalize.
pub(crate) unsafe fn set_native_progress_min_max(_indicator: id, _min: f64, _max: f64) {
    // UIProgressView is always 0.0..1.0. The element layer should normalize.
}

/// Start animation (no-op for UIProgressView — it doesn't animate).
pub(crate) unsafe fn start_native_progress_animation(_indicator: id) {}

/// Stop animation (no-op for UIProgressView).
pub(crate) unsafe fn stop_native_progress_animation(_indicator: id) {}

/// Sets whether the view is displayed when stopped.
pub(crate) unsafe fn set_native_progress_displayed_when_stopped(indicator: id, displayed: bool) {
    unsafe {
        let _: () = msg_send![indicator, setHidden: !displayed as i8];
    }
}

/// Releases a UIProgressView.
pub(crate) unsafe fn release_native_progress_indicator(indicator: id) {
    unsafe {
        if !indicator.is_null() {
            let _: () = msg_send![indicator, release];
        }
    }
}
