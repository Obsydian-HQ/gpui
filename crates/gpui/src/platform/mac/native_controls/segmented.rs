use super::CALLBACK_IVAR;
use cocoa::{
    base::{id, nil},
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
// Segmented-control target (fires Fn(usize) with the selected segment index)
// =============================================================================

static mut SEGMENTED_TARGET_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_segmented_target_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUINativeSegmentedTarget", class!(NSObject)).unwrap();
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
            let selected: i64 = msg_send![sender, selectedSegment];
            let callback = &*(ptr as *const Box<dyn Fn(usize)>);
            callback(selected as usize);
        }
    }
}

// =============================================================================
// NSSegmentedControl — creation & lifecycle
// =============================================================================

/// Creates a new NSSegmentedControl with the given labels.
pub(crate) unsafe fn create_native_segmented_control(labels: &[&str], selected_index: usize) -> id {
    unsafe {
        use super::super::ns_string;
        let control: id = msg_send![class!(NSSegmentedControl), alloc];
        let control: id = msg_send![control, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(200.0, 24.0),
        )];
        let _: () = msg_send![control, setSegmentCount: labels.len() as i64];
        for (i, label) in labels.iter().enumerate() {
            let _: () = msg_send![control, setLabel: ns_string(label) forSegment: i as i64];
        }
        // NSSegmentSwitchTrackingSelectOne = 0
        let _: () = msg_send![control, setTrackingMode: 0i64];
        let _: () = msg_send![control, setSelectedSegment: selected_index as i64];
        let _: () = msg_send![control, setAutoresizingMask: 0u64];
        // NSSegmentStyleAutomatic = 0
        let _: () = msg_send![control, setSegmentStyle: 0i64];
        control
    }
}

/// Sets the selected segment.
pub(crate) unsafe fn set_native_segmented_selected(control: id, index: usize) {
    unsafe {
        let _: () = msg_send![control, setSelectedSegment: index as i64];
    }
}

/// Sets the segmented control style.
/// 0 = Automatic, 1 = Rounded, 3 = RoundRect, 5 = Capsule, 8 = Separated.
pub(crate) unsafe fn set_native_segmented_style(control: id, style: i64) {
    unsafe {
        let _: () = msg_send![control, setSegmentStyle: style];
        let _: () = msg_send![control, setNeedsDisplay: true];
    }
}

/// Sets an SF Symbol image on a specific segment and clears its text label (macOS 11+).
pub(crate) unsafe fn set_native_segmented_image(control: id, segment: usize, symbol_name: &str) {
    unsafe {
        use super::super::ns_string;
        let image: id = msg_send![
            class!(NSImage),
            imageWithSystemSymbolName: ns_string(symbol_name)
            accessibilityDescription: nil
        ];
        if image != nil {
            let _: () = msg_send![control, setImage: image forSegment: segment as i64];
            let _: () = msg_send![control, setLabel: ns_string("") forSegment: segment as i64];
        }
    }
}

/// Sets the target/action for a segmented control. The callback receives the selected index.
/// Returns a pointer to the target object.
pub(crate) unsafe fn set_native_segmented_action(
    control: id,
    callback: Box<dyn Fn(usize)>,
) -> *mut c_void {
    unsafe {
        let target: id = msg_send![SEGMENTED_TARGET_CLASS, alloc];
        let target: id = msg_send![target, init];

        let callback_ptr = Box::into_raw(Box::new(callback)) as *mut c_void;
        (*target).set_ivar::<*mut c_void>(CALLBACK_IVAR, callback_ptr);

        let _: () = msg_send![control, setTarget: target];
        let _: () = msg_send![control, setAction: sel!(segmentAction:)];

        target as *mut c_void
    }
}

/// Releases the segmented target and frees the stored `Box<dyn Fn(usize)>` callback.
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

/// Releases an NSSegmentedControl.
pub(crate) unsafe fn release_native_segmented_control(control: id) {
    unsafe {
        let _: () = msg_send![control, release];
    }
}
