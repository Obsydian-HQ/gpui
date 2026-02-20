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
    runtime::{Class, Object, Protocol, Sel},
    sel, sel_impl,
};
use std::{ffi::c_void, ptr};

struct PopoverCallbacks {
    on_close: Option<Box<dyn Fn()>>,
    on_show: Option<Box<dyn Fn()>>,
}

static mut POPOVER_DELEGATE_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_popover_delegate_class() {
    unsafe {
        let mut decl =
            ClassDecl::new("GPUINativePopoverDelegate", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);

        decl.add_method(
            sel!(popoverDidClose:),
            popover_did_close as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(popoverDidShow:),
            popover_did_show as extern "C" fn(&Object, Sel, id),
        );

        if let Some(protocol) = Protocol::get("NSPopoverDelegate") {
            decl.add_protocol(protocol);
        }

        POPOVER_DELEGATE_CLASS = decl.register();
    }
}

extern "C" fn popover_did_close(this: &Object, _sel: Sel, _notification: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if ptr.is_null() {
            return;
        }
        let callbacks = &*(ptr as *const PopoverCallbacks);
        if let Some(ref on_close) = callbacks.on_close {
            on_close();
        }
    }
}

extern "C" fn popover_did_show(this: &Object, _sel: Sel, _notification: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if ptr.is_null() {
            return;
        }
        let callbacks = &*(ptr as *const PopoverCallbacks);
        if let Some(ref on_show) = callbacks.on_show {
            on_show();
        }
    }
}

/// Creates an NSPopover with a content view of the given size.
///
/// Returns `(popover, delegate_ptr)`. The caller owns both and must eventually
/// call `release_native_popover` to clean up.
///
/// The content view can be retrieved with `get_native_popover_content_view` to
/// add subviews.
///
/// `behavior` maps to `NSPopoverBehavior`:
///   - 0 = applicationDefined
///   - 1 = transient (closes on click outside)
///   - 2 = semitransient
pub(crate) unsafe fn create_native_popover(
    width: f64,
    height: f64,
    behavior: i64,
    on_close: Option<Box<dyn Fn()>>,
    on_show: Option<Box<dyn Fn()>>,
) -> (id, *mut c_void) {
    unsafe {
        let content_view: id = msg_send![class!(NSView), alloc];
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height));
        let content_view: id = msg_send![content_view, initWithFrame: frame];

        let view_controller: id = msg_send![class!(NSViewController), alloc];
        let view_controller: id = msg_send![view_controller, init];
        let _: () = msg_send![view_controller, setView: content_view];
        let _: () = msg_send![content_view, release];

        let popover: id = msg_send![class!(NSPopover), alloc];
        let popover: id = msg_send![popover, init];
        let _: () = msg_send![popover, setContentViewController: view_controller];
        let _: () = msg_send![popover, setBehavior: behavior];
        let content_size = NSSize::new(width, height);
        let _: () = msg_send![popover, setContentSize: content_size];

        let _: () = msg_send![view_controller, release];

        let delegate: id = msg_send![POPOVER_DELEGATE_CLASS, alloc];
        let delegate: id = msg_send![delegate, init];

        let callbacks = PopoverCallbacks { on_close, on_show };
        let callbacks_ptr = Box::into_raw(Box::new(callbacks)) as *mut c_void;
        (*delegate).set_ivar::<*mut c_void>(CALLBACK_IVAR, callbacks_ptr);

        let _: () = msg_send![popover, setDelegate: delegate];

        (popover, delegate as *mut c_void)
    }
}

/// Returns the content view (NSView) of the popover's content view controller.
pub(crate) unsafe fn get_native_popover_content_view(popover: id) -> id {
    unsafe {
        let view_controller: id = msg_send![popover, contentViewController];
        msg_send![view_controller, view]
    }
}

/// Shows the popover anchored to an NSToolbarItem (macOS 14+).
///
/// On macOS < 14 this will be a no-op (the selector doesn't exist).
pub(crate) unsafe fn show_native_popover_relative_to_toolbar_item(
    popover: id,
    toolbar_item: id,
) {
    unsafe {
        let sel = sel!(showRelativeToToolbarItem:);
        if msg_send![popover, respondsToSelector: sel] {
            let _: () = msg_send![popover, showRelativeToToolbarItem: toolbar_item];
        }
    }
}

