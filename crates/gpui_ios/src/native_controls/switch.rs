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

static mut SWITCH_TARGET_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_switch_target_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUIiOSNativeSwitchTarget", class!(NSObject)).unwrap();
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
            let is_on: bool = msg_send![sender, isOn];
            let callback = &*(ptr as *const Box<dyn Fn(bool)>);
            callback(is_on);
        }
    }
}

/// Creates a new UISwitch.
pub(crate) unsafe fn create_native_switch() -> id {
    unsafe {
        let switch: id = msg_send![class!(UISwitch), alloc];
        let switch: id = msg_send![switch, init];

        // UIAccessibility — UISwitch already has correct default traits
        let _: () = msg_send![switch, setAccessibilityLabel: ns_string("Switch")];
        let _: () = msg_send![switch, setAccessibilityValue: ns_string("off")];

        switch
    }
}

/// Sets whether the switch is on.
pub(crate) unsafe fn set_native_switch_state(switch: id, checked: bool) {
    unsafe {
        let _: () = msg_send![switch, setOn: checked as i8 animated: false as i8];
        let value_str = if checked { "on" } else { "off" };
        let _: () = msg_send![switch, setAccessibilityValue: ns_string(value_str)];
    }
}

/// Sets target/action callback for the switch.
pub(crate) unsafe fn set_native_switch_action(
    switch: id,
    callback: Box<dyn Fn(bool)>,
) -> *mut c_void {
    unsafe {
        let target: id = msg_send![SWITCH_TARGET_CLASS, alloc];
        let target: id = msg_send![target, init];

        let callback_ptr = Box::into_raw(Box::new(callback)) as *mut c_void;
        (*target).set_ivar::<*mut c_void>(CALLBACK_IVAR, callback_ptr);

        let _: () = msg_send![switch,
            addTarget: target
            action: sel!(switchAction:)
            forControlEvents: UI_CONTROL_EVENT_VALUE_CHANGED
        ];

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

/// Releases a UISwitch.
pub(crate) unsafe fn release_native_switch(switch: id) {
    unsafe {
        if !switch.is_null() {
            let _: () = msg_send![switch, release];
        }
    }
}
