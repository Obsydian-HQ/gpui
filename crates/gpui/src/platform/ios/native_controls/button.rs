use super::{id, nil, ns_string, CALLBACK_IVAR, UI_CONTROL_EVENT_TOUCH_UP_INSIDE};
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
// Button target (fires a simple Fn() callback)
// =============================================================================

static mut BUTTON_TARGET_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_button_target_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUIiOSNativeButtonTarget", class!(NSObject)).unwrap();
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

unsafe fn install_button_configuration(button: id, configuration: id) {
    if configuration == nil {
        return;
    }

    // Preserve any title/image already set through legacy UIButton setters.
    let title: id = msg_send![button, titleForState: 0u64];
    if title != nil {
        let _: () = msg_send![configuration, setTitle: title];
    }
    let image: id = msg_send![button, imageForState: 0u64];
    if image != nil {
        let _: () = msg_send![configuration, setImage: image];
    }

    let _: () = msg_send![button, setConfiguration: configuration];
}

unsafe fn get_button_configuration(button: id) -> id {
    if msg_send![button, respondsToSelector: sel!(configuration)] {
        msg_send![button, configuration]
    } else {
        nil
    }
}

// =============================================================================
// UIButton — creation & lifecycle
// =============================================================================

/// Creates a new UIButton with the given title.
pub(crate) unsafe fn create_native_button(title: &str) -> id {
    unsafe {
        // UIButton.buttonWithType: 0 = UIButtonTypeCustom, 1 = UIButtonTypeSystem
        let button: id = msg_send![class!(UIButton), buttonWithType: 1i64];
        let _: () = msg_send![button, retain];
        let _: () = msg_send![button, setTitle: ns_string(title) forState: 0u64]; // UIControlStateNormal

        // UIAccessibility — UIButton already has UIAccessibilityTraitButton by default
        let _: () = msg_send![button, setAccessibilityLabel: ns_string(title)];

        button
    }
}

/// Updates the button's title.
pub(crate) unsafe fn set_native_button_title(button: id, title: &str) {
    unsafe {
        let title = ns_string(title);
        let _: () = msg_send![button, setTitle: title forState: 0u64];

        let configuration = get_button_configuration(button);
        if configuration != nil {
            let _: () = msg_send![configuration, setTitle: title];
            let _: () = msg_send![button, setConfiguration: configuration];
        }

        let _: () = msg_send![button, setAccessibilityLabel: title];
    }
}

/// Sets the button's target/action to invoke a Rust callback.
pub(crate) unsafe fn set_native_button_action(button: id, callback: Box<dyn Fn()>) -> *mut c_void {
    unsafe {
        let target: id = msg_send![BUTTON_TARGET_CLASS, alloc];
        let target: id = msg_send![target, init];

        let callback_ptr = Box::into_raw(Box::new(callback)) as *mut c_void;
        (*target).set_ivar::<*mut c_void>(CALLBACK_IVAR, callback_ptr);

        let _: () = msg_send![button,
            addTarget: target
            action: sel!(buttonAction:)
            forControlEvents: UI_CONTROL_EVENT_TOUCH_UP_INSIDE
        ];

        target as *mut c_void
    }
}

/// Releases the target object and frees the stored callback.
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

/// Releases a UIButton.
pub(crate) unsafe fn release_native_button(button: id) {
    unsafe {
        if !button.is_null() {
            let _: () = msg_send![button, release];
        }
    }
}

// =============================================================================
// UIButton — styling
// =============================================================================

