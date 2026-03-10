use super::id;
use objc::{class, msg_send, sel, sel_impl};

/// Creates a glass effect view using UIVisualEffectView with a prominent blur.
pub(crate) unsafe fn create_native_glass_effect_view() -> id {
    unsafe {
        // UIBlurEffectStyleSystemUltraThinMaterial = 8 — closest to macOS glass
        let effect: id = msg_send![class!(UIBlurEffect), effectWithStyle: 8i64];
        let view: id = msg_send![class!(UIVisualEffectView), alloc];
        let view: id = msg_send![view, initWithEffect: effect];
        view
    }
}

/// Sets the glass effect style. Maps to UIBlurEffect styles.
pub(crate) unsafe fn set_native_glass_effect_style(view: id, style: i64) {
    unsafe {
        let ios_style = match style {
            0 => 8i64,  // Ultra thin
            1 => 7,     // Thin
            2 => 6,     // Regular
            3 => 9,     // Thick
            _ => 8,     // Default ultra thin
        };
        let effect: id = msg_send![class!(UIBlurEffect), effectWithStyle: ios_style];
        let _: () = msg_send![view, setEffect: effect];
    }
}

/// Sets the corner radius.
pub(crate) unsafe fn set_native_glass_effect_corner_radius(view: id, radius: f64) {
    unsafe {
        let layer: id = msg_send![view, layer];
        let _: () = msg_send![layer, setCornerRadius: radius];
        let _: () = msg_send![layer, setMasksToBounds: true as i8];
    }
}

/// Sets a tint color overlay on the glass.
pub(crate) unsafe fn set_native_glass_effect_tint_color(
    view: id,
    r: f64,
    g: f64,
    b: f64,
    a: f64,
) {
    unsafe {
        let color: id = msg_send![class!(UIColor),
            colorWithRed: r green: g blue: b alpha: a
        ];
        // Apply tint via a background color with low alpha on the content view
        let content_view: id = msg_send![view, contentView];
        let _: () = msg_send![content_view, setBackgroundColor: color];
    }
}

/// Releases a glass effect view.
pub(crate) unsafe fn release_native_glass_effect_view(view: id) {
    unsafe {
        if !view.is_null() {
            let _: () = msg_send![view, release];
        }
    }
}
