use cocoa::{
    base::{id, nil},
    foundation::{NSPoint, NSRect, NSSize},
};
use objc::{class, msg_send, sel, sel_impl};

pub(crate) unsafe fn create_native_visual_effect_view() -> id {
    unsafe {
        let view: id = msg_send![class!(NSVisualEffectView), alloc];
        let view: id = msg_send![view, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(100.0, 100.0),
        )];
        let _: () = msg_send![view, setAutoresizingMask: 0u64];
        view
    }
}

pub(crate) unsafe fn set_native_visual_effect_material(view: id, material: i64) {
    unsafe {
        let _: () = msg_send![view, setMaterial: material];
    }
}

pub(crate) unsafe fn set_native_visual_effect_blending_mode(view: id, mode: i64) {
    unsafe {
        let _: () = msg_send![view, setBlendingMode: mode];
    }
}

pub(crate) unsafe fn set_native_visual_effect_state(view: id, state: i64) {
    unsafe {
        let _: () = msg_send![view, setState: state];
    }
}

pub(crate) unsafe fn set_native_visual_effect_emphasized(view: id, emphasized: bool) {
    unsafe {
        let _: () = msg_send![view, setEmphasized: emphasized as i8];
    }
}

pub(crate) unsafe fn set_native_visual_effect_corner_radius(view: id, radius: f64) {
    unsafe {
        let _: () = msg_send![view, setWantsLayer: true as i8];
        let layer: id = msg_send![view, layer];
        if layer != nil {
            let _: () = msg_send![layer, setCornerRadius: radius];
            let _: () = msg_send![layer, setMasksToBounds: true as i8];
        }
    }
}

pub(crate) unsafe fn release_native_visual_effect_view(view: id) {
    unsafe {
        if view != nil {
            let _: () = msg_send![view, removeFromSuperview];
            let _: () = msg_send![view, release];
        }
    }
}
