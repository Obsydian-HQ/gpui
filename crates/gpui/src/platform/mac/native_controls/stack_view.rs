use cocoa::{
    base::{id, nil},
    foundation::{NSPoint, NSRect, NSSize},
};
use objc::{class, msg_send, sel, sel_impl};

/// Creates an NSStackView with the given orientation.
/// orientation: 0 = Horizontal, 1 = Vertical
pub(crate) unsafe fn create_native_stack_view(orientation: i64) -> id {
    unsafe {
        let view: id = msg_send![class!(NSStackView), alloc];
        let view: id = msg_send![view, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(200.0, 40.0),
        )];
        let _: () = msg_send![view, setOrientation: orientation];
        let _: () = msg_send![view, setAutoresizingMask: 0u64];
        // Default to center alignment
        let alignment: i64 = if orientation == 0 { 10 } else { 9 }; // CenterY or CenterX
        let _: () = msg_send![view, setAlignment: alignment];
        view
    }
}

pub(crate) unsafe fn set_native_stack_view_spacing(view: id, spacing: f64) {
    unsafe {
        let _: () = msg_send![view, setSpacing: spacing];
    }
}

/// alignment maps to NSLayoutAttribute values:
/// CenterY=10, CenterX=9, Top=3, Bottom=4, Leading=5, Trailing=6
pub(crate) unsafe fn set_native_stack_view_alignment(view: id, alignment: i64) {
    unsafe {
        let _: () = msg_send![view, setAlignment: alignment];
    }
}

/// distribution: GravityAreas=0, EqualCentering=1, EqualSpacing=2,
/// Fill=3, FillEqually=4, FillProportionally=5
pub(crate) unsafe fn set_native_stack_view_distribution(view: id, distribution: i64) {
    unsafe {
        let _: () = msg_send![view, setDistribution: distribution];
    }
}

pub(crate) unsafe fn set_native_stack_view_edge_insets(
    view: id,
    top: f64,
    left: f64,
    bottom: f64,
    right: f64,
) {
    unsafe {
        // NSEdgeInsets { top, left, bottom, right }
        #[repr(C)]
        struct NSEdgeInsets {
            top: f64,
            left: f64,
            bottom: f64,
            right: f64,
        }
        let insets = NSEdgeInsets {
            top,
            left,
            bottom,
            right,
        };
        let _: () = msg_send![view, setEdgeInsets: insets];
    }
}

pub(crate) unsafe fn add_native_stack_view_arranged_subview(stack: id, subview: id) {
    unsafe {
        let _: () = msg_send![stack, addArrangedSubview: subview];
    }
}

pub(crate) unsafe fn remove_native_stack_view_arranged_subview(stack: id, subview: id) {
    unsafe {
        let _: () = msg_send![stack, removeArrangedSubview: subview];
        let _: () = msg_send![subview, removeFromSuperview];
    }
}

pub(crate) unsafe fn remove_all_native_stack_view_arranged_subviews(stack: id) {
    unsafe {
        let arranged: id = msg_send![stack, arrangedSubviews];
        let count: u64 = msg_send![arranged, count];
        // Remove in reverse to avoid index shifting
        for i in (0..count).rev() {
            let subview: id = msg_send![arranged, objectAtIndex: i];
            let _: () = msg_send![stack, removeArrangedSubview: subview];
            let _: () = msg_send![subview, removeFromSuperview];
        }
    }
}

pub(crate) unsafe fn set_native_stack_view_detach_hidden(view: id, detach: bool) {
    unsafe {
        let _: () = msg_send![view, setDetachesHiddenViews: detach as i8];
    }
}

pub(crate) unsafe fn release_native_stack_view(view: id) {
    unsafe {
        if view != nil {
            let _: () = msg_send![view, removeFromSuperview];
            let _: () = msg_send![view, release];
        }
    }
}
