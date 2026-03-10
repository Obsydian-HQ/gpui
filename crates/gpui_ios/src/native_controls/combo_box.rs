// iOS has no native combo box (NSComboBox equivalent).
// We implement it as a UITextField — the dropdown aspect is not provided.
// A full implementation would use UITextField + UIPickerView as inputView.

use super::{id, ns_string};
use objc::{class, msg_send, sel, sel_impl};
use std::ffi::c_void;

/// Creates a UITextField styled as a combo box.
pub(crate) unsafe fn create_native_combo_box(
    items: &[&str],
    selected_index: Option<usize>,
    editable: bool,
) -> id {
    unsafe {
        let field: id = msg_send![class!(UITextField), alloc];
        let field: id = msg_send![field, init];
        let _: () = msg_send![field, setBorderStyle: 3i64]; // UITextBorderStyleRoundedRect

        // Set initial value from selected index
        if let Some(idx) = selected_index {
            if let Some(text) = items.get(idx) {
                let _: () = msg_send![field, setText: ns_string(text)];
            }
        }

        if !editable {
            // Disable direct text editing — user must pick from list
            let _: () = msg_send![field, setUserInteractionEnabled: false as i8];
        }

        // UIAccessibility
        let _: () = msg_send![field, setAccessibilityLabel: ns_string("Combo Box")];
        let _: () = msg_send![field, setAccessibilityHint: ns_string("Double tap to edit")];

        field
    }
}

/// Sets the combo box items. On iOS this is stored but not visually displayed
/// as a dropdown. A full implementation would use UIPickerView.
pub(crate) unsafe fn set_native_combo_box_items(_combo: id, _items: &[&str]) {
    // No-op — items would be set on a UIPickerView inputView
}

/// Sets the selected index by updating the text field value.
pub(crate) unsafe fn set_native_combo_box_selected(_combo: id, _index: usize) {
    // Would need access to items array to set text. No-op for now.
}

/// Sets the string value.
pub(crate) unsafe fn set_native_combo_box_string_value(combo: id, value: &str) {
    unsafe {
        let _: () = msg_send![combo, setText: ns_string(value)];
    }
}

/// Gets the string value.
pub(crate) unsafe fn get_native_combo_box_string_value(combo: id) -> String {
    unsafe {
        let text: id = msg_send![combo, text];
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

/// Sets whether the combo box is editable.
pub(crate) unsafe fn set_native_combo_box_editable(combo: id, editable: bool) {
    unsafe {
        let _: () = msg_send![combo, setUserInteractionEnabled: editable as i8];
    }
}

/// No-op on iOS.
pub(crate) unsafe fn set_native_combo_box_completes(_combo: id, _completes: bool) {}

/// Sets the delegate (target/action for text changes).
pub(crate) unsafe fn set_native_combo_box_delegate(
    combo: id,
    callbacks: super::text_field::TextFieldCallbacks,
) -> *mut c_void { unsafe {
    super::text_field::set_native_text_field_delegate(combo, callbacks)
}}

/// Releases the combo box delegate.
pub(crate) unsafe fn release_native_combo_box_delegate(delegate_ptr: *mut c_void) { unsafe {
    super::text_field::release_native_text_field_delegate(delegate_ptr)
}}

/// Releases a combo box (UITextField).
pub(crate) unsafe fn release_native_combo_box(combo: id) {
    unsafe {
        if !combo.is_null() {
            let _: () = msg_send![combo, release];
        }
    }
}
