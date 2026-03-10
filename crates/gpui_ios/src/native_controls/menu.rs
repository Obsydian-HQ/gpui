// iOS menu button implementation using UIButton + UIMenu (iOS 14+).
// NSMenu doesn't exist on iOS. UIMenu is the equivalent.

use super::{id, nil, ns_string, CALLBACK_IVAR};
use block::ConcreteBlock;
use ctor::ctor;
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::Class,
    sel, sel_impl,
};
use std::{ffi::c_void, ptr};

static mut MENU_TARGET_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_menu_target_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUIiOSNativeMenuTarget", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);
        MENU_TARGET_CLASS = decl.register();
    }
}

/// Creates a UIButton configured to show a menu on tap.
pub(crate) unsafe fn create_native_menu_button(title: &str) -> id {
    unsafe {
        let button: id = msg_send![class!(UIButton), buttonWithType: 1i64];
        let _: () = msg_send![button, retain];
        let _: () = msg_send![button, setTitle: ns_string(title) forState: 0u64];
        let _: () = msg_send![button, setShowsMenuAsPrimaryAction: true as i8];
        button
    }
}

/// Creates a UIButton with a context menu (same as menu button on iOS).
pub(crate) unsafe fn create_native_context_menu_button(title: &str) -> id {
    unsafe {
        create_native_menu_button(title)
    }
}

/// Sets the menu button title.
pub(crate) unsafe fn set_native_menu_button_title(button: id, title: &str) {
    unsafe {
        let _: () = msg_send![button, setTitle: ns_string(title) forState: 0u64];
    }
}

/// Sets the menu items using UIMenu + UIAction.
pub(crate) unsafe fn set_native_menu_button_items(
    button: id,
    items: &[&str],
    target: *mut c_void,
) {
    unsafe {
        let mut actions: Vec<id> = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            let button = button;
            let target = target;
            let handler = ConcreteBlock::new(move |action: id| {
                if !target.is_null() {
                    let target = target as id;
                    let callback_ptr: *mut c_void = *(*target).get_ivar(CALLBACK_IVAR);
                    if !callback_ptr.is_null() {
                        let callback = &*(callback_ptr as *const Box<dyn Fn(usize)>);
                        callback(index);
                    }
                }

                let title: id = msg_send![action, title];
                let _: () = msg_send![button, setTitle: title forState: 0u64];
                let _: () = msg_send![button, setAccessibilityLabel: title];
            });
            let handler = handler.copy();
            let action: id = msg_send![class!(UIAction),
                actionWithTitle: ns_string(*item)
                image: nil
                identifier: nil
                handler: handler
            ];
            actions.push(action);
        }

        let array: id = msg_send![class!(NSArray),
            arrayWithObjects: actions.as_ptr()
            count: actions.len()
        ];
        let menu: id = msg_send![class!(UIMenu),
            menuWithTitle: ns_string("")
            children: array
        ];
        let _: () = msg_send![button, setMenu: menu];
    }
}

/// Creates a menu target for callbacks.
pub(crate) unsafe fn create_native_menu_target(
    callback: Box<dyn Fn(usize)>,
) -> *mut c_void {
    unsafe {
        let target: id = msg_send![MENU_TARGET_CLASS, alloc];
        let target: id = msg_send![target, init];
        let callback_ptr = Box::into_raw(Box::new(callback)) as *mut c_void;
        (*target).set_ivar::<*mut c_void>(CALLBACK_IVAR, callback_ptr);
        target as *mut c_void
    }
}

/// Releases the menu target.
pub(crate) unsafe fn release_native_menu_button_target(target: *mut c_void) {
    unsafe {
        if !target.is_null() {
            let target = target as id;
            let callback_ptr: *mut c_void = *(*target).get_ivar(CALLBACK_IVAR);
            if !callback_ptr.is_null() {
                let _ = Box::from_raw(callback_ptr as *mut Box<dyn Fn(usize)>);
            }
            let _: () = msg_send![target, release];
        }
    }
}

/// Releases a menu button.
pub(crate) unsafe fn release_native_menu_button(button: id) {
    unsafe {
        if !button.is_null() {
            let _: () = msg_send![button, release];
        }
    }
}
