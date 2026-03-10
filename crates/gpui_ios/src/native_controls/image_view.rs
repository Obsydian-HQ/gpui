use super::{id, nil, ns_string};
use objc::{class, msg_send, sel, sel_impl};

/// Creates a UIImageView.
pub(crate) unsafe fn create_native_image_view() -> id {
    unsafe {
        let view: id = msg_send![class!(UIImageView), alloc];
        let view: id = msg_send![view, init];
        // Default content mode: aspect fit
        let _: () = msg_send![view, setContentMode: 1i64]; // UIViewContentModeScaleAspectFit
        let _: () = msg_send![view, setClipsToBounds: true as i8];
        view
    }
}

/// Sets an SF Symbol image.
pub(crate) unsafe fn set_native_image_view_sf_symbol(view: id, symbol_name: &str) {
    unsafe {
        let image: id = msg_send![class!(UIImage), systemImageNamed: ns_string(symbol_name)];
        if image != nil {
            let _: () = msg_send![view, setImage: image];
        }
    }
}

/// Sets an SF Symbol with configuration (point size and weight).
pub(crate) unsafe fn set_native_image_view_sf_symbol_config(
    view: id,
    symbol_name: &str,
    point_size: f64,
    weight: i64,
) {
    unsafe {
        // UIImageSymbolWeight is an NSInteger enum.
        let ui_weight: i64 = match weight {
            1..=9 => weight,
            _ => 4, // UIImageSymbolWeightRegular
        };

        let config: id = msg_send![class!(UIImageSymbolConfiguration),
            configurationWithPointSize: point_size
            weight: ui_weight
        ];
        let image: id = msg_send![class!(UIImage), systemImageNamed: ns_string(symbol_name)];
        if image != nil {
            let configured: id = msg_send![image, imageWithConfiguration: config];
            let _: () = msg_send![view, setImage: configured];
        }
    }
}

/// Sets image from raw data bytes (PNG/JPEG).
pub(crate) unsafe fn set_native_image_view_image_from_data(view: id, data: &[u8]) {
    unsafe {
        let ns_data: id = msg_send![class!(NSData),
            dataWithBytes: data.as_ptr()
            length: data.len()
        ];
        let image: id = msg_send![class!(UIImage), imageWithData: ns_data];
        if image != nil {
            let _: () = msg_send![view, setImage: image];
        }
    }
}

/// Clears the image.
pub(crate) unsafe fn clear_native_image_view_image(view: id) {
    unsafe {
        let _: () = msg_send![view, setImage: nil];
    }
}

/// Sets the content mode (scaling).
/// 0 = scale to fill, 1 = aspect fit, 2 = aspect fill.
pub(crate) unsafe fn set_native_image_view_scaling(view: id, scaling: i64) {
    unsafe {
        let _: () = msg_send![view, setContentMode: scaling];
    }
}

/// Sets the tint color for template images.
pub(crate) unsafe fn set_native_image_view_content_tint_color(
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
        let _: () = msg_send![view, setTintColor: color];
    }
}

/// Sets the enabled state (alpha dimming).
pub(crate) unsafe fn set_native_image_view_enabled(view: id, enabled: bool) {
    unsafe {
        let alpha: f64 = if enabled { 1.0 } else { 0.4 };
        let _: () = msg_send![view, setAlpha: alpha];
    }
}

/// Releases a UIImageView.
pub(crate) unsafe fn release_native_image_view(view: id) {
    unsafe {
        if !view.is_null() {
            let _: () = msg_send![view, release];
        }
    }
}
