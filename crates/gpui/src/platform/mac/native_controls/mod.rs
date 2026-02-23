#[allow(dead_code)]
mod alert;
mod button;
mod checkbox;
mod collection;
mod combo_box;
mod glass_effect_view;
mod menu;
mod outline;
#[allow(dead_code)]
mod panel;
mod popover;
mod popup;
mod progress;
mod search_field;
mod segmented;
#[allow(dead_code)]
mod sidebar;
mod slider;
#[allow(dead_code)]
mod stack_view;
mod stepper;
mod switch;
mod tab_view;
mod table;
#[allow(dead_code)]
mod image_view;
#[allow(dead_code)]
mod text_field;
#[allow(dead_code)]
mod tracking_area;
mod visual_effect_view;

pub(crate) use alert::*;
pub(crate) use button::*;
pub(crate) use checkbox::*;
pub(crate) use collection::*;
pub(crate) use combo_box::*;
pub(crate) use glass_effect_view::*;
pub(crate) use image_view::*;
pub(crate) use menu::*;
pub(crate) use outline::*;
pub(crate) use panel::*;
pub(crate) use popover::*;
pub(crate) use popup::*;
pub(crate) use progress::*;
pub(crate) use search_field::*;
pub(crate) use segmented::*;
pub(crate) use sidebar::*;
pub(crate) use slider::*;
pub(crate) use stack_view::*;
pub(crate) use stepper::*;
pub(crate) use switch::*;
pub(crate) use tab_view::*;
pub(crate) use table::*;
pub(crate) use text_field::*;
pub(crate) use tracking_area::*;
pub(crate) use visual_effect_view::*;

use crate::{Bounds, Pixels};
use cocoa::{
    base::id,
    foundation::{NSPoint, NSRect, NSSize},
};
use objc::{msg_send, sel, sel_impl};

pub(super) const CALLBACK_IVAR: &str = "callbackPtr";

// =============================================================================
// Shared view helpers
// =============================================================================

/// Adds a native view as a subview of the given parent view.
pub(crate) unsafe fn attach_native_view_to_parent(view: id, parent: id) {
    unsafe {
        let _: () = msg_send![parent, addSubview: view];
    }
}

/// Alias for backwards compat.
pub(crate) unsafe fn attach_native_button_to_view(button: id, parent: id) {
    unsafe { attach_native_view_to_parent(button, parent) };
}

/// Positions any NSView within its parent, converting from GPUI's top-down coordinate
/// system to NSView's bottom-up coordinate system.
pub(crate) unsafe fn set_native_view_frame(
    view: id,
    bounds: Bounds<Pixels>,
    parent_view: id,
    _scale_factor: f32,
) {
    unsafe {
        let x = bounds.origin.x.0 as f64;
        let y = bounds.origin.y.0 as f64;
        let w = bounds.size.width.0 as f64;
        let h = bounds.size.height.0 as f64;

        let is_flipped: bool = msg_send![parent_view, isFlipped];
        let final_y = if is_flipped {
            y
        } else {
            let parent_frame: NSRect = msg_send![parent_view, frame];
            parent_frame.size.height - y - h
        };

        let frame = NSRect::new(NSPoint::new(x, final_y), NSSize::new(w, h));
        let _: () = msg_send![view, setFrame: frame];
    }
}

/// Alias for backwards compat.
pub(crate) unsafe fn set_native_button_frame(
    button: id,
    bounds: Bounds<Pixels>,
    parent_view: id,
    scale_factor: f32,
) {
    unsafe { set_native_view_frame(button, bounds, parent_view, scale_factor) };
}

/// Removes a native view from its parent.
pub(crate) unsafe fn remove_native_view_from_parent(view: id) {
    unsafe {
        let _: () = msg_send![view, removeFromSuperview];
    }
}

/// Alias for backwards compat.
#[allow(dead_code)]
pub(crate) unsafe fn remove_native_button_from_view(button: id) {
    unsafe { remove_native_view_from_parent(button) };
}

/// Sets the enabled state of an NSControl (button, segmented control, etc.).
pub(crate) unsafe fn set_native_control_enabled(control: id, enabled: bool) {
    unsafe {
        let _: () = msg_send![control, setEnabled: enabled as i8];
    }
}

/// Sets the tooltip on any NSView.
pub(crate) unsafe fn set_native_view_tooltip(view: id, tooltip: &str) {
    unsafe {
        use super::ns_string;
        let _: () = msg_send![view, setToolTip: ns_string(tooltip)];
    }
}
