use super::CALLBACK_IVAR;
use cocoa::base::{id, nil};
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
///
/// Uses the `+segmentedControlWithLabels:trackingMode:target:action:` convenience
/// constructor (macOS 10.12+) which properly initializes the cell and drawing
/// infrastructure, ensuring `setSegmentStyle:` works correctly.
pub(crate) unsafe fn create_native_segmented_control(
    labels: &[&str],
    selected_index: Option<usize>,
) -> id {
    unsafe {
        use super::super::ns_string;

        // Build an NSArray of NSString labels
        let labels_array: id =
            msg_send![class!(NSMutableArray), arrayWithCapacity: labels.len() as u64];
        for label in labels {
            let _: () = msg_send![labels_array, addObject: ns_string(label)];
        }

        // NSSegmentSwitchTrackingSelectOne = 0
        let control: id = msg_send![
            class!(NSSegmentedControl),
            segmentedControlWithLabels: labels_array
            trackingMode: 0i64
            target: nil
            action: nil
        ];

        // The convenience constructor returns an autoreleased object; retain it
        // so our manual release in `release_native_segmented_control` balances.
        let _: () = msg_send![control, retain];

        let selected: i64 = selected_index.map_or(-1, |i| i as i64);
        let _: () = msg_send![control, setSelectedSegment: selected];
        let _: () = msg_send![control, setAutoresizingMask: 0u64];
        control
    }
}

/// Sets the selected segment. Pass `None` to deselect all segments.
pub(crate) unsafe fn set_native_segmented_selected(control: id, index: Option<usize>) {
    unsafe {
        let selected: i64 = index.map_or(-1, |i| i as i64);
        let _: () = msg_send![control, setSelectedSegment: selected];
    }
}

/// Sets the border shape of the segmented control (macOS 26+, `NSControlBorderShape`).
/// 0 = Automatic, 1 = Capsule, 2 = RoundedRectangle, 3 = Circle.
pub(crate) unsafe fn set_native_segmented_border_shape(control: id, shape: i64) {
    unsafe {
        let _: () = msg_send![control, setBorderShape: shape];
        let _: () = msg_send![control, setNeedsDisplay: true];
    }
}

/// Sets the control size of the segmented control (`NSControlSize`).
/// 0 = Regular, 1 = Small, 2 = Mini, 3 = Large, 4 = ExtraLarge.
pub(crate) unsafe fn set_native_segmented_control_size(control: id, size: u64) {
    unsafe {
        let _: () = msg_send![control, setControlSize: size];
        let _: () = msg_send![control, sizeToFit];
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
