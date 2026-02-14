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

static mut SWITCH_TARGET_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_switch_target_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUINativeSwitchTarget", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);

        decl.add_method(
            sel!(switchAction:),
            switch_action as extern "C" fn(&Object, Sel, id),
        );

        SWITCH_TARGET_CLASS = decl.register();
    }
}

extern "C" fn switch_action(this: &Object, _sel: Sel, sender: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if !ptr.is_null() {
            let state: i64 = msg_send![sender, state];
            let callback = &*(ptr as *const Box<dyn Fn(bool)>);
            callback(state != 0);
        }
    }
}

/// Creates a new NSSwitch.
pub(crate) unsafe fn create_native_switch() -> id {
    unsafe {
        let switch: id = msg_send![class!(NSSwitch), alloc];
        let switch: id = msg_send![switch, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(38.0, 22.0),
        )];
        let _: () = msg_send![switch, setAutoresizingMask: 0u64];
        switch
    }
}

/// Sets whether the switch is on.
pub(crate) unsafe fn set_native_switch_state(switch: id, checked: bool) {
    unsafe {
        let state: i64 = if checked { 1 } else { 0 };
        let _: () = msg_send![switch, setState: state];
    }
}

/// Sets target/action callback for a switch.
pub(crate) unsafe fn set_native_switch_action(
    switch: id,
    callback: Box<dyn Fn(bool)>,
) -> *mut c_void {
    unsafe {
        let target: id = msg_send![SWITCH_TARGET_CLASS, alloc];
        let target: id = msg_send![target, init];

        let callback_ptr = Box::into_raw(Box::new(callback)) as *mut c_void;
        (*target).set_ivar::<*mut c_void>(CALLBACK_IVAR, callback_ptr);

        let _: () = msg_send![switch, setTarget: target];
        let _: () = msg_send![switch, setAction: sel!(switchAction:)];

        target as *mut c_void
    }
}

/// Releases the switch target and callback.
pub(crate) unsafe fn release_native_switch_target(target: *mut c_void) {
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

/// Releases an NSSwitch.
pub(crate) unsafe fn release_native_switch(switch: id) {
    unsafe {
        let _: () = msg_send![switch, release];
    }
}
