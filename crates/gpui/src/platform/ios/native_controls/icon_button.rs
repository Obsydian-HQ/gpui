// Icon button on iOS uses the same UIButton infrastructure as button.rs.
// The SF Symbol API is identical on iOS: UIImage.systemImageNamed:.
// All functions are re-exported from button.rs — this module provides
// only the icon-specific helper that the element layer uses.

use super::{id, nil, ns_string};
use objc::{class, msg_send, sel, sel_impl};

/// Creates a UIButton displaying only an SF Symbol icon.
pub(crate) unsafe fn create_native_icon_button(symbol_name: &str) -> id {
    unsafe {
        let button: id = msg_send![class!(UIButton), buttonWithType: 1i64]; // UIButtonTypeSystem
        let _: () = msg_send![button, retain];

        let image: id = msg_send![class!(UIImage), systemImageNamed: ns_string(symbol_name)];
        if image != nil {
            let _: () = msg_send![button, setImage: image forState: 0u64];
        }
        // No title — icon only
        let _: () = msg_send![button, setTitle: ns_string("") forState: 0u64];

        // UIAccessibility — UIButton already has UIAccessibilityTraitButton by default
        let _: () = msg_send![button, setAccessibilityLabel: ns_string(symbol_name)];

        button
    }
}

/// Updates the SF Symbol on an icon button.
pub(crate) unsafe fn set_native_icon_button_symbol(button: id, symbol_name: &str) {
    unsafe {
        let image: id = msg_send![class!(UIImage), systemImageNamed: ns_string(symbol_name)];
        if image != nil {
            let _: () = msg_send![button, setImage: image forState: 0u64];
            if msg_send![button, respondsToSelector: sel!(configuration)] {
                let configuration: id = msg_send![button, configuration];
                if configuration != nil {
                    let _: () = msg_send![configuration, setImage: image];
                    let _: () = msg_send![button, setConfiguration: configuration];
                }
            }
        }

        // Update accessibility label to match new symbol
        let _: () = msg_send![button, setAccessibilityLabel: ns_string(symbol_name)];
    }
}

/// Releases an icon button — same as a regular UIButton.
pub(crate) unsafe fn release_native_icon_button(button: id) {
    unsafe {
        if !button.is_null() {
            let _: () = msg_send![button, release];
        }
    }
}
