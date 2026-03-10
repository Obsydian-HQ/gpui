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

struct TabViewCallbacks {
    on_select: Option<Box<dyn Fn(usize)>>,
}

static mut TAB_VIEW_DELEGATE_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_tab_view_delegate_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUINativeTabViewDelegate", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);

        decl.add_method(
            sel!(tabView:didSelectTabViewItem:),
            did_select_tab_item as extern "C" fn(&Object, Sel, id, id),
        );

        TAB_VIEW_DELEGATE_CLASS = decl.register();
    }
}

extern "C" fn did_select_tab_item(this: &Object, _sel: Sel, tab_view: id, tab_item: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if ptr.is_null() {
            return;
        }

        let callbacks = &*(ptr as *const TabViewCallbacks);
        if let Some(ref on_select) = callbacks.on_select {
            let index: i64 = msg_send![tab_view, indexOfTabViewItem: tab_item];
            if index >= 0 {
                on_select(index as usize);
            }
        }
    }
}

pub(crate) unsafe fn create_native_tab_view() -> id {
    unsafe {
        let tab_view: id = msg_send![class!(NSTabView), alloc];
        let tab_view: id = msg_send![tab_view, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(360.0, 220.0),
        )];
        let _: () = msg_send![tab_view, setAutoresizingMask: 0u64];
        tab_view
    }
}

pub(crate) unsafe fn set_native_tab_view_items(
    tab_view: id,
    labels: &[&str],
    selected_index: usize,
) {
    unsafe {
        use super::super::ns_string;

        let count: i64 = msg_send![tab_view, numberOfTabViewItems];
        for _ in 0..count {
            let item: id = msg_send![tab_view, tabViewItemAtIndex: 0i64];
            let _: () = msg_send![tab_view, removeTabViewItem: item];
        }

        for label in labels {
            let item: id = msg_send![class!(NSTabViewItem), alloc];
            let item: id = msg_send![item, initWithIdentifier: ns_string(label)];
            let _: () = msg_send![item, setLabel: ns_string(label)];

            let content: id = msg_send![class!(NSView), alloc];
            let content: id = msg_send![content, initWithFrame: NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(320.0, 180.0),
            )];
            let _: () = msg_send![item, setView: content];
            let _: () = msg_send![content, release];

            let _: () = msg_send![tab_view, addTabViewItem: item];
            let _: () = msg_send![item, release];
        }

        if !labels.is_empty() {
            let clamped = selected_index.min(labels.len() - 1);
            let _: () = msg_send![tab_view, selectTabViewItemAtIndex: clamped as i64];
        }
    }
}

pub(crate) unsafe fn set_native_tab_view_selected(tab_view: id, index: usize) {
    unsafe {
        let count: i64 = msg_send![tab_view, numberOfTabViewItems];
        if count <= 0 {
            return;
        }

        let clamped = (index as i64).min(count - 1).max(0);
        let _: () = msg_send![tab_view, selectTabViewItemAtIndex: clamped];
    }
}

pub(crate) unsafe fn set_native_tab_view_action(
    tab_view: id,
    on_select: Option<Box<dyn Fn(usize)>>,
) -> *mut c_void {
    unsafe {
        let delegate: id = msg_send![TAB_VIEW_DELEGATE_CLASS, alloc];
        let delegate: id = msg_send![delegate, init];

        let callbacks = TabViewCallbacks { on_select };
        let callbacks_ptr = Box::into_raw(Box::new(callbacks)) as *mut c_void;
        (*delegate).set_ivar::<*mut c_void>(CALLBACK_IVAR, callbacks_ptr);

        let _: () = msg_send![tab_view, setDelegate: delegate];

        delegate as *mut c_void
    }
}

pub(crate) unsafe fn release_native_tab_view_target(target: *mut c_void) {
    unsafe {
        if target.is_null() {
            return;
        }

        let delegate = target as id;
        let callbacks_ptr: *mut c_void = *(*delegate).get_ivar(CALLBACK_IVAR);
        if !callbacks_ptr.is_null() {
            let _ = Box::from_raw(callbacks_ptr as *mut TabViewCallbacks);
        }

        let _: () = msg_send![delegate, release];
    }
}

pub(crate) unsafe fn release_native_tab_view(tab_view: id) {
    unsafe {
        let _: () = msg_send![tab_view, setDelegate: ptr::null_mut::<c_void>() as id];
        let _: () = msg_send![tab_view, release];
    }
}
