use super::{id, ns_string, UI_CONTROL_EVENT_EDITING_CHANGED, UI_CONTROL_EVENT_EDITING_DID_BEGIN, UI_CONTROL_EVENT_EDITING_DID_END, UI_CONTROL_EVENT_EDITING_DID_END_ON_EXIT};
use ctor::ctor;
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Sel},
    sel, sel_impl,
};
use std::{ffi::c_void, ptr};

/// Callbacks struct for text field delegate events.
pub(crate) struct TextFieldCallbacks {
    pub on_change: Option<Box<dyn Fn(String)>>,
    pub on_begin_editing: Option<Box<dyn Fn()>>,
    pub on_end_editing: Option<Box<dyn Fn(String)>>,
    pub on_submit: Option<Box<dyn Fn(String)>>,
    pub on_move_up: Option<Box<dyn Fn()>>,
    pub on_move_down: Option<Box<dyn Fn()>>,
    pub on_cancel: Option<Box<dyn Fn()>>,
}

const CALLBACKS_IVAR: &str = "callbacksPtr";

static mut TEXT_FIELD_TARGET_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_text_field_target_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUIiOSNativeTextFieldTarget", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>(CALLBACKS_IVAR);

        decl.add_method(
            sel!(textChanged:),
            text_changed as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(editingDidBegin:),
            editing_did_begin as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(editingDidEnd:),
            editing_did_end as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(editingDidEndOnExit:),
            editing_did_end_on_exit as extern "C" fn(&Object, Sel, id),
        );

        TEXT_FIELD_TARGET_CLASS = decl.register();
    }
}

extern "C" fn text_changed(this: &Object, _sel: Sel, sender: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACKS_IVAR);
        if !ptr.is_null() {
            let callbacks = &*(ptr as *const TextFieldCallbacks);
            if let Some(ref on_change) = callbacks.on_change {
                let text: id = msg_send![sender, text];
                let utf8: *const std::os::raw::c_char = msg_send![text, UTF8String];
                let value = if utf8.is_null() {
                    String::new()
                } else {
                    std::ffi::CStr::from_ptr(utf8).to_string_lossy().into_owned()
                };
                on_change(value);
            }
        }
    }
}

extern "C" fn editing_did_begin(this: &Object, _sel: Sel, _sender: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACKS_IVAR);
        if !ptr.is_null() {
            let callbacks = &*(ptr as *const TextFieldCallbacks);
            if let Some(ref on_begin_editing) = callbacks.on_begin_editing {
                on_begin_editing();
            }
        }
    }
}

extern "C" fn editing_did_end(this: &Object, _sel: Sel, sender: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACKS_IVAR);
        if !ptr.is_null() {
            let callbacks = &*(ptr as *const TextFieldCallbacks);
            if let Some(ref on_end_editing) = callbacks.on_end_editing {
                let value = get_native_text_field_string_value(sender);
                on_end_editing(value);
            }
        }
    }
}

extern "C" fn editing_did_end_on_exit(this: &Object, _sel: Sel, sender: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACKS_IVAR);
        if !ptr.is_null() {
            let callbacks = &*(ptr as *const TextFieldCallbacks);
            if let Some(ref on_submit) = callbacks.on_submit {
                let value = get_native_text_field_string_value(sender);
                on_submit(value);
            }
        }
    }
}

// =============================================================================
// UITextField — creation & lifecycle
// =============================================================================

/// Creates a new UITextField with a placeholder.
pub(crate) unsafe fn create_native_text_field(placeholder: &str) -> id {
    unsafe {
        let field: id = msg_send![class!(UITextField), alloc];
        let field: id = msg_send![field, init];
        let _: () = msg_send![field, setPlaceholder: ns_string(placeholder)];
        // Default border style: rounded rect
        let _: () = msg_send![field, setBorderStyle: 3i64]; // UITextBorderStyleRoundedRect

        // UIAccessibility — use placeholder as hint so VoiceOver reads it
        let _: () = msg_send![field, setAccessibilityHint: ns_string(placeholder)];

        field
    }
}

/// Creates a secure (password) UITextField.
pub(crate) unsafe fn create_native_secure_text_field(placeholder: &str) -> id {
    unsafe {
        let field = create_native_text_field(placeholder);
        let _: () = msg_send![field, setSecureTextEntry: true as i8];
        field
    }
}

