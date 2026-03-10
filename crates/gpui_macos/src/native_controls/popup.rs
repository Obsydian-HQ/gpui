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
// NSPopUpButton target (fires Fn(usize) with selected item index)
// =============================================================================

static mut POPUP_TARGET_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_popup_target_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUINativePopupTarget", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);

        decl.add_method(
            sel!(popupAction:),
            popup_action as extern "C" fn(&Object, Sel, id),
        );

        POPUP_TARGET_CLASS = decl.register();
    }
}

extern "C" fn popup_action(this: &Object, _sel: Sel, sender: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if !ptr.is_null() {
            let selected: i64 = msg_send![sender, indexOfSelectedItem];
            if selected >= 0 {
                let callback = &*(ptr as *const Box<dyn Fn(usize)>);
                callback(selected as usize);
            }
        }
    }
}

// =============================================================================
// NSPopUpButton — creation & lifecycle
// =============================================================================

/// Creates a new popup button with items and selected index.
pub(crate) unsafe fn create_native_popup_button(items: &[&str], selected_index: usize) -> id {
    unsafe {
        let popup: id = msg_send![class!(NSPopUpButton), alloc];
        let popup: id = msg_send![popup, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(180.0, 24.0),
        ) pullsDown: 0i8];
        let _: () = msg_send![popup, setAutoresizingMask: 0u64];

        set_native_popup_items(popup, items);
        if !items.is_empty() {
            set_native_popup_selected(popup, selected_index.min(items.len() - 1));
        }

        popup
    }
}

/// Replaces all popup items.
pub(crate) unsafe fn set_native_popup_items(popup: id, items: &[&str]) {
    unsafe {
        use super::super::ns_string;
        let _: () = msg_send![popup, removeAllItems];
        for item in items {
            let _: () = msg_send![popup, addItemWithTitle: ns_string(item)];
        }
    }
}

/// Sets the selected popup item index.
pub(crate) unsafe fn set_native_popup_selected(popup: id, index: usize) {
    unsafe {
        let _: () = msg_send![popup, selectItemAtIndex: index as i64];
    }
}

/// Sets target/action callback for a popup button.
/// Returns a pointer to the target object.
pub(crate) unsafe fn set_native_popup_action(
    popup: id,
    callback: Box<dyn Fn(usize)>,
) -> *mut c_void {
    unsafe {
        let target: id = msg_send![POPUP_TARGET_CLASS, alloc];
        let target: id = msg_send![target, init];

        let callback_ptr = Box::into_raw(Box::new(callback)) as *mut c_void;
        (*target).set_ivar::<*mut c_void>(CALLBACK_IVAR, callback_ptr);

        let _: () = msg_send![popup, setTarget: target];
        let _: () = msg_send![popup, setAction: sel!(popupAction:)];

        target as *mut c_void
    }
}

/// Releases the popup target and stored callback.
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

/// Releases a popup button.
pub(crate) unsafe fn release_native_popup_button(popup: id) {
    unsafe {
        let _: () = msg_send![popup, release];
    }
}
