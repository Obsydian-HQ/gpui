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

// On iOS, we use UISegmentedControl as a tab selector (not UITabBarController,
// since we need an in-view tab control like NSTabView, not a full navigation paradigm).

static mut TAB_TARGET_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_tab_target_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUIiOSNativeTabTarget", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);
        decl.add_method(
            sel!(tabAction:),
            tab_action as extern "C" fn(&Object, Sel, id),
        );
        TAB_TARGET_CLASS = decl.register();
    }
}

extern "C" fn tab_action(this: &Object, _sel: Sel, sender: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if !ptr.is_null() {
            let index: isize = msg_send![sender, selectedSegmentIndex];
            if index >= 0 {
                let callback = &*(ptr as *const Box<dyn Fn(usize)>);
                callback(index as usize);
            }
        }
    }
}

/// Creates a UISegmentedControl configured as a tab view.
pub(crate) unsafe fn create_native_tab_view() -> id {
    unsafe {
        let control: id = msg_send![class!(UISegmentedControl), alloc];
        let control: id = msg_send![control, init];
        control
    }
}

/// Sets the tab items.
pub(crate) unsafe fn set_native_tab_view_items(tab_view: id, labels: &[&str]) {
    unsafe {
        // Remove all existing segments
        let _: () = msg_send![tab_view, removeAllSegments];

        for (i, label) in labels.iter().enumerate() {
            let _: () = msg_send![tab_view,
                insertSegmentWithTitle: ns_string(label)
                atIndex: i as u64
                animated: false as i8
            ];
        }
    }
}

/// Sets the selected tab.
pub(crate) unsafe fn set_native_tab_view_selected(tab_view: id, index: usize) {
    unsafe {
        let _: () = msg_send![tab_view, setSelectedSegmentIndex: index as isize];
    }
}

/// Sets the tab selection callback.
pub(crate) unsafe fn set_native_tab_view_action(
    tab_view: id,
    callback: Box<dyn Fn(usize)>,
) -> *mut c_void {
    unsafe {
        let target: id = msg_send![TAB_TARGET_CLASS, alloc];
        let target: id = msg_send![target, init];

        let callback_ptr = Box::into_raw(Box::new(callback)) as *mut c_void;
        (*target).set_ivar::<*mut c_void>(CALLBACK_IVAR, callback_ptr);

        let _: () = msg_send![tab_view,
            addTarget: target
            action: sel!(tabAction:)
            forControlEvents: UI_CONTROL_EVENT_VALUE_CHANGED
        ];

        target as *mut c_void
    }
}

pub(crate) unsafe fn release_native_tab_view_target(target: *mut c_void) {
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

pub(crate) unsafe fn release_native_tab_view(tab_view: id) {
    unsafe {
        if !tab_view.is_null() {
            let _: () = msg_send![tab_view, release];
        }
    }
}
