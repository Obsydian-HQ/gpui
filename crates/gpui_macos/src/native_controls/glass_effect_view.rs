use cocoa::{
    base::{id, nil},
    foundation::{NSPoint, NSRect, NSSize},
};
use objc::{class, msg_send, runtime::Class, sel, sel_impl};

/// Returns `true` if `NSGlassEffectView` is available at runtime (macOS 26+).
pub(crate) fn is_glass_effect_available() -> bool {
    Class::get("NSGlassEffectView").is_some()
}

/// Creates an `NSGlassEffectView` instance.
/// Returns `nil` if the class is unavailable (pre-macOS 26).
pub(crate) unsafe fn create_native_glass_effect_view() -> id {
    unsafe {
        let cls = match Class::get("NSGlassEffectView") {
            Some(cls) => cls,
            None => return nil,
        };
        let view: id = msg_send![cls, alloc];
        let view: id = msg_send![view, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(100.0, 100.0),
        )];
        let _: () = msg_send![view, setAutoresizingMask: 0u64];
        view
    }
}

/// Sets the public `style` property.
/// Regular = 0, Clear = 1.
pub(crate) unsafe fn set_native_glass_effect_style(view: id, style: i64) {
    unsafe {
        let _: () = msg_send![view, setStyle: style];
    }
}

/// Sets the corner radius via the view's own property (not the layer).
pub(crate) unsafe fn set_native_glass_effect_corner_radius(view: id, radius: f64) {
    unsafe {
        let _: () = msg_send![view, setCornerRadius: radius];
    }
}

/// Sets the tint color using an NSColor.
pub(crate) unsafe fn set_native_glass_effect_tint_color(
    view: id,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) {
    unsafe {
        let color: id = msg_send![
            class!(NSColor),
            colorWithSRGBRed: red
            green: green
            blue: blue
            alpha: alpha
        ];
        let _: () = msg_send![view, setTintColor: color];
    }
}

/// Clears the tint color (sets it to nil).
pub(crate) unsafe fn clear_native_glass_effect_tint_color(view: id) {
    unsafe {
        let _: () = msg_send![view, setTintColor: nil];
    }
}

/// Releases an `NSGlassEffectView`, removing it from its superview first.
pub(crate) unsafe fn release_native_glass_effect_view(view: id) {
    unsafe {
        if view != nil {
            let _: () = msg_send![view, removeFromSuperview];
            let _: () = msg_send![view, release];
        }
    }
}
