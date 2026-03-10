use super::{id, ns_string};
use objc::{class, msg_send, sel, sel_impl};

/// Creates a UISearchBar (iOS equivalent of NSSearchField).
pub(crate) unsafe fn create_native_search_field(placeholder: &str) -> id {
    unsafe {
        let bar: id = msg_send![class!(UISearchBar), alloc];
        let bar: id = msg_send![bar, init];
        let _: () = msg_send![bar, setPlaceholder: ns_string(placeholder)];
        bar
    }
}

/// Sets the search text.
pub(crate) unsafe fn set_native_search_field_string_value(field: id, value: &str) {
    unsafe {
        let _: () = msg_send![field, setText: ns_string(value)];
    }
}

/// Sets the placeholder text.
pub(crate) unsafe fn set_native_search_field_placeholder(field: id, placeholder: &str) {
    unsafe {
        let _: () = msg_send![field, setPlaceholder: ns_string(placeholder)];
    }
}

/// Sets an identifier (accessibility identifier on iOS).
pub(crate) unsafe fn set_native_search_field_identifier(field: id, identifier: &str) {
    unsafe {
        let _: () = msg_send![field, setAccessibilityIdentifier: ns_string(identifier)];
    }
}

/// No-op on iOS — UISearchBar always sends immediately.
pub(crate) unsafe fn set_native_search_field_sends_immediately(_field: id, _sends: bool) {}

/// No-op on iOS.
pub(crate) unsafe fn set_native_search_field_sends_whole_string(_field: id, _sends: bool) {}

/// Releases a UISearchBar.
pub(crate) unsafe fn release_native_search_field(field: id) {
    unsafe {
        if !field.is_null() {
            let _: () = msg_send![field, release];
        }
    }
}