/// Closes the popover.
pub(crate) unsafe fn dismiss_native_popover(popover: id) {
    unsafe {
        let _: () = msg_send![popover, performClose: nil];
    }
}

/// Releases the popover and its delegate, freeing all callback memory.
pub(crate) unsafe fn release_native_popover(popover: id, delegate_ptr: *mut c_void) {
    unsafe {
        if !delegate_ptr.is_null() {
            let delegate = delegate_ptr as id;
            let callbacks_ptr: *mut c_void = *(*delegate).get_ivar(CALLBACK_IVAR);
            if !callbacks_ptr.is_null() {
                let _ = Box::from_raw(callbacks_ptr as *mut PopoverCallbacks);
            }
            let _: () = msg_send![delegate, release];
        }
        if popover != nil {
            let _: () = msg_send![popover, release];
        }
    }
}

/// Adds a label (non-editable NSTextField) to a content view at the given position.
/// Returns the created label id.
pub(crate) unsafe fn add_native_popover_label(
    content_view: id,
    text: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    font_size: f64,
    bold: bool,
) -> id {
    unsafe {
        use super::super::ns_string;

        let label: id = msg_send![class!(NSTextField), alloc];
        let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(width, height));
        let label: id = msg_send![label, initWithFrame: frame];
        let _: () = msg_send![label, setStringValue: ns_string(text)];
        let _: () = msg_send![label, setBezeled: false];
        let _: () = msg_send![label, setDrawsBackground: false];
        let _: () = msg_send![label, setEditable: false];
        let _: () = msg_send![label, setSelectable: false];

        let font: id = if bold {
            msg_send![class!(NSFont), boldSystemFontOfSize: font_size]
        } else {
            msg_send![class!(NSFont), systemFontOfSize: font_size]
        };
        let _: () = msg_send![label, setFont: font];

        let _: () = msg_send![content_view, addSubview: label];
        let _: () = msg_send![label, release];

        label
    }
}

/// Adds a smaller, secondary-colored label (for metadata/detail text).
pub(crate) unsafe fn add_native_popover_small_label(
    content_view: id,
    text: &str,
    x: f64,
    y: f64,
    width: f64,
) -> id {
    unsafe {
        use super::super::ns_string;

        let label: id = msg_send![class!(NSTextField), alloc];
        let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(width, 14.0));
        let label: id = msg_send![label, initWithFrame: frame];
        let _: () = msg_send![label, setStringValue: ns_string(text)];
        let _: () = msg_send![label, setBezeled: false];
        let _: () = msg_send![label, setDrawsBackground: false];
        let _: () = msg_send![label, setEditable: false];
        let _: () = msg_send![label, setSelectable: false];

        let font: id = msg_send![class!(NSFont), systemFontOfSize: 11.0f64];
        let _: () = msg_send![label, setFont: font];

        let color: id = msg_send![class!(NSColor), secondaryLabelColor];
        let _: () = msg_send![label, setTextColor: color];

        let _: () = msg_send![content_view, addSubview: label];
        let _: () = msg_send![label, release];

        label
    }
}

