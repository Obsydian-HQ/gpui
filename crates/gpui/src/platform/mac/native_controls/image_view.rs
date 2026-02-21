use cocoa::{
    base::{id, nil},
    foundation::{NSPoint, NSRect, NSSize},
};
use objc::{class, msg_send, sel, sel_impl};

pub(crate) unsafe fn create_native_image_view() -> id {
    unsafe {
        let view: id = msg_send![class!(NSImageView), alloc];
        let view: id = msg_send![view, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(24.0, 24.0),
        )];
        let _: () = msg_send![view, setAutoresizingMask: 0u64];
        // NSImageAlignCenter = 0
        let _: () = msg_send![view, setImageAlignment: 0u64];
        // NSImageFrameNone = 0
        let _: () = msg_send![view, setImageFrameStyle: 0u64];
        view
    }
}

pub(crate) unsafe fn set_native_image_view_sf_symbol(view: id, symbol_name: &str) {
    unsafe {
        use super::super::ns_string;
        let name = ns_string(symbol_name);
        let image: id = msg_send![class!(NSImage), imageWithSystemSymbolName: name accessibilityDescription: nil];
        if image != nil {
            let _: () = msg_send![view, setImage: image];
        }
    }
}

pub(crate) unsafe fn set_native_image_view_sf_symbol_config(
    view: id,
    symbol_name: &str,
    point_size: f64,
    weight: i64,
) {
    unsafe {
        use super::super::ns_string;
        let name = ns_string(symbol_name);
        let image: id = msg_send![class!(NSImage), imageWithSystemSymbolName: name accessibilityDescription: nil];
        if image != nil {
            let config: id = msg_send![
                class!(NSImageSymbolConfiguration),
                configurationWithPointSize: point_size
                weight: weight
            ];
            if config != nil {
                let configured: id =
                    msg_send![image, imageWithSymbolConfiguration: config];
                if configured != nil {
                    let _: () = msg_send![view, setImage: configured];
                } else {
                    let _: () = msg_send![view, setImage: image];
                }
            } else {
                let _: () = msg_send![view, setImage: image];
            }
        }
    }
}

pub(crate) unsafe fn set_native_image_view_image_from_data(view: id, data: &[u8]) {
    unsafe {
        let ns_data: id = msg_send![class!(NSData), dataWithBytes: data.as_ptr() length: data.len()];
        if ns_data != nil {
            let image: id = msg_send![class!(NSImage), alloc];
            let image: id = msg_send![image, initWithData: ns_data];
            if image != nil {
                let _: () = msg_send![view, setImage: image];
                let _: () = msg_send![image, release];
            }
        }
    }
}

pub(crate) unsafe fn set_native_image_view_scaling(view: id, scaling: i64) {
    unsafe {
        let _: () = msg_send![view, setImageScaling: scaling];
    }
}

pub(crate) unsafe fn set_native_image_view_content_tint_color(
    view: id,
    r: f64,
    g: f64,
    b: f64,
    a: f64,
) {
    unsafe {
        let color: id = msg_send![
            class!(NSColor),
            colorWithSRGBRed: r green: g blue: b alpha: a
        ];
        if color != nil {
            let _: () = msg_send![view, setContentTintColor: color];
        }
    }
}

pub(crate) unsafe fn set_native_image_view_enabled(view: id, enabled: bool) {
    unsafe {
        let _: () = msg_send![view, setEnabled: enabled as i8];
    }
}

pub(crate) unsafe fn release_native_image_view(view: id) {
    unsafe {
        if view != nil {
            let _: () = msg_send![view, removeFromSuperview];
            let _: () = msg_send![view, release];
        }
    }
}
