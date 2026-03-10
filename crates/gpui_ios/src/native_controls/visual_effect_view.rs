use super::id;
use objc::{class, msg_send, sel, sel_impl};

/// Creates a UIVisualEffectView with a blur effect.
pub(crate) unsafe fn create_native_visual_effect_view() -> id {
    unsafe {
        // UIBlurEffectStyleSystemMaterial = 6
        let effect: id = msg_send![class!(UIBlurEffect), effectWithStyle: 6i64];
        let view: id = msg_send![class!(UIVisualEffectView), alloc];
        let view: id = msg_send![view, initWithEffect: effect];
        view
    }
}

/// Sets the blur effect material/style.
/// Maps macOS material values to iOS UIBlurEffect.Style:
/// 0 = extraLight, 1 = light, 2 = dark, 3-6 = system materials.
pub(crate) unsafe fn set_native_visual_effect_material(view: id, material: i64) {
    unsafe {
        let style = match material {
            0 => 0i64,  // UIBlurEffectStyleExtraLight
            1 => 1,     // UIBlurEffectStyleLight
            2 => 2,     // UIBlurEffectStyleDark
            3 => 6,     // UIBlurEffectStyleSystemMaterial
            4 => 7,     // UIBlurEffectStyleSystemThinMaterial
            5 => 8,     // UIBlurEffectStyleSystemUltraThinMaterial
            _ => 6,     // Default to system material
        };
        let effect: id = msg_send![class!(UIBlurEffect), effectWithStyle: style];
        let _: () = msg_send![view, setEffect: effect];
    }
}

/// No-op on iOS — blending mode is not separately configurable.
pub(crate) unsafe fn set_native_visual_effect_blending_mode(_view: id, _mode: i64) {}

/// No-op on iOS — state (active/inactive) is automatic.
pub(crate) unsafe fn set_native_visual_effect_state(_view: id, _state: i64) {}

/// No-op on iOS — emphasis is not separately configurable.
pub(crate) unsafe fn set_native_visual_effect_emphasized(_view: id, _emphasized: bool) {}

/// Sets the corner radius.
pub(crate) unsafe fn set_native_visual_effect_corner_radius(view: id, radius: f64) {
    unsafe {
        let layer: id = msg_send![view, layer];
        let _: () = msg_send![layer, setCornerRadius: radius];
        let _: () = msg_send![layer, setMasksToBounds: true as i8];
    }
}

/// Releases a UIVisualEffectView.
pub(crate) unsafe fn release_native_visual_effect_view(view: id) {
    unsafe {
        if !view.is_null() {
            let _: () = msg_send![view, release];
        }
    }
}