/// Adds a label with an SF Symbol icon to its left.
pub(crate) unsafe fn add_native_popover_icon_label(
    content_view: id,
    icon_name: &str,
    text: &str,
    x: f64,
    y: f64,
    width: f64,
) -> id {
    unsafe {
        use super::super::ns_string;

        // Create an NSImageView for the SF Symbol
        let image: id = msg_send![
            class!(NSImage),
            imageWithSystemSymbolName: ns_string(icon_name)
            accessibilityDescription: cocoa::base::nil
        ];

        let icon_size = 16.0;
        let icon_view: id = msg_send![class!(NSImageView), alloc];
        let icon_frame = NSRect::new(NSPoint::new(x, y + 2.0), NSSize::new(icon_size, icon_size));
        let icon_view: id = msg_send![icon_view, initWithFrame: icon_frame];

        if image != cocoa::base::nil {
            let _: () = msg_send![icon_view, setImage: image];
        }
        // NSImageScaleProportionallyUpOrDown = 3
        let _: () = msg_send![icon_view, setImageScaling: 3i64];

        let color: id = msg_send![class!(NSColor), secondaryLabelColor];
        let _: () = msg_send![icon_view, setContentTintColor: color];

        let _: () = msg_send![content_view, addSubview: icon_view];
        let _: () = msg_send![icon_view, release];

        // Create the text label offset to the right of the icon
        let text_x = x + icon_size + 6.0;
        let text_width = width - icon_size - 6.0;
        let label: id = msg_send![class!(NSTextField), alloc];
        let frame = NSRect::new(NSPoint::new(text_x, y), NSSize::new(text_width, 20.0));
        let label: id = msg_send![label, initWithFrame: frame];
        let _: () = msg_send![label, setStringValue: ns_string(text)];
        let _: () = msg_send![label, setBezeled: false];
        let _: () = msg_send![label, setDrawsBackground: false];
        let _: () = msg_send![label, setEditable: false];
        let _: () = msg_send![label, setSelectable: false];

        let font: id = msg_send![class!(NSFont), systemFontOfSize: 13.0f64];
        let _: () = msg_send![label, setFont: font];

        let _: () = msg_send![content_view, addSubview: label];
        let _: () = msg_send![label, release];

        label
    }
}

/// Adds an NSButton to the content view at the given position.
/// Returns the button id.
pub(crate) unsafe fn add_native_popover_button(
    content_view: id,
    title: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> id {
    unsafe {
        use super::super::ns_string;

        let button: id = msg_send![class!(NSButton), alloc];
        let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(width, height));
        let button: id = msg_send![button, initWithFrame: frame];
        let _: () = msg_send![button, setTitle: ns_string(title)];
        let _: () = msg_send![button, setBezelStyle: 1i64];

        let _: () = msg_send![content_view, addSubview: button];
        let _: () = msg_send![button, release];

        button
    }
}

/// Adds a horizontal separator (NSBox) to the content view at the given position.
pub(crate) unsafe fn add_native_popover_separator(
    content_view: id,
    x: f64,
    y: f64,
    width: f64,
) -> id {
    unsafe {
        let separator: id = msg_send![class!(NSBox), alloc];
        let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(width, 1.0));
        let separator: id = msg_send![separator, initWithFrame: frame];
        // NSBoxSeparator = 2
        let _: () = msg_send![separator, setBoxType: 2i64];

        let _: () = msg_send![content_view, addSubview: separator];
        let _: () = msg_send![separator, release];

        separator
    }
}

