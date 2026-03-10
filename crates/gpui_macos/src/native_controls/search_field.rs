use cocoa::{
    base::id,
    foundation::{NSPoint, NSRect, NSSize},
};
use objc::{class, msg_send, sel, sel_impl};

/// Creates a new NSSearchField with placeholder text.
pub(crate) unsafe fn create_native_search_field(placeholder: &str) -> id {
    unsafe {
        use super::super::ns_string;
        let field: id = msg_send![class!(NSSearchField), alloc];
        let field: id = msg_send![field, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(220.0, 24.0),
        )];
        let _: () = msg_send![field, setPlaceholderString: ns_string(placeholder)];
        let _: () = msg_send![field, setEditable: 1i8];
        let _: () = msg_send![field, setSelectable: 1i8];
        let _: () = msg_send![field, setBezeled: 1i8];
        let _: () = msg_send![field, setDrawsBackground: 1i8];
        let _: () = msg_send![field, setSendsSearchStringImmediately: 1i8];
        let _: () = msg_send![field, setSendsWholeSearchString: 0i8];
        let _: () = msg_send![field, setAutoresizingMask: 0u64];
        field
    }
}

/// Sets the current search text.
pub(crate) unsafe fn set_native_search_field_string_value(field: id, value: &str) {
    unsafe {
        use super::super::ns_string;
        let _: () = msg_send![field, setStringValue: ns_string(value)];
    }
}

/// Sets placeholder text.
pub(crate) unsafe fn set_native_search_field_placeholder(field: id, placeholder: &str) {
    unsafe {
        use super::super::ns_string;
        let _: () = msg_send![field, setPlaceholderString: ns_string(placeholder)];
    }
}

/// Sets a stable identifier used for programmatic focus lookups.
pub(crate) unsafe fn set_native_search_field_identifier(field: id, identifier: &str) {
    unsafe {
        use super::super::ns_string;
        let _: () = msg_send![field, setIdentifier: ns_string(identifier)];
    }
}

/// Controls whether the field sends each partial query as the user types.
pub(crate) unsafe fn set_native_search_field_sends_immediately(field: id, sends: bool) {
    unsafe {
        let _: () = msg_send![field, setSendsSearchStringImmediately: sends as i8];
    }
}

/// Controls whether the field only sends the complete search string.
pub(crate) unsafe fn set_native_search_field_sends_whole_string(field: id, sends: bool) {
    unsafe {
        let _: () = msg_send![field, setSendsWholeSearchString: sends as i8];
    }
}

/// Releases an NSSearchField.
pub(crate) unsafe fn release_native_search_field(field: id) {
    unsafe {
        let _: () = msg_send![field, release];
    }
}
