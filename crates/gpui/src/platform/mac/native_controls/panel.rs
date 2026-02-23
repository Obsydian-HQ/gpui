use super::CALLBACK_IVAR;
use cocoa::{
    base::{id, nil, BOOL, NO, YES},
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

// NSWindow level constants
const NS_NORMAL_WINDOW_LEVEL: i64 = 0;
const NS_FLOATING_WINDOW_LEVEL: i64 = 3;
const NS_MODAL_PANEL_WINDOW_LEVEL: i64 = 8;
const NS_POP_UP_MENU_WINDOW_LEVEL: i64 = 101;

// NSWindowStyleMask flags
const NS_TITLED_WINDOW_MASK: u64 = 1 << 0;
const NS_CLOSABLE_WINDOW_MASK: u64 = 1 << 1;
const NS_RESIZABLE_WINDOW_MASK: u64 = 1 << 3;
const NS_BORDERLESS_WINDOW_MASK: u64 = 0;
const NS_FULL_SIZE_CONTENT_VIEW_WINDOW_MASK: u64 = 1 << 15;
const NS_NONACTIVATING_PANEL_MASK: u64 = 1 << 7;
const NS_HUD_WINDOW_MASK: u64 = 1 << 13;
const NS_UTILITY_WINDOW_MASK: u64 = 1 << 4;

// NSBackingStoreType
const NS_BACKING_STORE_BUFFERED: u64 = 2;

// NSVisualEffectMaterial
const NS_VISUAL_EFFECT_MATERIAL_HUD_WINDOW: i64 = 13;
const NS_VISUAL_EFFECT_MATERIAL_POPOVER: i64 = 15;
const NS_VISUAL_EFFECT_MATERIAL_SIDEBAR: i64 = 7;
const NS_VISUAL_EFFECT_MATERIAL_UNDER_WINDOW: i64 = 21;

// NSVisualEffectBlendingMode
const NS_VISUAL_EFFECT_BLENDING_MODE_BEHIND_WINDOW: i64 = 0;

// NSVisualEffectState
const NS_VISUAL_EFFECT_STATE_ACTIVE: i64 = 1;

struct PanelCallbacks {
    on_close: Option<Box<dyn Fn()>>,
}

static mut PANEL_DELEGATE_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_panel_delegate_class() {
    unsafe {
        let mut decl =
            ClassDecl::new("GPUINativePanelDelegate", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);

        decl.add_method(
            sel!(windowWillClose:),
            panel_will_close as extern "C" fn(&Object, Sel, id),
        );

        PANEL_DELEGATE_CLASS = decl.register();
    }
}

extern "C" fn panel_will_close(this: &Object, _sel: Sel, _notification: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if ptr.is_null() {
            return;
        }
        let callbacks = &*(ptr as *const PanelCallbacks);
        if let Some(ref on_close) = callbacks.on_close {
            on_close();
        }
    }
}

/// Specifies the floating level for a native panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativePanelLevel {
    Normal,
    Floating,
    ModalPanel,
    PopUpMenu,
    Custom(i64),
}

impl NativePanelLevel {
    fn to_raw(self) -> i64 {
        match self {
            Self::Normal => NS_NORMAL_WINDOW_LEVEL,
            Self::Floating => NS_FLOATING_WINDOW_LEVEL,
            Self::ModalPanel => NS_MODAL_PANEL_WINDOW_LEVEL,
            Self::PopUpMenu => NS_POP_UP_MENU_WINDOW_LEVEL,
            Self::Custom(level) => level,
        }
    }
}

/// Style configuration for a native panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativePanelStyle {
    /// Standard titled panel with close button.
    Titled,
    /// Borderless panel (no title bar, no chrome). Used for suggestion boxes, launchers, etc.
    Borderless,
    /// HUD panel — dark translucent appearance.
    Hud,
    /// Utility panel — smaller title bar, floats above regular windows.
    Utility,
}

/// Visual effect material for a panel's background.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativePanelMaterial {
    HudWindow,
    Popover,
    Sidebar,
    UnderWindow,
}

impl NativePanelMaterial {
    fn to_raw(self) -> i64 {
        match self {
            Self::HudWindow => NS_VISUAL_EFFECT_MATERIAL_HUD_WINDOW,
            Self::Popover => NS_VISUAL_EFFECT_MATERIAL_POPOVER,
            Self::Sidebar => NS_VISUAL_EFFECT_MATERIAL_SIDEBAR,
            Self::UnderWindow => NS_VISUAL_EFFECT_MATERIAL_UNDER_WINDOW,
        }
    }
}

