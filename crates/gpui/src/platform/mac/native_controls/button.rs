use super::CALLBACK_IVAR;
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

// =============================================================================
// Button target (fires a simple Fn() callback)
// =============================================================================

static mut BUTTON_TARGET_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_button_target_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUINativeButtonTarget", class!(NSObject)).unwrap();
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
// NSButton — creation & lifecycle
// =============================================================================

/// Creates a new NSButton with the given title. The button is not yet added to any view.
pub(crate) unsafe fn create_native_button(title: &str) -> id {
    unsafe {
        use super::super::ns_string;
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

/// Updates the button's title.
pub(crate) unsafe fn set_native_button_title(button: id, title: &str) {
    unsafe {
        use super::super::ns_string;
        let _: () = msg_send![button, setTitle: ns_string(title)];
    }
}

/// Sets the button's target/action to invoke a Rust callback.
/// Returns a pointer to the target object (must be retained for the callback to work).
pub(crate) unsafe fn set_native_button_action(button: id, callback: Box<dyn Fn()>) -> *mut c_void {
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
pub(crate) unsafe fn set_native_button_bezel_color(button: id, r: f64, g: f64, b: f64, a: f64) {
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
pub(crate) unsafe fn set_native_button_sf_symbol(button: id, symbol_name: &str, image_only: bool) {
    unsafe {
        use super::super::ns_string;
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