/// Sets the text value.
pub(crate) unsafe fn set_native_text_field_string_value(field: id, value: &str) {
    unsafe {
        let _: () = msg_send![field, setText: ns_string(value)];
    }
}

/// Gets the current text value.
pub(crate) unsafe fn get_native_text_field_string_value(field: id) -> String {
    unsafe {
        let text: id = msg_send![field, text];
        if text.is_null() {
            return String::new();
        }
        let utf8: *const std::os::raw::c_char = msg_send![text, UTF8String];
        if utf8.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(utf8).to_string_lossy().into_owned()
        }
    }
}

/// Sets the placeholder text.
pub(crate) unsafe fn set_native_text_field_placeholder(field: id, placeholder: &str) {
    unsafe {
        let _: () = msg_send![field, setPlaceholder: ns_string(placeholder)];
        let _: () = msg_send![field, setAccessibilityHint: ns_string(placeholder)];
    }
}

/// Sets the font size.
pub(crate) unsafe fn set_native_text_field_font_size(field: id, size: f64) {
    unsafe {
        let font: id = msg_send![class!(UIFont), systemFontOfSize: size];
        let _: () = msg_send![field, setFont: font];
    }
}

/// Sets text alignment. 0 = left, 1 = center, 2 = right.
pub(crate) unsafe fn set_native_text_field_alignment(field: id, alignment: u64) {
    unsafe {
        // NSTextAlignment values are the same on iOS
        let _: () = msg_send![field, setTextAlignment: alignment];
    }
}

/// Sets the border style. On iOS: 0 = none, 1 = line, 2 = bezel, 3 = rounded rect.
pub(crate) unsafe fn set_native_text_field_bezel_style(field: id, style: i64) {
    unsafe {
        let _: () = msg_send![field, setBorderStyle: style];
    }
}

/// Sets up the target/action callbacks for the text field.
pub(crate) unsafe fn set_native_text_field_delegate(
    field: id,
    callbacks: TextFieldCallbacks,
) -> *mut c_void {
    unsafe {
        let target: id = msg_send![TEXT_FIELD_TARGET_CLASS, alloc];
        let target: id = msg_send![target, init];

        let _ = (&callbacks.on_move_up, &callbacks.on_move_down, &callbacks.on_cancel);
        let callbacks_ptr = Box::into_raw(Box::new(callbacks)) as *mut c_void;
        (*target).set_ivar::<*mut c_void>(CALLBACKS_IVAR, callbacks_ptr);

        // Wire UIControl events to target methods
        let _: () = msg_send![field,
            addTarget: target
            action: sel!(textChanged:)
            forControlEvents: UI_CONTROL_EVENT_EDITING_CHANGED
        ];
        let _: () = msg_send![field,
            addTarget: target
            action: sel!(editingDidBegin:)
            forControlEvents: UI_CONTROL_EVENT_EDITING_DID_BEGIN
        ];
        let _: () = msg_send![field,
            addTarget: target
            action: sel!(editingDidEnd:)
            forControlEvents: UI_CONTROL_EVENT_EDITING_DID_END
        ];
        let _: () = msg_send![field,
            addTarget: target
            action: sel!(editingDidEndOnExit:)
            forControlEvents: UI_CONTROL_EVENT_EDITING_DID_END_ON_EXIT
        ];

        target as *mut c_void
    }
}

/// Releases the text field delegate/target and its callbacks.
pub(crate) unsafe fn release_native_text_field_delegate(delegate_ptr: *mut c_void) {
    unsafe {
        if !delegate_ptr.is_null() {
            let target = delegate_ptr as id;
            let callbacks_ptr: *mut c_void = *(*target).get_ivar(CALLBACKS_IVAR);
            if !callbacks_ptr.is_null() {
                let _ = Box::from_raw(callbacks_ptr as *mut TextFieldCallbacks);
            }
            let _: () = msg_send![target, release];
        }
    }
}

/// Releases a UITextField.
pub(crate) unsafe fn release_native_text_field(field: id) {
    unsafe {
        if !field.is_null() {
            let _: () = msg_send![field, release];
        }
    }
}
