// iOS sidebar implementation using UISplitViewController concepts.
// On iOS, sidebars are typically managed by UISplitViewController or
// a custom side panel. This is a simplified wrapper that creates a
// container UIView that manages sidebar + content layout.

use super::id;
use objc::{class, msg_send, sel, sel_impl};
use std::ffi::c_void;

/// Creates a container view that acts as a sidebar host.
/// On iOS, this is a plain UIView — the sidebar behavior
/// (slide in/out, width management) is managed at the element layer.
pub(crate) unsafe fn create_native_sidebar_view(
    _width: f64,
    _min_width: f64,
    _max_width: f64,
) -> id {
    unsafe {
        let view: id = msg_send![class!(UIView), alloc];
        let view: id = msg_send![view, init];
        // Set a default background for the sidebar area
        let bg: id = msg_send![class!(UIColor), systemGroupedBackgroundColor];
        let _: () = msg_send![view, setBackgroundColor: bg];
        view
    }
}

/// No-op on iOS — window configuration is managed by UIKit.
pub(crate) unsafe fn configure_native_sidebar_window(
    _view: id,
    _window: *mut c_void,
) {}

/// Sets the sidebar width by updating the frame.
pub(crate) unsafe fn set_native_sidebar_width(_view: id, _width: f64) {
    // Frame management handled by set_native_view_frame at the element layer.
}

/// Sets sidebar collapsed state by hiding/showing.
pub(crate) unsafe fn set_native_sidebar_collapsed(view: id, collapsed: bool) {
    unsafe {
        let _: () = msg_send![view, setHidden: collapsed as i8];
    }
}

/// Sets sidebar items. No-op for the container — items are child views.
pub(crate) unsafe fn set_native_sidebar_items(
    _view: id,
    _items: *mut c_void,
) -> *mut c_void {
    std::ptr::null_mut()
}

/// No-op on iOS.
pub(crate) unsafe fn set_native_sidebar_header(
    _view: id,
    _title: &str,
) {}

/// Embeds a surface view in the sidebar content area.
pub(crate) unsafe fn embed_surface_view_in_sidebar(host_view: id, surface_view: id) {
    unsafe {
        let _: () = msg_send![host_view, addSubview: surface_view];
    }
}

/// Sets the sidebar background color.
pub(crate) unsafe fn set_native_sidebar_background_color(
    view: id,
    r: f64,
    g: f64,
    b: f64,
    a: f64,
) {
    unsafe {
        let color: id = msg_send![class!(UIColor),
            colorWithRed: r green: g blue: b alpha: a
        ];
        let _: () = msg_send![view, setBackgroundColor: color];
    }
}

/// Clears the background color.
pub(crate) unsafe fn clear_native_sidebar_background_color(view: id) {
    unsafe {
        let clear: id = msg_send![class!(UIColor), clearColor];
        let _: () = msg_send![view, setBackgroundColor: clear];
    }
}

/// Releases the sidebar target/delegate.
pub(crate) unsafe fn release_native_sidebar_target(_target: *mut c_void) {}

/// Releases the sidebar view.
pub(crate) unsafe fn release_native_sidebar_view(view: id) {
    unsafe {
        if !view.is_null() {
            let _: () = msg_send![view, release];
        }
    }
}