/// Adds a toggle row (label on left, NSSwitch on right) to a popover content view.
/// Returns the switch target pointer for cleanup (or null if no callback).
pub(crate) unsafe fn add_native_popover_toggle(
    content_view: id,
    text: &str,
    checked: bool,
    x: f64,
    y: f64,
    width: f64,
    enabled: bool,
    description: Option<&str>,
    on_change: Option<Box<dyn Fn(bool)>>,
) -> *mut c_void {
    unsafe {
        use super::super::ns_string;

        let switch_width = 38.0;
        let label_width = width - switch_width - 8.0;
        let has_desc = description.is_some();

        // In NSView bottom-up coords, y is the bottom of the item area.
        // With description (height=44): title in upper half, description in lower half.
        // Without description (height=30): title vertically centered.
        let label_y = if has_desc { y + 22.0 } else { y + 6.0 };
        let switch_y = if has_desc { y + 18.0 } else { y + 4.0 };

        // Label
        let label: id = msg_send![class!(NSTextField), alloc];
        let label_frame = NSRect::new(NSPoint::new(x, label_y), NSSize::new(label_width, 18.0));
        let label: id = msg_send![label, initWithFrame: label_frame];
        let _: () = msg_send![label, setStringValue: ns_string(text)];
        let _: () = msg_send![label, setBezeled: false];
        let _: () = msg_send![label, setDrawsBackground: false];
        let _: () = msg_send![label, setEditable: false];
        let _: () = msg_send![label, setSelectable: false];
        let font: id = msg_send![class!(NSFont), systemFontOfSize: 13.0f64];
        let _: () = msg_send![label, setFont: font];
        let _: () = msg_send![content_view, addSubview: label];
        let _: () = msg_send![label, release];

        // Description below label (if provided)
        if let Some(desc) = description {
            let desc_label: id = msg_send![class!(NSTextField), alloc];
            let desc_frame =
                NSRect::new(NSPoint::new(x, y + 2.0), NSSize::new(label_width, 14.0));
            let desc_label: id = msg_send![desc_label, initWithFrame: desc_frame];
            let _: () = msg_send![desc_label, setStringValue: ns_string(desc)];
            let _: () = msg_send![desc_label, setBezeled: false];
            let _: () = msg_send![desc_label, setDrawsBackground: false];
            let _: () = msg_send![desc_label, setEditable: false];
            let _: () = msg_send![desc_label, setSelectable: false];
            let small_font: id = msg_send![class!(NSFont), systemFontOfSize: 11.0f64];
            let _: () = msg_send![desc_label, setFont: small_font];
            let color: id = msg_send![class!(NSColor), secondaryLabelColor];
            let _: () = msg_send![desc_label, setTextColor: color];
            let _: () = msg_send![content_view, addSubview: desc_label];
            let _: () = msg_send![desc_label, release];
        }

        // NSSwitch on the right
        let switch = super::create_native_switch();
        let switch_x = x + width - switch_width;
        let switch_frame =
            NSRect::new(NSPoint::new(switch_x, switch_y), NSSize::new(switch_width, 22.0));
        let _: () = msg_send![switch, setFrame: switch_frame];
        super::set_native_switch_state(switch, checked);
        let _: () = msg_send![switch, setEnabled: enabled as i8];
        let _: () = msg_send![content_view, addSubview: switch];

        let target = if let Some(callback) = on_change {
            super::set_native_switch_action(switch, callback)
        } else {
            ptr::null_mut()
        };

        let _: () = msg_send![switch, release];

        target
    }
}

/// Adds a checkbox (NSButton in checkbox mode) to a popover content view.
/// Returns the checkbox target pointer for cleanup (or null if no callback).
pub(crate) unsafe fn add_native_popover_checkbox(
    content_view: id,
    text: &str,
    checked: bool,
    x: f64,
    y: f64,
    width: f64,
    enabled: bool,
    on_change: Option<Box<dyn Fn(bool)>>,
) -> *mut c_void {
    unsafe {
        let checkbox = super::create_native_checkbox(text);
        let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(width, 18.0));
        let _: () = msg_send![checkbox, setFrame: frame];
        super::set_native_checkbox_state(checkbox, checked);
        let _: () = msg_send![checkbox, setEnabled: enabled as i8];
        let _: () = msg_send![content_view, addSubview: checkbox];

        let target = if let Some(callback) = on_change {
            super::set_native_checkbox_action(checkbox, callback)
        } else {
            ptr::null_mut()
        };

        let _: () = msg_send![checkbox, release];

        target
    }
}

