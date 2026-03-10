use super::{id, ns_string, CALLBACK_IVAR, UI_CONTROL_EVENT_VALUE_CHANGED};
use ctor::ctor;
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Sel},
    sel, sel_impl,
};
use std::{ffi::c_void, ptr};

static mut STEPPER_TARGET_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_stepper_target_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUIiOSNativeStepperTarget", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);
        decl.add_method(
            sel!(stepperAction:),
            stepper_action as extern "C" fn(&Object, Sel, id),
        );
        STEPPER_TARGET_CLASS = decl.register();
    }
}

extern "C" fn stepper_action(this: &Object, _sel: Sel, sender: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if !ptr.is_null() {
            let value: f64 = msg_send![sender, value];
            let callback = &*(ptr as *const Box<dyn Fn(f64)>);
            callback(value);
        }
    }
}

/// Creates a new UIStepper.
pub(crate) unsafe fn create_native_stepper(min: f64, max: f64, value: f64, increment: f64) -> id {
    unsafe {
        let stepper: id = msg_send![class!(UIStepper), alloc];
        let stepper: id = msg_send![stepper, init];
        let _: () = msg_send![stepper, setMinimumValue: min];
        let _: () = msg_send![stepper, setMaximumValue: max];
        let _: () = msg_send![stepper, setValue: value];
        let _: () = msg_send![stepper, setStepValue: increment];

        // UIAccessibility — UIStepper already has correct default traits
        let _: () = msg_send![stepper, setAccessibilityLabel: ns_string("Stepper")];

        stepper
    }
}

pub(crate) unsafe fn set_native_stepper_min(stepper: id, min: f64) {
    unsafe { let _: () = msg_send![stepper, setMinimumValue: min]; }
}

pub(crate) unsafe fn set_native_stepper_max(stepper: id, max: f64) {
    unsafe { let _: () = msg_send![stepper, setMaximumValue: max]; }
}

pub(crate) unsafe fn set_native_stepper_value(stepper: id, value: f64) {
    unsafe {
        let _: () = msg_send![stepper, setValue: value];
        let value_str = format!("{:.1}", value);
        let _: () = msg_send![stepper, setAccessibilityValue: ns_string(&value_str)];
    }
}

pub(crate) unsafe fn set_native_stepper_increment(stepper: id, increment: f64) {
    unsafe { let _: () = msg_send![stepper, setStepValue: increment]; }
}

pub(crate) unsafe fn set_native_stepper_wraps(stepper: id, wraps: bool) {
    unsafe { let _: () = msg_send![stepper, setWraps: wraps as i8]; }
}

pub(crate) unsafe fn set_native_stepper_autorepeat(stepper: id, autorepeat: bool) {
    unsafe { let _: () = msg_send![stepper, setAutorepeat: autorepeat as i8]; }
}

pub(crate) unsafe fn set_native_stepper_action(
    stepper: id,
    callback: Box<dyn Fn(f64)>,
) -> *mut c_void {
    unsafe {
        let target: id = msg_send![STEPPER_TARGET_CLASS, alloc];
        let target: id = msg_send![target, init];

        let callback_ptr = Box::into_raw(Box::new(callback)) as *mut c_void;
        (*target).set_ivar::<*mut c_void>(CALLBACK_IVAR, callback_ptr);

        let _: () = msg_send![stepper,
            addTarget: target
            action: sel!(stepperAction:)
            forControlEvents: UI_CONTROL_EVENT_VALUE_CHANGED
        ];

        target as *mut c_void
    }
}

pub(crate) unsafe fn release_native_stepper_target(target: *mut c_void) {
    unsafe {
        if !target.is_null() {
            let target = target as id;
            let callback_ptr: *mut c_void = *(*target).get_ivar(CALLBACK_IVAR);
            if !callback_ptr.is_null() {
                let _ = Box::from_raw(callback_ptr as *mut Box<dyn Fn(f64)>);
            }
            let _: () = msg_send![target, release];
        }
    }
}

pub(crate) unsafe fn release_native_stepper(stepper: id) {
    unsafe {
        if !stepper.is_null() {
            let _: () = msg_send![stepper, release];
        }
    }
}