/// Sets the button's bezel style. On iOS this maps to UIButtonConfiguration.
/// 0 = Borderless/plain, 1 = Rounded/bordered, 12 = Filled, 15 = Inline.
pub(crate) unsafe fn set_native_button_bezel_style(button: id, bezel_style: i64) {
    unsafe {
        if msg_send![button, respondsToSelector: sel!(setConfiguration:)] {
            let configuration_class = class!(UIButtonConfiguration);
            let configuration: id = match bezel_style {
                0 => msg_send![configuration_class, plainButtonConfiguration],
                12 => msg_send![configuration_class, borderedProminentButtonConfiguration],
                15 => msg_send![configuration_class, borderlessButtonConfiguration],
                _ => msg_send![configuration_class, borderedButtonConfiguration],
            };
            install_button_configuration(button, configuration);
            return;
        }

        // Older iOS fallback: keep system button look.
        let _: () = msg_send![button, setBackgroundColor: nil];
        let layer: id = msg_send![button, layer];
        let _: () = msg_send![layer, setBorderWidth: 0.0f64];
        let _: () = msg_send![layer, setCornerRadius: 0.0f64];
    }
}

/// Sets whether the button draws a border.
pub(crate) unsafe fn set_native_button_bordered(button: id, bordered: bool) {
    unsafe {
        if msg_send![button, respondsToSelector: sel!(setConfiguration:)] {
            if bordered {
                return;
            }
            let configuration: id =
                msg_send![class!(UIButtonConfiguration), plainButtonConfiguration];
            install_button_configuration(button, configuration);
            return;
        }

        if bordered {
            return;
        }
        let layer: id = msg_send![button, layer];
        let _: () = msg_send![layer, setBorderWidth: 0.0f64];
    }
}

/// Sets the bezel/background color of the button.
pub(crate) unsafe fn set_native_button_bezel_color(button: id, r: f64, g: f64, b: f64, a: f64) {
    unsafe {
        let color: id = msg_send![class!(UIColor), colorWithRed: r green: g blue: b alpha: a];

        let configuration = get_button_configuration(button);
        if configuration != nil {
            let _: () = msg_send![configuration, setBaseBackgroundColor: color];
            let _: () = msg_send![button, setConfiguration: configuration];
        } else {
            let _: () = msg_send![button, setBackgroundColor: color];
        }
    }
}

/// No-op on iOS (no hover state for touch).
pub(crate) unsafe fn set_native_button_shows_border_on_hover(_button: id, _shows: bool) {}

/// Sets the button background to the system tint color.
pub(crate) unsafe fn set_native_button_bezel_color_accent(button: id) {
    unsafe {
        let color: id = msg_send![class!(UIColor), systemBlueColor];

        let configuration = get_button_configuration(button);
        if configuration != nil {
            let _: () = msg_send![configuration, setBaseBackgroundColor: color];
            let _: () = msg_send![button, setConfiguration: configuration];
        } else {
            let _: () = msg_send![button, setBackgroundColor: color];
        }
    }
}

/// Sets the content tint color for text and images.
pub(crate) unsafe fn set_native_button_content_tint_color(
    button: id,
    r: f64,
    g: f64,
    b: f64,
    a: f64,
) {
    unsafe {
        let color: id = msg_send![class!(UIColor), colorWithRed: r green: g blue: b alpha: a];

        let configuration = get_button_configuration(button);
        if configuration != nil {
            let _: () = msg_send![configuration, setBaseForegroundColor: color];
            let _: () = msg_send![button, setConfiguration: configuration];
        } else {
            let _: () = msg_send![button, setTintColor: color];
        }
    }
}

// =============================================================================
// UIButton — SF Symbol icons
// =============================================================================

/// Sets an SF Symbol image on the button.
pub(crate) unsafe fn set_native_button_sf_symbol(button: id, symbol_name: &str, image_only: bool) {
    unsafe {
        let image: id = msg_send![class!(UIImage), systemImageNamed: ns_string(symbol_name)];
        if image == nil {
            return;
        }

        let _: () = msg_send![button, setImage: image forState: 0u64];

        let configuration = get_button_configuration(button);
        if configuration != nil {
            let _: () = msg_send![configuration, setImage: image];
            if image_only {
                let empty = ns_string("");
                let _: () = msg_send![button, setTitle: empty forState: 0u64];
                let _: () = msg_send![configuration, setTitle: empty];
            }
            let _: () = msg_send![button, setConfiguration: configuration];
        } else if image_only {
            let _: () = msg_send![button, setTitle: ns_string("") forState: 0u64];
        }
    }
}