/// Adds a progress bar (NSProgressIndicator) with optional label to a popover content view.
pub(crate) unsafe fn add_native_popover_progress(
    content_view: id,
    value: f64,
    max: f64,
    label: Option<&str>,
    x: f64,
    y: f64,
    width: f64,
) {
    unsafe {
        use super::super::ns_string;

        let indicator = super::create_native_progress_indicator();
        let bar_height = 6.0;
        let bar_y = if label.is_some() { y + 18.0 } else { y + 7.0 };
        let indicator_frame = NSRect::new(NSPoint::new(x, bar_y), NSSize::new(width, bar_height));
        let _: () = msg_send![indicator, setFrame: indicator_frame];
        super::set_native_progress_style(indicator, 0); // bar style
        super::set_native_progress_indeterminate(indicator, false);
        super::set_native_progress_min_max(indicator, 0.0, max);
        super::set_native_progress_value(indicator, value);

        // Use small control size for a thinner bar
        // NSControlSizeMini = 3
        let _: () = msg_send![indicator, setControlSize: 3i64];

        let _: () = msg_send![content_view, addSubview: indicator];
        let _: () = msg_send![indicator, release];

        if let Some(label_text) = label {
            let label: id = msg_send![class!(NSTextField), alloc];
            let label_frame = NSRect::new(NSPoint::new(x, y), NSSize::new(width, 14.0));
            let label: id = msg_send![label, initWithFrame: label_frame];
            let _: () = msg_send![label, setStringValue: ns_string(label_text)];
            let _: () = msg_send![label, setBezeled: false];
            let _: () = msg_send![label, setDrawsBackground: false];
            let _: () = msg_send![label, setEditable: false];
            let _: () = msg_send![label, setSelectable: false];
            let font: id = msg_send![class!(NSFont), systemFontOfSize: 11.0f64];
            let _: () = msg_send![label, setFont: font];
            let color: id = msg_send![class!(NSColor), secondaryLabelColor];
            let _: () = msg_send![label, setTextColor: color];
            let _: () = msg_send![content_view, addSubview: label];
            let _: () = msg_send![label, release];
        }
    }
}

/// Returns the NSColor for a PlatformNativeColor.
pub(crate) unsafe fn ns_color_for_platform_color(
    color: crate::platform::PlatformNativeColor,
) -> id {
    unsafe {
        use crate::platform::PlatformNativeColor;
        match color {
            PlatformNativeColor::Red => msg_send![class!(NSColor), systemRedColor],
            PlatformNativeColor::Orange => msg_send![class!(NSColor), systemOrangeColor],
            PlatformNativeColor::Yellow => msg_send![class!(NSColor), systemYellowColor],
            PlatformNativeColor::Green => msg_send![class!(NSColor), systemGreenColor],
            PlatformNativeColor::Blue => msg_send![class!(NSColor), systemBlueColor],
            PlatformNativeColor::Purple => msg_send![class!(NSColor), systemPurpleColor],
            PlatformNativeColor::Gray => msg_send![class!(NSColor), systemGrayColor],
            PlatformNativeColor::Primary => msg_send![class!(NSColor), labelColor],
            PlatformNativeColor::Secondary => msg_send![class!(NSColor), secondaryLabelColor],
        }
    }
}

