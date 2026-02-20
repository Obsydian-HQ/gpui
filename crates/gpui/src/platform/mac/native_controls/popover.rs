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
