use super::ns_string;
use crate::{Bounds, Pixels};
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

const CALLBACK_IVAR: &str = "callbackPtr";

// =============================================================================
// Button target (fires a simple Fn() callback)
// =============================================================================

static mut BUTTON_TARGET_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_button_target_class() {
    unsafe {
        let mut decl =
            ClassDecl::new("GPUINativeButtonTarget", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);

        decl.add_method(
            sel!(buttonAction:),
            button_action as extern "C" fn(&Object, Sel, id),
        );

        BUTTON_TARGET_CLASS = decl.register();
    }
}

extern "C" fn button_action(this: &Object, _sel: Sel, _sender: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if !ptr.is_null() {
            let callback = &*(ptr as *const Box<dyn Fn()>);
            callback();
        }
    }
}

// =============================================================================
// Segmented-control target (fires Fn(usize) with the selected segment index)
// =============================================================================

static mut SEGMENTED_TARGET_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_segmented_target_class() {
    unsafe {
        let mut decl =
            ClassDecl::new("GPUINativeSegmentedTarget", class!(NSObject)).unwrap();
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
// NSButton — creation & lifecycle
// =============================================================================

/// Creates a new NSButton with the given title. The button is not yet added to any view.
pub(crate) unsafe fn create_native_button(title: &str) -> id {
    unsafe {
        let button: id = msg_send![class!(NSButton), alloc];
        let button: id = msg_send![button, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(100.0, 24.0),
        )];
        let _: () = msg_send![button, setTitle: ns_string(title)];
        // NSBezelStyleRounded = 1
        let _: () = msg_send![button, setBezelStyle: 1i64];
        let _: () = msg_send![button, setAutoresizingMask: 0u64];
        button
    }
}

/// Adds a native view as a subview of the given parent view.
pub(crate) unsafe fn attach_native_view_to_parent(view: id, parent: id) {
    unsafe {
        let _: () = msg_send![parent, addSubview: view];
    }
}

/// Alias for backwards compat.
pub(crate) unsafe fn attach_native_button_to_view(button: id, parent: id) {
    unsafe { attach_native_view_to_parent(button, parent) };
}

/// Positions any NSView within its parent, converting from GPUI's top-down coordinate
/// system to NSView's bottom-up coordinate system.
pub(crate) unsafe fn set_native_view_frame(
    view: id,
    bounds: Bounds<Pixels>,
    parent_view: id,
    _scale_factor: f32,
) {
    unsafe {
        let parent_frame: NSRect = msg_send![parent_view, frame];
        let parent_height = parent_frame.size.height;

        let x = bounds.origin.x.0 as f64;
        let y = bounds.origin.y.0 as f64;
        let w = bounds.size.width.0 as f64;
        let h = bounds.size.height.0 as f64;

        // NSView y-axis is bottom-up, GPUI is top-down
        let flipped_y = parent_height - y - h;

        let frame = NSRect::new(NSPoint::new(x, flipped_y), NSSize::new(w, h));
        let _: () = msg_send![view, setFrame: frame];
    }
}

/// Alias for backwards compat.
pub(crate) unsafe fn set_native_button_frame(
    button: id,
    bounds: Bounds<Pixels>,
    parent_view: id,
    scale_factor: f32,
) {
    unsafe { set_native_view_frame(button, bounds, parent_view, scale_factor) };
}

/// Updates the button's title.
pub(crate) unsafe fn set_native_button_title(button: id, title: &str) {
    unsafe {
        let _: () = msg_send![button, setTitle: ns_string(title)];
    }
}

/// Sets the button's target/action to invoke a Rust callback.
/// Returns a pointer to the target object (must be retained for the callback to work).
pub(crate) unsafe fn set_native_button_action(
    button: id,
    callback: Box<dyn Fn()>,
) -> *mut c_void {
    unsafe {
        let target: id = msg_send![BUTTON_TARGET_CLASS, alloc];
        let target: id = msg_send![target, init];

        let callback_ptr = Box::into_raw(Box::new(callback)) as *mut c_void;
        (*target).set_ivar::<*mut c_void>(CALLBACK_IVAR, callback_ptr);

        let _: () = msg_send![button, setTarget: target];
        let _: () = msg_send![button, setAction: sel!(buttonAction:)];

        target as *mut c_void
    }
}

/// Removes a native view from its parent.
pub(crate) unsafe fn remove_native_view_from_parent(view: id) {
    unsafe {
        let _: () = msg_send![view, removeFromSuperview];
    }
}

/// Alias for backwards compat.
pub(crate) unsafe fn remove_native_button_from_view(button: id) {
    unsafe { remove_native_view_from_parent(button) };
}

/// Releases the target object and frees the stored `Box<dyn Fn()>` callback.
pub(crate) unsafe fn release_native_button_target(target: *mut c_void) {
    unsafe {
        if !target.is_null() {
            let target = target as id;
            let callback_ptr: *mut c_void = *(*target).get_ivar(CALLBACK_IVAR);
            if !callback_ptr.is_null() {
                let _ = Box::from_raw(callback_ptr as *mut Box<dyn Fn()>);
            }
            let _: () = msg_send![target, release];
        }
    }
}

/// Releases an NSButton.
pub(crate) unsafe fn release_native_button(button: id) {
    unsafe {
        let _: () = msg_send![button, release];
    }
}

// =============================================================================
// NSButton — styling
// =============================================================================

/// Sets the bezel style of an NSButton.
/// Common values: 1 = Rounded, 6 = SmallSquare, 12 = Toolbar, 14 = Push, 15 = Inline.
pub(crate) unsafe fn set_native_button_bezel_style(button: id, bezel_style: i64) {
    unsafe {
        let _: () = msg_send![button, setBezelStyle: bezel_style];
    }
}

/// Sets whether the button draws a border.
pub(crate) unsafe fn set_native_button_bordered(button: id, bordered: bool) {
    unsafe {
        let _: () = msg_send![button, setBordered: bordered as i8];
    }
}

/// Sets the bezel color of the button (macOS 10.12.2+).
pub(crate) unsafe fn set_native_button_bezel_color(
    button: id,
    r: f64,
    g: f64,
    b: f64,
    a: f64,
) {
    unsafe {
        let color: id = msg_send![class!(NSColor), colorWithSRGBRed:r green:g blue:b alpha:a];
        let _: () = msg_send![button, setBezelColor: color];
    }
}

/// Sets whether the button shows its border only while the mouse is inside (hover effect).
pub(crate) unsafe fn set_native_button_shows_border_on_hover(button: id, shows: bool) {
    unsafe {
        let _: () = msg_send![button, setShowsBorderOnlyWhileMouseInside: shows as i8];
    }
}

/// Sets the button's bezel color to the system accent color (macOS 10.14+).
pub(crate) unsafe fn set_native_button_bezel_color_accent(button: id) {
    unsafe {
        let color: id = msg_send![class!(NSColor), controlAccentColor];
        let _: () = msg_send![button, setBezelColor: color];
    }
}

/// Sets the content tint color for text and images (macOS 10.14+).
pub(crate) unsafe fn set_native_button_content_tint_color(
    button: id,
    r: f64,
    g: f64,
    b: f64,
    a: f64,
) {
    unsafe {
        let color: id = msg_send![class!(NSColor), colorWithSRGBRed:r green:g blue:b alpha:a];
        let _: () = msg_send![button, setContentTintColor: color];
    }
}

// =============================================================================
// NSButton — SF Symbol icons
// =============================================================================

/// Sets an SF Symbol image on the button (macOS 11+).
/// Pass `image_only = true` to hide the title and show only the icon.
pub(crate) unsafe fn set_native_button_sf_symbol(
    button: id,
    symbol_name: &str,
    image_only: bool,
) {
    unsafe {
        let image: id = msg_send![
            class!(NSImage),
            imageWithSystemSymbolName: ns_string(symbol_name)
            accessibilityDescription: nil
        ];
        if image != nil {
            let _: () = msg_send![button, setImage: image];
            if image_only {
                // NSImageOnly = 1
                let _: () = msg_send![button, setImagePosition: 1i64];
            } else {
                // NSImageLeading = 7
                let _: () = msg_send![button, setImagePosition: 7i64];
            }
        }
    }
}

// =============================================================================
// NSSegmentedControl — creation & lifecycle
// =============================================================================

/// Creates a new NSSegmentedControl with the given labels.
pub(crate) unsafe fn create_native_segmented_control(
    labels: &[&str],
    selected_index: usize,
) -> id {
    unsafe {
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

/// Sets an SF Symbol image on a specific segment (macOS 11+).
pub(crate) unsafe fn set_native_segmented_image(
    control: id,
    segment: usize,
    symbol_name: &str,
) {
    unsafe {
        let image: id = msg_send![
            class!(NSImage),
            imageWithSystemSymbolName: ns_string(symbol_name)
            accessibilityDescription: nil
        ];
        if image != nil {
            let _: () = msg_send![control, setImage: image forSegment: segment as i64];
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

// =============================================================================
// Generic helpers
// =============================================================================

/// Sets the enabled state of an NSControl (button, segmented control, etc.).
pub(crate) unsafe fn set_native_control_enabled(control: id, enabled: bool) {
    unsafe {
        let _: () = msg_send![control, setEnabled: enabled as i8];
    }
}

/// Sets the tooltip on any NSView.
pub(crate) unsafe fn set_native_view_tooltip(view: id, tooltip: &str) {
    unsafe {
        let _: () = msg_send![view, setToolTip: ns_string(tooltip)];
    }
}