/// Adds a color dot row (colored circle + text + optional detail) to a popover content view.
/// Returns the button target pointer for cleanup (or null if no callback).
pub(crate) unsafe fn add_native_popover_color_dot(
    content_view: id,
    color: crate::platform::PlatformNativeColor,
    text: &str,
    detail: Option<&str>,
    x: f64,
    y: f64,
    width: f64,
    on_click: Option<Box<dyn Fn()>>,
) -> *mut c_void {
    unsafe {
        use super::super::ns_string;

        let dot_size = 10.0;
        let has_detail = detail.is_some();

        // In NSView bottom-up coords, y is the bottom of the item area.
        // With detail (height=38): title in upper half, detail in lower half.
        // Without detail (height=24): single line centered.
        let title_y = if has_detail { y + 18.0 } else { y + 3.0 };
        let dot_y = if has_detail { y + 22.0 } else { y + 7.0 };

        // Colored circle view
        let dot_view: id = msg_send![class!(NSView), alloc];
        let dot_frame = NSRect::new(NSPoint::new(x, dot_y), NSSize::new(dot_size, dot_size));
        let dot_view: id = msg_send![dot_view, initWithFrame: dot_frame];
        let _: () = msg_send![dot_view, setWantsLayer: true];
        let layer: id = msg_send![dot_view, layer];
        let _: () = msg_send![layer, setCornerRadius: dot_size / 2.0];
        let ns_color = ns_color_for_platform_color(color);
        let cg_color: id = msg_send![ns_color, CGColor];
        let _: () = msg_send![layer, setBackgroundColor: cg_color];
        let _: () = msg_send![content_view, addSubview: dot_view];
        let _: () = msg_send![dot_view, release];

        // Text label
        let text_x = x + dot_size + 8.0;
        let text_width = width - dot_size - 8.0;
        let label: id = msg_send![class!(NSTextField), alloc];
        let label_frame = NSRect::new(NSPoint::new(text_x, title_y), NSSize::new(text_width, 18.0));
        let label: id = msg_send![label, initWithFrame: label_frame];
        let _: () = msg_send![label, setStringValue: ns_string(text)];
        let _: () = msg_send![label, setBezeled: false];
        let _: () = msg_send![label, setDrawsBackground: false];
        let _: () = msg_send![label, setEditable: false];
        let _: () = msg_send![label, setSelectable: false];
        let font: id = msg_send![class!(NSFont), systemFontOfSize: 13.0f64];
        let _: () = msg_send![label, setFont: font];
        let _: () = msg_send![content_view, addSubview: label];
        let _: () = msg_send![label, release];

        // Detail label (if provided) — positioned in lower portion of item
        if let Some(detail_text) = detail {
            let detail_label: id = msg_send![class!(NSTextField), alloc];
            let detail_frame =
                NSRect::new(NSPoint::new(text_x, y + 2.0), NSSize::new(text_width, 14.0));
            let detail_label: id = msg_send![detail_label, initWithFrame: detail_frame];
            let _: () = msg_send![detail_label, setStringValue: ns_string(detail_text)];
            let _: () = msg_send![detail_label, setBezeled: false];
            let _: () = msg_send![detail_label, setDrawsBackground: false];
            let _: () = msg_send![detail_label, setEditable: false];
            let _: () = msg_send![detail_label, setSelectable: false];
            let small_font: id = msg_send![class!(NSFont), systemFontOfSize: 11.0f64];
            let _: () = msg_send![detail_label, setFont: small_font];
            let detail_color: id = msg_send![class!(NSColor), secondaryLabelColor];
            let _: () = msg_send![detail_label, setTextColor: detail_color];
            let _: () = msg_send![content_view, addSubview: detail_label];
            let _: () = msg_send![detail_label, release];
        }

        // If clickable, overlay with a transparent button
        if let Some(callback) = on_click {
            let row_height = if detail.is_some() { 38.0 } else { 24.0 };
            let button: id = msg_send![class!(NSButton), alloc];
            let button_frame = NSRect::new(NSPoint::new(x, y), NSSize::new(width, row_height));
            let button: id = msg_send![button, initWithFrame: button_frame];
            let _: () = msg_send![button, setTitle: ns_string("")];
            let _: () = msg_send![button, setTransparent: true];
            // NSBezelStyleSmallSquare = 10
            let _: () = msg_send![button, setBezelStyle: 10i64];
            let _: () = msg_send![content_view, addSubview: button];

            let target = super::set_native_button_action(button, callback);
            let _: () = msg_send![button, release];
            target
        } else {
            ptr::null_mut()
        }
    }
}