/// Creates a standalone NSPanel with the given configuration.
///
/// Returns `(panel, delegate_ptr)`. The caller owns both and must eventually
/// call `release_native_panel` to clean up.
pub(crate) unsafe fn create_native_panel(
    width: f64,
    height: f64,
    style: NativePanelStyle,
    level: NativePanelLevel,
    non_activating: bool,
    has_shadow: bool,
    corner_radius: f64,
    material: Option<NativePanelMaterial>,
    on_close: Option<Box<dyn Fn()>>,
) -> (id, *mut c_void) {
    unsafe {
        let style_mask = match style {
            NativePanelStyle::Titled => {
                NS_TITLED_WINDOW_MASK
                    | NS_CLOSABLE_WINDOW_MASK
                    | NS_FULL_SIZE_CONTENT_VIEW_WINDOW_MASK
            }
            NativePanelStyle::Borderless => NS_BORDERLESS_WINDOW_MASK,
            NativePanelStyle::Hud => {
                NS_TITLED_WINDOW_MASK
                    | NS_CLOSABLE_WINDOW_MASK
                    | NS_HUD_WINDOW_MASK
                    | NS_UTILITY_WINDOW_MASK
            }
            NativePanelStyle::Utility => {
                NS_TITLED_WINDOW_MASK
                    | NS_CLOSABLE_WINDOW_MASK
                    | NS_UTILITY_WINDOW_MASK
            }
        };

        let style_mask = if non_activating {
            style_mask | NS_NONACTIVATING_PANEL_MASK
        } else {
            style_mask
        };

        let frame = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(width, height),
        );

        let panel: id = msg_send![class!(NSPanel), alloc];
        let panel: id = msg_send![
            panel,
            initWithContentRect: frame
            styleMask: style_mask
            backing: NS_BACKING_STORE_BUFFERED
            defer: NO
        ];

        let _: () = msg_send![panel, setLevel: level.to_raw()];
        let _: () = msg_send![panel, setHasShadow: has_shadow as BOOL];
        let _: () = msg_send![panel, setReleasedWhenClosed: NO];
        let _: () = msg_send![panel, setMovableByWindowBackground: YES];

        // For borderless panels, make the background transparent so visual effect shows through
        if style == NativePanelStyle::Borderless {
            let _: () = msg_send![panel, setOpaque: NO];
            let clear_color: id = msg_send![class!(NSColor), clearColor];
            let _: () = msg_send![panel, setBackgroundColor: clear_color];
        }

        // For titled panels, make the titlebar transparent for a cleaner look
        if style == NativePanelStyle::Titled {
            let _: () = msg_send![panel, setTitlebarAppearsTransparent: YES];
            let _: () = msg_send![panel, setTitleVisibility: 1i64]; // NSWindowTitleHidden
        }

        // Apply visual effect material if requested
        let content_view: id = msg_send![panel, contentView];
        if let Some(mat) = material {
            let effect_view: id = msg_send![class!(NSVisualEffectView), alloc];
            let content_frame: NSRect = msg_send![content_view, bounds];
            let effect_view: id = msg_send![effect_view, initWithFrame: content_frame];
            let _: () = msg_send![effect_view, setMaterial: mat.to_raw()];
            let _: () = msg_send![effect_view, setBlendingMode: NS_VISUAL_EFFECT_BLENDING_MODE_BEHIND_WINDOW];
            let _: () = msg_send![effect_view, setState: NS_VISUAL_EFFECT_STATE_ACTIVE];
            // NSViewWidthSizable | NSViewHeightSizable = 18
            let _: () = msg_send![effect_view, setAutoresizingMask: 18u64];

            if corner_radius > 0.0 {
                let _: () = msg_send![effect_view, setWantsLayer: YES];
                let layer: id = msg_send![effect_view, layer];
                let _: () = msg_send![layer, setCornerRadius: corner_radius];
                let _: () = msg_send![layer, setMasksToBounds: YES];
            }

            let _: () = msg_send![content_view, addSubview: effect_view];
            let _: () = msg_send![effect_view, release];
        } else if corner_radius > 0.0 {
            let _: () = msg_send![content_view, setWantsLayer: YES];
            let layer: id = msg_send![content_view, layer];
            let _: () = msg_send![layer, setCornerRadius: corner_radius];
            let _: () = msg_send![layer, setMasksToBounds: YES];
        }

        // Set up delegate for close callback
        let delegate: id = msg_send![PANEL_DELEGATE_CLASS, alloc];
        let delegate: id = msg_send![delegate, init];

        let callbacks = PanelCallbacks { on_close };
        let callbacks_ptr = Box::into_raw(Box::new(callbacks)) as *mut c_void;
        (*delegate).set_ivar::<*mut c_void>(CALLBACK_IVAR, callbacks_ptr);
        let _: () = msg_send![panel, setDelegate: delegate];

        (panel, delegate as *mut c_void)
    }
}

/// Returns the content view of the panel.
pub(crate) unsafe fn get_native_panel_content_view(panel: id) -> id {
    unsafe {
        msg_send![panel, contentView]
    }
}

