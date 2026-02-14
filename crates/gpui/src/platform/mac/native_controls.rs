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

// =============================================================================
// NSTextField delegate (fires callbacks for text changes, editing, and submit)
// =============================================================================

/// Callbacks stored in the text field delegate's ivar.
pub(crate) struct TextFieldCallbacks {
    pub on_change: Option<Box<dyn Fn(String)>>,
    pub on_begin_editing: Option<Box<dyn Fn()>>,
    pub on_end_editing: Option<Box<dyn Fn(String)>>,
    pub on_submit: Option<Box<dyn Fn(String)>>,
}

static mut TEXT_FIELD_DELEGATE_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_text_field_delegate_class() {
    unsafe {
        let mut decl =
            ClassDecl::new("GPUINativeTextFieldDelegate", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);

        decl.add_method(
            sel!(controlTextDidChange:),
            text_did_change as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(controlTextDidBeginEditing:),
            text_did_begin_editing as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(controlTextDidEndEditing:),
            text_did_end_editing as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(control:textView:doCommandBySelector:),
            text_do_command as extern "C" fn(&Object, Sel, id, id, Sel) -> i8,
        );

        TEXT_FIELD_DELEGATE_CLASS = decl.register();
    }
}

/// Helper: extract a Rust String from an NSString pointer.
unsafe fn string_from_ns_string(ns_string: id) -> String {
    unsafe {
        let cstr: *const std::os::raw::c_char = msg_send![ns_string, UTF8String];
        if cstr.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(cstr)
                .to_string_lossy()
                .into_owned()
        }
    }
}

/// Helper: get the string value from an NSTextField extracted from an NSNotification.
unsafe fn string_value_from_notification(notification: id) -> String {
    unsafe {
        let field: id = msg_send![notification, object];
        let ns_str: id = msg_send![field, stringValue];
        string_from_ns_string(ns_str)
    }
}

extern "C" fn text_did_change(this: &Object, _sel: Sel, notification: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if !ptr.is_null() {
            let callbacks = &*(ptr as *const TextFieldCallbacks);
            if let Some(ref on_change) = callbacks.on_change {
                let text = string_value_from_notification(notification);
                on_change(text);
            }
        }
    }
}

extern "C" fn text_did_begin_editing(this: &Object, _sel: Sel, _notification: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if !ptr.is_null() {
            let callbacks = &*(ptr as *const TextFieldCallbacks);
            if let Some(ref on_begin) = callbacks.on_begin_editing {
                on_begin();
            }
        }
    }
}

extern "C" fn text_did_end_editing(this: &Object, _sel: Sel, notification: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if !ptr.is_null() {
            let callbacks = &*(ptr as *const TextFieldCallbacks);
            if let Some(ref on_end) = callbacks.on_end_editing {
                let text = string_value_from_notification(notification);
                on_end(text);
            }
        }
    }
}

extern "C" fn text_do_command(
    this: &Object,
    _sel: Sel,
    control: id,
    _text_view: id,
    command_selector: Sel,
) -> i8 {
    unsafe {
        // Check if the command is insertNewline: (Enter key)
        if command_selector == sel!(insertNewline:) {
            let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
            if !ptr.is_null() {
                let callbacks = &*(ptr as *const TextFieldCallbacks);
                if let Some(ref on_submit) = callbacks.on_submit {
                    let ns_str: id = msg_send![control, stringValue];
                    let text = string_from_ns_string(ns_str);
                    on_submit(text);
                    return 1; // YES — we handled it
                }
            }
        }
        0 // NO — let the system handle it
    }
}

// =============================================================================
// NSTextField — creation & lifecycle
// =============================================================================

/// Creates a new NSTextField with the given placeholder text.
pub(crate) unsafe fn create_native_text_field(placeholder: &str) -> id {
    unsafe {
        let field: id = msg_send![class!(NSTextField), alloc];
        let field: id = msg_send![field, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(200.0, 22.0),
        )];
        let _: () = msg_send![field, setPlaceholderString: ns_string(placeholder)];
        let _: () = msg_send![field, setEditable: 1i8];
        let _: () = msg_send![field, setSelectable: 1i8];
        let _: () = msg_send![field, setBezeled: 1i8];
        // NSTextFieldSquareBezel = 0
        let _: () = msg_send![field, setBezelStyle: 0i64];
        let _: () = msg_send![field, setDrawsBackground: 1i8];
        let _: () = msg_send![field, setAutoresizingMask: 0u64];
        field
    }
}

