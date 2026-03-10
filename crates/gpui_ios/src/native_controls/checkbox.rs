use super::{
    id, ns_string, CALLBACK_IVAR, UI_CONTROL_EVENT_TOUCH_UP_INSIDE, UI_CONTROL_EVENT_VALUE_CHANGED,
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

static mut CHECKBOX_TARGET_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_checkbox_target_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUIiOSNativeCheckboxTarget", class!(NSObject)).unwrap();
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
            let callback = &*(ptr as *const Box<dyn Fn(bool)>);
            if msg_send![sender, respondsToSelector: sel!(isOn)] {
                let is_on: bool = msg_send![sender, isOn];
                callback(is_on);
            } else {
                // UIButton fallback doesn't toggle selected automatically.
                let was_selected: bool = msg_send![sender, isSelected];
                let checked = !was_selected;
                let _: () = msg_send![sender, setSelected: checked as i8];
                set_fallback_checkbox_image(sender, checked);
                callback(checked);
            }
        }
    }
}

unsafe fn set_fallback_checkbox_image(checkbox: id, checked: bool) {
    unsafe {
        let symbol = if checked {
            "checkmark.square.fill"
        } else {
            "square"
        };
        let image: id = msg_send![class!(UIImage), systemImageNamed: ns_string(symbol)];
        if !image.is_null() {
            let _: () = msg_send![checkbox, setImage: image forState: 0u64];
        }
    }
}

/// Creates a native checkbox control.
///
/// Uses checkbox-style `UISwitch` for Mac idiom (Catalyst).
/// Falls back to a UIButton-based checkbox on iPhone/iPad, since UIKit doesn't
/// provide a standalone checkbox control there.
pub(crate) unsafe fn create_native_checkbox(title: &str) -> id {
    unsafe {
        let checkbox: id = msg_send![class!(UISwitch), alloc];
        let checkbox: id = msg_send![checkbox, init];

        let device: id = msg_send![class!(UIDevice), currentDevice];
        let idiom: i64 = msg_send![device, userInterfaceIdiom];
        let is_mac_idiom = idiom == 5; // UIUserInterfaceIdiomMac

        if is_mac_idiom && msg_send![checkbox, respondsToSelector: sel!(setPreferredStyle:)] {
            // UISwitchStyleCheckbox = 1 (Catalyst Mac idiom only)
            let _: () = msg_send![checkbox, setPreferredStyle: 1i64];
            if msg_send![checkbox, respondsToSelector: sel!(setTitle:)] {
                let _: () = msg_send![checkbox, setTitle: ns_string(title)];
            }
            let _: () = msg_send![checkbox, setAccessibilityLabel: ns_string(title)];
            return checkbox;
        }

        let _: () = msg_send![checkbox, release];

        let button: id = msg_send![class!(UIButton), buttonWithType: 1i64];
        let _: () = msg_send![button, retain];
        let _: () = msg_send![button, setTitle: ns_string(title) forState: 0u64];
        set_fallback_checkbox_image(button, false);
        let _: () = msg_send![button, setAccessibilityLabel: ns_string(title)];
        button
    }
}

/// Sets the checkbox title.
pub(crate) unsafe fn set_native_checkbox_title(checkbox: id, title: &str) {
    unsafe {
        if msg_send![checkbox, respondsToSelector: sel!(setTitle:forState:)] {
            let _: () = msg_send![checkbox, setTitle: ns_string(title) forState: 0u64];
        } else if msg_send![checkbox, respondsToSelector: sel!(setTitle:)] {
            let _: () = msg_send![checkbox, setTitle: ns_string(title)];
        }
        let _: () = msg_send![checkbox, setAccessibilityLabel: ns_string(title)];
    }
}

/// Sets the checked state.
pub(crate) unsafe fn set_native_checkbox_state(checkbox: id, checked: bool) {
    unsafe {
        if msg_send![checkbox, respondsToSelector: sel!(setOn:animated:)] {
            let _: () = msg_send![checkbox, setOn: checked as i8 animated: false as i8];
        } else {
            let _: () = msg_send![checkbox, setSelected: checked as i8];
            set_fallback_checkbox_image(checkbox, checked);
        }
    }
}

/// Sets target/action for the checkbox toggle.
pub(crate) unsafe fn set_native_checkbox_action(
    checkbox: id,
    callback: Box<dyn Fn(bool)>,
) -> *mut c_void {
    unsafe {
        let target: id = msg_send![CHECKBOX_TARGET_CLASS, alloc];
        let target: id = msg_send![target, init];

        let callback_ptr = Box::into_raw(Box::new(callback)) as *mut c_void;
        (*target).set_ivar::<*mut c_void>(CALLBACK_IVAR, callback_ptr);

        let control_events = if msg_send![checkbox, respondsToSelector: sel!(isOn)] {
            UI_CONTROL_EVENT_VALUE_CHANGED
        } else {
            UI_CONTROL_EVENT_TOUCH_UP_INSIDE
        };

        let _: () = msg_send![checkbox,
            addTarget: target
            action: sel!(checkboxAction:)
            forControlEvents: control_events
        ];

        target as *mut c_void
    }
}

/// Releases the checkbox target and callback.
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

/// Releases the checkbox.
pub(crate) unsafe fn release_native_checkbox(checkbox: id) {
    unsafe {
        if !checkbox.is_null() {
            let _: () = msg_send![checkbox, release];
        }
    }
}
