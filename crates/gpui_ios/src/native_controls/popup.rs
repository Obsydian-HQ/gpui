use super::{id, ns_string, CALLBACK_IVAR};
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

static mut POPUP_TARGET_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_popup_target_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUIiOSNativePopupTarget", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);
        // Store items count so we can determine which was selected
        POPUP_TARGET_CLASS = decl.register();
    }
}

/// Creates a UIButton configured as a dropdown/popup using UIMenu (iOS 14+).
pub(crate) unsafe fn create_native_popup_button(items: &[&str], selected_index: usize) -> id {
    unsafe {
        let button: id = msg_send![class!(UIButton), buttonWithType: 1i64];
        let _: () = msg_send![button, retain];

        // Set initial title to the selected item
        let initial_title = items.get(selected_index).copied().unwrap_or("Popup");
        let _: () = msg_send![button, setTitle: ns_string(initial_title) forState: 0u64];

        // Show a dropdown indicator
        let _: () = msg_send![button, setShowsMenuAsPrimaryAction: true as i8];

        // UIAccessibility — UIButton already has UIAccessibilityTraitButton by default
        let _: () = msg_send![button, setAccessibilityLabel: ns_string(initial_title)];
        let _: () = msg_send![button, setAccessibilityHint: ns_string("Double tap to show options")];

        button
    }
}

/// Updates the popup items using UIMenu.
pub(crate) unsafe fn set_native_popup_items(popup: id, items: &[&str], target: *mut c_void) {
    unsafe {
        let mut actions: Vec<id> = Vec::with_capacity(items.len());
        for (i, item) in items.iter().enumerate() {
            let title = ns_string(item);
            let popup = popup;
            let target = target;
            let handler = ConcreteBlock::new(move |action: id| {
                if !target.is_null() {
                    let target = target as id;
                    let callback_ptr: *mut c_void = *(*target).get_ivar(CALLBACK_IVAR);
                    if !callback_ptr.is_null() {
                        let callback = &*(callback_ptr as *const Box<dyn Fn(usize)>);
                        callback(i);
                    }
                }

                let title: id = msg_send![action, title];
                let _: () = msg_send![popup, setTitle: title forState: 0u64];
                let _: () = msg_send![popup, setAccessibilityLabel: title];
            });
            let handler = handler.copy();
            let action: id = msg_send![class!(UIAction),
                actionWithTitle: title
                image: super::nil
                identifier: super::nil
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
        let _: () = msg_send![popup, setMenu: menu];
    }
}

/// Sets the selected item by updating the button title.
pub(crate) unsafe fn set_native_popup_selected(popup: id, index: usize) {
    unsafe {
        // Get the menu and read the title of the selected action
        let menu: id = msg_send![popup, menu];
        if !menu.is_null() {
            let children: id = msg_send![menu, children];
            let count: usize = msg_send![children, count];
            if index < count {
                let action: id = msg_send![children, objectAtIndex: index];
                let title: id = msg_send![action, title];
                let _: () = msg_send![popup, setTitle: title forState: 0u64];
                let _: () = msg_send![popup, setAccessibilityLabel: title];
            }
        }
    }
}

/// Sets the popup action callback.
pub(crate) unsafe fn set_native_popup_action(
    callback: Box<dyn Fn(usize)>,
) -> *mut c_void {
    unsafe {
        let target: id = msg_send![POPUP_TARGET_CLASS, alloc];
        let target: id = msg_send![target, init];

        let callback_ptr = Box::into_raw(Box::new(callback)) as *mut c_void;
        (*target).set_ivar::<*mut c_void>(CALLBACK_IVAR, callback_ptr);
        target as *mut c_void
    }
}

pub(crate) unsafe fn release_native_popup_target(target: *mut c_void) {
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

pub(crate) unsafe fn release_native_popup_button(popup: id) {
    unsafe {
        if !popup.is_null() {
            let _: () = msg_send![popup, release];
        }
    }
}