/// Adds a clickable row (optional icon + text + optional detail) to a popover content view.
/// Returns the button target pointer for cleanup (or null if not clickable).
pub(crate) unsafe fn add_native_popover_clickable_row(
    content_view: id,
    icon: Option<&str>,
    text: &str,
    detail: Option<&str>,
    x: f64,
    y: f64,
    width: f64,
    enabled: bool,
    on_click: Option<Box<dyn Fn()>>,
) -> *mut c_void {
    unsafe {
        use super::super::ns_string;

        let icon_offset = if icon.is_some() { 22.0 } else { 0.0 };
        let text_x = x + icon_offset;
        let text_width = width - icon_offset;

        let has_detail = detail.is_some();

        // Icon (if provided) — vertically aligned with title
        if let Some(icon_name) = icon {
            let image: id = msg_send![
                class!(NSImage),
                imageWithSystemSymbolName: ns_string(icon_name)
                accessibilityDescription: nil
            ];
            let icon_size = 16.0;
            let icon_y = if has_detail { y + 16.0 } else { y + 6.0 };
            let icon_view: id = msg_send![class!(NSImageView), alloc];
            let icon_frame =
                NSRect::new(NSPoint::new(x, icon_y), NSSize::new(icon_size, icon_size));
            let icon_view: id = msg_send![icon_view, initWithFrame: icon_frame];
            if image != nil {
                let _: () = msg_send![icon_view, setImage: image];
            }
            // NSImageScaleProportionallyUpOrDown = 3
            let _: () = msg_send![icon_view, setImageScaling: 3i64];
            let tint: id = msg_send![class!(NSColor), secondaryLabelColor];
            let _: () = msg_send![icon_view, setContentTintColor: tint];
            let _: () = msg_send![content_view, addSubview: icon_view];
            let _: () = msg_send![icon_view, release];
        }

        // Text label
        let label: id = msg_send![class!(NSTextField), alloc];
        let label_y = if has_detail { y + 16.0 } else { y + 5.0 };
        let label_frame = NSRect::new(NSPoint::new(text_x, label_y), NSSize::new(text_width, 18.0));
        let label: id = msg_send![label, initWithFrame: label_frame];
        let _: () = msg_send![label, setStringValue: ns_string(text)];
        let _: () = msg_send![label, setBezeled: false];
        let _: () = msg_send![label, setDrawsBackground: false];
        let _: () = msg_send![label, setEditable: false];
        let _: () = msg_send![label, setSelectable: false];
        let font: id = msg_send![class!(NSFont), systemFontOfSize: 13.0f64];
        let _: () = msg_send![label, setFont: font];
        if !enabled {
            let disabled_color: id = msg_send![class!(NSColor), tertiaryLabelColor];
            let _: () = msg_send![label, setTextColor: disabled_color];
        }
        let _: () = msg_send![content_view, addSubview: label];
        let _: () = msg_send![label, release];

        // Detail label (if provided) — positioned in lower portion of item
        if let Some(detail_text) = detail {
            let detail_label: id = msg_send![class!(NSTextField), alloc];
            let detail_frame =
                NSRect::new(NSPoint::new(text_x, y + 1.0), NSSize::new(text_width, 14.0));
            let detail_label: id = msg_send![detail_label, initWithFrame: detail_frame];
            let _: () = msg_send![detail_label, setStringValue: ns_string(detail_text)];
            let _: () = msg_send![detail_label, setBezeled: false];
            let _: () = msg_send![detail_label, setDrawsBackground: false];
            let _: () = msg_send![detail_label, setEditable: false];
            let _: () = msg_send![detail_label, setSelectable: false];
            let small_font: id = msg_send![class!(NSFont), systemFontOfSize: 11.0f64];
            let _: () = msg_send![detail_label, setFont: small_font];
            let detail_color: id = msg_send![class!(NSColor), secondaryLabelColor];
            let _: () = msg_send![detail_label, setTextColor: detail_color];
            let _: () = msg_send![content_view, addSubview: detail_label];
            let _: () = msg_send![detail_label, release];
        }

        // Overlay transparent button for click handling
        if let Some(callback) = on_click {
            let row_height = if detail.is_some() { 36.0 } else { 24.0 };
            let button: id = msg_send![class!(NSButton), alloc];
            let button_frame = NSRect::new(NSPoint::new(x, y), NSSize::new(width, row_height));
            let button: id = msg_send![button, initWithFrame: button_frame];
            let _: () = msg_send![button, setTitle: ns_string("")];
            let _: () = msg_send![button, setTransparent: true];
            // NSBezelStyleSmallSquare = 10
            let _: () = msg_send![button, setBezelStyle: 10i64];
            let _: () = msg_send![button, setEnabled: enabled as i8];
            let _: () = msg_send![content_view, addSubview: button];

            let target = super::set_native_button_action(button, callback);
            let _: () = msg_send![button, release];
            target
        } else {
            ptr::null_mut()
        }
    }
}

/// Releases a switch target for popover cleanup.
pub(crate) unsafe fn release_native_popover_switch_target(target: *mut c_void) {
    unsafe {
        super::release_native_switch_target(target);
    }
}

/// Releases a checkbox target for popover cleanup.
pub(crate) unsafe fn release_native_popover_checkbox_target(target: *mut c_void) {
    unsafe {
        super::release_native_checkbox_target(target);
    }
}
