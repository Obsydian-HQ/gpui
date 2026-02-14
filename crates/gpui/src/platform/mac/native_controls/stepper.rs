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

static mut STEPPER_TARGET_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_stepper_target_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUINativeStepperTarget", class!(NSObject)).unwrap();
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
            let value: f64 = msg_send![sender, doubleValue];
            let callback = &*(ptr as *const Box<dyn Fn(f64)>);
            callback(value);
        }
    }
}

/// Creates a new NSStepper.
pub(crate) unsafe fn create_native_stepper(min: f64, max: f64, value: f64, increment: f64) -> id {
    unsafe {
        let stepper: id = msg_send![class!(NSStepper), alloc];
        let stepper: id = msg_send![stepper, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(20.0, 24.0),
        )];
        let _: () = msg_send![stepper, setAutoresizingMask: 0u64];
        let _: () = msg_send![stepper, setMinValue: min];
        let _: () = msg_send![stepper, setMaxValue: max];
        let _: () = msg_send![stepper, setDoubleValue: value];
        let _: () = msg_send![stepper, setIncrement: increment];
        let _: () = msg_send![stepper, setAutorepeat: 1i8];
        let _: () = msg_send![stepper, setValueWraps: 0i8];
        stepper
    }
}

pub(crate) unsafe fn set_native_stepper_min(stepper: id, min: f64) {
    unsafe {
        let _: () = msg_send![stepper, setMinValue: min];
    }
}

pub(crate) unsafe fn set_native_stepper_max(stepper: id, max: f64) {
    unsafe {
        let _: () = msg_send![stepper, setMaxValue: max];
    }
}

pub(crate) unsafe fn set_native_stepper_value(stepper: id, value: f64) {
    unsafe {
        let _: () = msg_send![stepper, setDoubleValue: value];
    }
}

pub(crate) unsafe fn set_native_stepper_increment(stepper: id, increment: f64) {
    unsafe {
        let _: () = msg_send![stepper, setIncrement: increment];
    }
}

pub(crate) unsafe fn set_native_stepper_wraps(stepper: id, wraps: bool) {
    unsafe {
        let _: () = msg_send![stepper, setValueWraps: wraps as i8];
    }
}

pub(crate) unsafe fn set_native_stepper_autorepeat(stepper: id, autorepeat: bool) {
    unsafe {
        let _: () = msg_send![stepper, setAutorepeat: autorepeat as i8];
    }
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

        let _: () = msg_send![stepper, setTarget: target];
        let _: () = msg_send![stepper, setAction: sel!(stepperAction:)];

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
        let _: () = msg_send![stepper, release];
    }
}