/// Creates a new NSSecureTextField with the given placeholder text.
pub(crate) unsafe fn create_native_secure_text_field(placeholder: &str) -> id {
    unsafe {
        let field: id = msg_send![class!(NSSecureTextField), alloc];
        let field: id = msg_send![field, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(200.0, 22.0),
        )];
        let _: () = msg_send![field, setPlaceholderString: ns_string(placeholder)];
        let _: () = msg_send![field, setEditable: 1i8];
        let _: () = msg_send![field, setSelectable: 1i8];
        let _: () = msg_send![field, setBezeled: 1i8];
        let _: () = msg_send![field, setBezelStyle: 0i64];
        let _: () = msg_send![field, setDrawsBackground: 1i8];
        let _: () = msg_send![field, setAutoresizingMask: 0u64];
        field
    }
}

/// Sets the string value of an NSTextField.
pub(crate) unsafe fn set_native_text_field_string_value(field: id, value: &str) {
    unsafe {
        let _: () = msg_send![field, setStringValue: ns_string(value)];
    }
}

/// Gets the string value of an NSTextField.
pub(crate) unsafe fn get_native_text_field_string_value(field: id) -> String {
    unsafe {
        let ns_str: id = msg_send![field, stringValue];
        string_from_ns_string(ns_str)
    }
}

/// Sets the placeholder string of an NSTextField.
pub(crate) unsafe fn set_native_text_field_placeholder(field: id, placeholder: &str) {
    unsafe {
        let _: () = msg_send![field, setPlaceholderString: ns_string(placeholder)];
    }
}

/// Sets the font size of an NSTextField.
pub(crate) unsafe fn set_native_text_field_font_size(field: id, size: f64) {
    unsafe {
        let font: id = msg_send![class!(NSFont), systemFontOfSize: size];
        let _: () = msg_send![field, setFont: font];
    }
}

/// Sets the text alignment of an NSTextField.
/// 0 = Left, 1 = Right, 2 = Center, 3 = Justified, 4 = Natural.
pub(crate) unsafe fn set_native_text_field_alignment(field: id, alignment: u64) {
    unsafe {
        let _: () = msg_send![field, setAlignment: alignment];
    }
}

/// Sets the bezel style of an NSTextField.
/// 0 = Square, 1 = Rounded.
pub(crate) unsafe fn set_native_text_field_bezel_style(field: id, style: i64) {
    unsafe {
        let _: () = msg_send![field, setBezelStyle: style];
    }
}

/// Sets the delegate for an NSTextField and stores the callbacks.
/// Returns a pointer to the delegate object (must be retained).
pub(crate) unsafe fn set_native_text_field_delegate(
    field: id,
    callbacks: TextFieldCallbacks,
) -> *mut c_void {
    unsafe {
        let delegate: id = msg_send![TEXT_FIELD_DELEGATE_CLASS, alloc];
        let delegate: id = msg_send![delegate, init];

        let callbacks_ptr = Box::into_raw(Box::new(callbacks)) as *mut c_void;
        (*delegate).set_ivar::<*mut c_void>(CALLBACK_IVAR, callbacks_ptr);

        let _: () = msg_send![field, setDelegate: delegate];

        delegate as *mut c_void
    }
}

/// Releases the text field delegate and frees the stored callbacks.
pub(crate) unsafe fn release_native_text_field_delegate(delegate_ptr: *mut c_void) {
    unsafe {
        if !delegate_ptr.is_null() {
            let delegate = delegate_ptr as id;
            let callbacks_ptr: *mut c_void = *(*delegate).get_ivar(CALLBACK_IVAR);
            if !callbacks_ptr.is_null() {
                let _ = Box::from_raw(callbacks_ptr as *mut TextFieldCallbacks);
            }
            let _: () = msg_send![delegate, release];
        }
    }
}

/// Releases an NSTextField.
pub(crate) unsafe fn release_native_text_field(field: id) {
    unsafe {
        let _: () = msg_send![field, release];
    }
}
