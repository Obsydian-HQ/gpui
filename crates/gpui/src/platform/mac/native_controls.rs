use super::ns_string;
use crate::{Bounds, Pixels};
use cocoa::{
    base::id,
    foundation::{NSPoint, NSRect, NSSize},
};
use ctor::ctor;
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Sel},
    sel, sel_impl,
};
use std::{
    ffi::c_void,
    ptr,
};

const CALLBACK_IVAR: &str = "callbackPtr";

static mut BUTTON_TARGET_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_button_target_class() {
    unsafe {
        let mut decl =
            ClassDecl::new("GPUINativeButtonTarget", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);

        decl.add_method(
            sel!(buttonAction:),
            button_action as extern "C" fn(&Object, Sel, id),
        );

        BUTTON_TARGET_CLASS = decl.register();
    }
}

extern "C" fn button_action(this: &Object, _sel: Sel, _sender: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if !ptr.is_null() {
            let callback = &*(ptr as *const Box<dyn Fn()>);
            callback();
        }
    }
}

/// Creates a new NSButton with the given title. The button is not yet added to any view.
pub(crate) unsafe fn create_native_button(title: &str) -> id {
    unsafe {
        let button: id = msg_send![class!(NSButton), alloc];
        let button: id = msg_send![button, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(100.0, 24.0),
        )];
        let _: () = msg_send![button, setTitle: ns_string(title)];
        // NSBezelStyleRounded = 1
        let _: () = msg_send![button, setBezelStyle: 1i64];
        let _: () = msg_send![button, setAutoresizingMask: 0u64];
        button
    }
}

/// Adds the button as a subview of the given parent view.
pub(crate) unsafe fn attach_native_button_to_view(button: id, parent: id) {
    unsafe {
        let _: () = msg_send![parent, addSubview: button];
    }
}

/// Positions the button within its parent view, converting from GPUI's top-down coordinate
/// system to NSView's bottom-up coordinate system.
pub(crate) unsafe fn set_native_button_frame(
    button: id,
    bounds: Bounds<Pixels>,
    parent_view: id,
    _scale_factor: f32,
) {
    unsafe {
        // Get parent view bounds to flip y-axis
        let parent_frame: NSRect = msg_send![parent_view, frame];
        let parent_height = parent_frame.size.height;

        let x = bounds.origin.x.0 as f64;
        let y = bounds.origin.y.0 as f64;
        let w = bounds.size.width.0 as f64;
        let h = bounds.size.height.0 as f64;

        // NSView y-axis is bottom-up, GPUI is top-down
        let flipped_y = parent_height - y - h;

        let frame = NSRect::new(NSPoint::new(x, flipped_y), NSSize::new(w, h));
        let _: () = msg_send![button, setFrame: frame];
    }
}

/// Updates the button's title.
pub(crate) unsafe fn set_native_button_title(button: id, title: &str) {
    unsafe {
        let _: () = msg_send![button, setTitle: ns_string(title)];
    }
}

/// Sets the button's target/action to invoke a Rust callback.
/// Returns a pointer to the target object (must be retained for the callback to work).
pub(crate) unsafe fn set_native_button_action(
    button: id,
    callback: Box<dyn Fn()>,
) -> *mut c_void {
    unsafe {
        let target: id = msg_send![BUTTON_TARGET_CLASS, alloc];
        let target: id = msg_send![target, init];

        // Store the callback as a heap-allocated Box<Box<dyn Fn()>>
        let callback_ptr = Box::into_raw(Box::new(callback)) as *mut c_void;
        (*target).set_ivar::<*mut c_void>(CALLBACK_IVAR, callback_ptr);

        let _: () = msg_send![button, setTarget: target];
        let _: () = msg_send![button, setAction: sel!(buttonAction:)];

        target as *mut c_void
    }
}

/// Removes the button from its parent view.
pub(crate) unsafe fn remove_native_button_from_view(button: id) {
    unsafe {
        let _: () = msg_send![button, removeFromSuperview];
    }
}

/// Releases the target object and frees the stored callback.
pub(crate) unsafe fn release_native_button_target(target: *mut c_void) {
    unsafe {
        if !target.is_null() {
            let target = target as id;
            let callback_ptr: *mut c_void = *(*target).get_ivar(CALLBACK_IVAR);
            if !callback_ptr.is_null() {
                // Reconstruct and drop the Box<dyn Fn()>
                let _ = Box::from_raw(callback_ptr as *mut Box<dyn Fn()>);
            }
            let _: () = msg_send![target, release];
        }
    }
}

/// Releases the NSButton itself.
pub(crate) unsafe fn release_native_button(button: id) {
    unsafe {
        let _: () = msg_send![button, release];
    }
}
