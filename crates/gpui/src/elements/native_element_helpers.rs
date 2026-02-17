use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;

use crate::{App, Window, WindowInvalidator};

pub(super) type FrameCallback = Box<dyn FnOnce(&mut Window, &mut App)>;

/// Creates a `Fn()` callback that schedules an event handler via next_frame_callbacks.
/// Used for buttons where the ObjC action provides no parameters.
pub(super) fn schedule_native_callback_no_args<Event: 'static>(
    handler: Rc<Box<dyn Fn(&Event, &mut Window, &mut App)>>,
    make_event: impl Fn() -> Event + 'static,
    nfc: Rc<RefCell<Vec<FrameCallback>>>,
    inv: WindowInvalidator,
) -> Box<dyn Fn()> {
    Box::new(move || {
        let handler = handler.clone();
        let event = make_event();
        let callback: FrameCallback = Box::new(move |window, cx| {
            handler(&event, window, cx);
        });
        RefCell::borrow_mut(&nfc).push(callback);
        inv.set_dirty(true);
    })
}

/// Creates a `Fn(P)` callback that schedules an event handler via next_frame_callbacks.
/// Used for controls where the ObjC action provides a parameter (e.g., segment index, text).
pub(super) fn schedule_native_callback<P: 'static, Event: 'static>(
    handler: Rc<Box<dyn Fn(&Event, &mut Window, &mut App)>>,
    make_event: impl Fn(P) -> Event + 'static,
    nfc: Rc<RefCell<Vec<FrameCallback>>>,
    inv: WindowInvalidator,
) -> Box<dyn Fn(P)> {
    Box::new(move |param| {
        let handler = handler.clone();
        let event = make_event(param);
        let callback: FrameCallback = Box::new(move |window, cx| {
            handler(&event, window, cx);
        });
        RefCell::borrow_mut(&nfc).push(callback);
        inv.set_dirty(true);
    })
}

/// Creates a `Fn()` callback that schedules a focus/blur handler (no event parameter).
pub(super) fn schedule_native_focus_callback(
    handler: Rc<Box<dyn Fn(&mut Window, &mut App)>>,
    nfc: Rc<RefCell<Vec<FrameCallback>>>,
    inv: WindowInvalidator,
) -> Box<dyn Fn()> {
    Box::new(move || {
        let handler = handler.clone();
        let callback: FrameCallback = Box::new(move |window, cx| {
            handler(window, cx);
        });
        RefCell::borrow_mut(&nfc).push(callback);
        inv.set_dirty(true);
    })
}

/// Cleans up a native control by removing it from its parent view, releasing the
/// target/delegate, and releasing the control itself.
///
/// # Safety
/// Both pointers must be valid ObjC objects (or null for target_ptr).
#[cfg(target_os = "macos")]
pub(super) unsafe fn cleanup_native_control(
    control_ptr: *mut c_void,
    target_ptr: *mut c_void,
    release_target_fn: unsafe fn(*mut c_void),
    release_control_fn: unsafe fn(cocoa::base::id),
) {
    unsafe {
        crate::platform::native_controls::remove_native_view_from_parent(
            control_ptr as cocoa::base::id,
        );
        release_target_fn(target_ptr);
        release_control_fn(control_ptr as cocoa::base::id);
    }
}
