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

static mut SEGMENTED_TARGET_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_segmented_target_class() {
    unsafe {
        let mut decl =
            ClassDecl::new("GPUIiOSNativeSegmentedTarget", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);
        decl.add_method(
            sel!(segmentAction:),
            segment_action as extern "C" fn(&Object, Sel, id),
        );
        SEGMENTED_TARGET_CLASS = decl.register();
    }
}

extern "C" fn segment_action(this: &Object, _sel: Sel, sender: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if !ptr.is_null() {
            let index: isize = msg_send![sender, selectedSegmentIndex];
            let callback = &*(ptr as *const Box<dyn Fn(usize)>);
            if index >= 0 {
                callback(index as usize);
            }
        }
    }
}

/// Creates a new UISegmentedControl with the given labels.
pub(crate) unsafe fn create_native_segmented_control(
    labels: &[&str],
    selected_index: Option<usize>,
) -> id {
    unsafe {
        // Build an NSArray of NSString items
        let mut items: Vec<id> = Vec::with_capacity(labels.len());
        for label in labels {
            items.push(ns_string(label));
        }
        let array: id = msg_send![class!(NSArray),
            arrayWithObjects: items.as_ptr()
            count: items.len()
        ];

        let control: id = msg_send![class!(UISegmentedControl), alloc];
        let control: id = msg_send![control, initWithItems: array];

        if let Some(idx) = selected_index {
            let _: () = msg_send![control, setSelectedSegmentIndex: idx as isize];
        }

        // UIAccessibility — label each segment individually
        let _: () = msg_send![control, setAccessibilityLabel: ns_string("Segmented Control")];

        control
    }
}

/// Sets the selected segment.
pub(crate) unsafe fn set_native_segmented_selected(control: id, index: Option<usize>) {
    unsafe {
        let idx: isize = match index {
            Some(i) => i as isize,
            None => -1, // UISegmentedControlNoSegment
        };
        let _: () = msg_send![control, setSelectedSegmentIndex: idx];
    }
}

/// No-op on iOS (UISegmentedControl doesn't have border shape).
pub(crate) unsafe fn set_native_segmented_border_shape(_control: id, _shape: i64) {}

/// Sets the control size. On iOS, we adjust the height via content size.
pub(crate) unsafe fn set_native_segmented_control_size(_control: id, _size: u64) {
    // UISegmentedControl auto-sizes on iOS.
}

/// Sets an SF Symbol image for a specific segment.
pub(crate) unsafe fn set_native_segmented_image(control: id, segment: usize, symbol_name: &str) {
    unsafe {
        let image: id = msg_send![class!(UIImage), systemImageNamed: ns_string(symbol_name)];
        if !image.is_null() {
            let _: () = msg_send![control, setImage: image forSegmentAtIndex: segment as u64];
        }
    }
}

/// Sets the target/action callback.
pub(crate) unsafe fn set_native_segmented_action(
    control: id,
    callback: Box<dyn Fn(usize)>,
) -> *mut c_void {
    unsafe {
        let target: id = msg_send![SEGMENTED_TARGET_CLASS, alloc];
        let target: id = msg_send![target, init];

        let callback_ptr = Box::into_raw(Box::new(callback)) as *mut c_void;
        (*target).set_ivar::<*mut c_void>(CALLBACK_IVAR, callback_ptr);

        let _: () = msg_send![control,
            addTarget: target
            action: sel!(segmentAction:)
            forControlEvents: UI_CONTROL_EVENT_VALUE_CHANGED
        ];

        target as *mut c_void
    }
}

pub(crate) unsafe fn release_native_segmented_target(target: *mut c_void) {
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

pub(crate) unsafe fn release_native_segmented_control(control: id) {
    unsafe {
        if !control.is_null() {
            let _: () = msg_send![control, release];
        }
    }
}
