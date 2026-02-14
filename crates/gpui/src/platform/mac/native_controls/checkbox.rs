use super::CALLBACK_IVAR;
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
use std::{ffi::c_void, ptr};

// =============================================================================
// Checkbox target (fires Fn(bool) with the checked state)
// =============================================================================

static mut CHECKBOX_TARGET_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_checkbox_target_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUINativeCheckboxTarget", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);

        decl.add_method(
            sel!(checkboxAction:),
            checkbox_action as extern "C" fn(&Object, Sel, id),
        );

        CHECKBOX_TARGET_CLASS = decl.register();
    }
}

extern "C" fn checkbox_action(this: &Object, _sel: Sel, sender: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if !ptr.is_null() {
            let state: i64 = msg_send![sender, state];
            let callback = &*(ptr as *const Box<dyn Fn(bool)>);
            callback(state != 0);
        }
    }
}

// =============================================================================
// NSButton (checkbox mode) — creation & lifecycle
// =============================================================================

/// Creates a new checkbox-style NSButton with the given title.
pub(crate) unsafe fn create_native_checkbox(title: &str) -> id {
    unsafe {
        use super::super::ns_string;
        let checkbox: id = msg_send![class!(NSButton), alloc];
        let checkbox: id = msg_send![checkbox, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(140.0, 18.0),
        )];
        let _: () = msg_send![checkbox, setTitle: ns_string(title)];
        // NSButtonTypeSwitch = 3
        let _: () = msg_send![checkbox, setButtonType: 3i64];
        let _: () = msg_send![checkbox, setAutoresizingMask: 0u64];
        checkbox
    }
}

/// Updates the checkbox title.
pub(crate) unsafe fn set_native_checkbox_title(checkbox: id, title: &str) {
    unsafe {
        use super::super::ns_string;
        let _: () = msg_send![checkbox, setTitle: ns_string(title)];
    }
}

/// Sets whether the checkbox is currently checked.
pub(crate) unsafe fn set_native_checkbox_state(checkbox: id, checked: bool) {
    unsafe {
        let state: i64 = if checked { 1 } else { 0 };
        let _: () = msg_send![checkbox, setState: state];
    }
}

/// Sets target/action callback for a checkbox.
/// Returns a pointer to the target object.
pub(crate) unsafe fn set_native_checkbox_action(
    checkbox: id,
    callback: Box<dyn Fn(bool)>,
) -> *mut c_void {
    unsafe {
        let target: id = msg_send![CHECKBOX_TARGET_CLASS, alloc];
        let target: id = msg_send![target, init];

        let callback_ptr = Box::into_raw(Box::new(callback)) as *mut c_void;
        (*target).set_ivar::<*mut c_void>(CALLBACK_IVAR, callback_ptr);

        let _: () = msg_send![checkbox, setTarget: target];
        let _: () = msg_send![checkbox, setAction: sel!(checkboxAction:)];

        target as *mut c_void
    }
}

/// Releases the checkbox target and stored callback.
pub(crate) unsafe fn release_native_checkbox_target(target: *mut c_void) {
    unsafe {
        if !target.is_null() {
            let target = target as id;
            let callback_ptr: *mut c_void = *(*target).get_ivar(CALLBACK_IVAR);
            if !callback_ptr.is_null() {
                let _ = Box::from_raw(callback_ptr as *mut Box<dyn Fn(bool)>);
            }
            let _: () = msg_send![target, release];
        }
    }
}

/// Releases a checkbox control.
pub(crate) unsafe fn release_native_checkbox(checkbox: id) {
    unsafe {
        let _: () = msg_send![checkbox, release];
    }
}