/// Shows the panel, ordering it to the front.
pub(crate) unsafe fn show_native_panel(panel: id) {
    unsafe {
        let _: () = msg_send![panel, orderFront: nil];
    }
}

/// Shows the panel centered on screen.
pub(crate) unsafe fn show_native_panel_centered(panel: id) {
    unsafe {
        let _: () = msg_send![panel, center];
        let _: () = msg_send![panel, orderFront: nil];
    }
}

/// Positions the panel at a specific screen location (bottom-left origin).
pub(crate) unsafe fn set_native_panel_frame_origin(panel: id, x: f64, y: f64) {
    unsafe {
        let origin = NSPoint::new(x, y);
        let _: () = msg_send![panel, setFrameOrigin: origin];
    }
}

/// Positions the panel at a specific screen location (top-left origin, GPUI-style).
pub(crate) unsafe fn set_native_panel_frame_top_left(panel: id, x: f64, y: f64) {
    unsafe {
        let point = NSPoint::new(x, y);
        let _: () = msg_send![panel, setFrameTopLeftPoint: point];
    }
}

/// Sets the panel's size.
pub(crate) unsafe fn set_native_panel_size(panel: id, width: f64, height: f64) {
    unsafe {
        let frame: NSRect = msg_send![panel, frame];
        let new_frame = NSRect::new(
            frame.origin,
            NSSize::new(width, height),
        );
        let _: () = msg_send![panel, setFrame: new_frame display: YES animate: NO];
    }
}

/// Sets the panel's full frame (origin + size).
pub(crate) unsafe fn set_native_panel_frame(
    panel: id,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    animate: bool,
) {
    unsafe {
        let frame = NSRect::new(
            NSPoint::new(x, y),
            NSSize::new(width, height),
        );
        let _: () = msg_send![panel, setFrame: frame display: YES animate: animate as BOOL];
    }
}

/// Closes the panel.
pub(crate) unsafe fn close_native_panel(panel: id) {
    unsafe {
        let _: () = msg_send![panel, close];
    }
}

/// Orders the panel out (hides without closing).
pub(crate) unsafe fn hide_native_panel(panel: id) {
    unsafe {
        let _: () = msg_send![panel, orderOut: nil];
    }
}

/// Returns whether the panel is currently visible.
pub(crate) unsafe fn is_native_panel_visible(panel: id) -> bool {
    unsafe {
        let visible: BOOL = msg_send![panel, isVisible];
        visible != NO
    }
}

/// Gets the screen frame of a toolbar item by its identifier.
/// Returns None if the toolbar or item is not found.
pub(crate) unsafe fn get_toolbar_item_screen_frame(
    window: id,
    item_identifier: &str,
) -> Option<NSRect> {
    unsafe {
        let toolbar: id = msg_send![window, toolbar];
        if toolbar == nil {
            return None;
        }

        let items: id = msg_send![toolbar, items];
        let count: usize = msg_send![items, count];

        for i in 0..count {
            let item: id = msg_send![items, objectAtIndex: i];
            let item_id: id = msg_send![item, itemIdentifier];

            let item_id_str: *const std::os::raw::c_char = msg_send![item_id, UTF8String];
            if item_id_str.is_null() {
                continue;
            }
            let id_str = std::ffi::CStr::from_ptr(item_id_str)
                .to_str()
                .unwrap_or("");

            if id_str == item_identifier {
                let view: id = msg_send![item, view];
                if view == nil {
                    return None;
                }

                let view_bounds: NSRect = msg_send![view, bounds];
                let window_rect: NSRect =
                    msg_send![view, convertRect: view_bounds toView: nil];
                let view_window: id = msg_send![view, window];
                if view_window == nil {
                    return None;
                }
                let screen_rect: NSRect =
                    msg_send![view_window, convertRectToScreen: window_rect];
                return Some(screen_rect);
            }
        }

        None
    }
}

/// Releases the panel and its delegate, freeing all callback memory.
pub(crate) unsafe fn release_native_panel(panel: id, delegate_ptr: *mut c_void) {
    unsafe {
        // Detach the delegate from the panel BEFORE closing, so `windowWillClose:`
        // won't fire on our delegate after we've freed its callback memory.
        if panel != nil {
            let _: () = msg_send![panel, setDelegate: nil];
        }

        if !delegate_ptr.is_null() {
            let delegate = delegate_ptr as id;
            let callbacks_ptr: *mut c_void = *(*delegate).get_ivar(CALLBACK_IVAR);
            if !callbacks_ptr.is_null() {
                let _ = Box::from_raw(callbacks_ptr as *mut PanelCallbacks);
            }
            let _: () = msg_send![delegate, release];
        }
        if panel != nil {
            let _: () = msg_send![panel, close];
            let _: () = msg_send![panel, release];
        }
    }
}
