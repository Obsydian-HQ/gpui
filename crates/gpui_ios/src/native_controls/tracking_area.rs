// iOS has no NSTrackingArea equivalent since there's no persistent cursor.
// On iPadOS with a pointer (trackpad/mouse), hover events come through
// UIHoverGestureRecognizer, not tracking areas.
// This module provides no-op stubs to match the macOS API surface.

use super::id;
use objc::{class, msg_send, sel, sel_impl};
use std::ffi::c_void;

/// Creates a transparent UIView placeholder (no tracking on iOS).
pub(crate) unsafe fn create_native_tracking_view() -> id {
    unsafe {
        let view: id = msg_send![class!(UIView), alloc];
        let view: id = msg_send![view, init];
        let _: () = msg_send![view, setUserInteractionEnabled: false as i8];
        view
    }
}

/// No-op — no mouse enter/exit events on iOS.
pub(crate) unsafe fn set_native_tracking_view_callbacks(
    _view: id,
    _on_enter: Box<dyn Fn()>,
    _on_exit: Box<dyn Fn()>,
) -> *mut c_void {
    std::ptr::null_mut()
}

/// No-op.
pub(crate) unsafe fn release_native_tracking_view_target(_target: *mut c_void) {}

/// Releases the tracking view.
pub(crate) unsafe fn release_native_tracking_view(view: id) {
    unsafe {
        if !view.is_null() {
            let _: () = msg_send![view, release];
        }
    }
}
