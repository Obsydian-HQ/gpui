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
        let mut decl = ClassDecl::new("GPUINativeTextFieldDelegate", class!(NSObject)).unwrap();
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
        use super::super::ns_string;
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
        use super::super::ns_string;
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
        use super::super::ns_string;
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
        use super::super::ns_string;
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
