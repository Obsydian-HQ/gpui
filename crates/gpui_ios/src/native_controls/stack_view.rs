use super::id;
use objc::{class, msg_send, sel, sel_impl};

/// Creates a UIStackView.
pub(crate) unsafe fn create_native_stack_view(orientation: i64) -> id {
    unsafe {
        let stack: id = msg_send![class!(UIStackView), alloc];
        let stack: id = msg_send![stack, init];
        // UILayoutConstraintAxis: 0 = horizontal, 1 = vertical
        let axis = if orientation == 0 { 0i64 } else { 1i64 };
        let _: () = msg_send![stack, setAxis: axis];
        // Default distribution: fill
        let _: () = msg_send![stack, setDistribution: 0i64]; // UIStackViewDistributionFill
        stack
    }
}

/// Sets the spacing between arranged subviews.
pub(crate) unsafe fn set_native_stack_view_spacing(view: id, spacing: f64) {
    unsafe {
        let _: () = msg_send![view, setSpacing: spacing];
    }
}

/// Sets the alignment. Maps to UIStackView.Alignment.
pub(crate) unsafe fn set_native_stack_view_alignment(view: id, alignment: i64) {
    unsafe {
        // UIStackViewAlignment: 0 = fill, 1 = leading, 2 = firstBaseline,
        // 3 = center, 4 = trailing, 5 = lastBaseline
        let _: () = msg_send![view, setAlignment: alignment];
    }
}

/// Sets the distribution mode.
pub(crate) unsafe fn set_native_stack_view_distribution(view: id, distribution: i64) {
    unsafe {
        // UIStackViewDistribution: 0 = fill, 1 = fillEqually, 2 = fillProportionally,
        // 3 = equalSpacing, 4 = equalCentering
        let _: () = msg_send![view, setDistribution: distribution];
    }
}

/// Sets edge insets (layout margins).
pub(crate) unsafe fn set_native_stack_view_edge_insets(
    view: id,
    top: f64,
    left: f64,
    bottom: f64,
    right: f64,
) {
    unsafe {
        let _: () = msg_send![view, setLayoutMarginsRelativeArrangement: true as i8];
        // UIEdgeInsets: {top, left, bottom, right}
        let insets: (f64, f64, f64, f64) = (top, left, bottom, right);
        let _: () = msg_send![view, setLayoutMargins: insets];
    }
}

/// Adds an arranged subview.
pub(crate) unsafe fn add_native_stack_view_arranged_subview(stack: id, subview: id) {
    unsafe {
        let _: () = msg_send![stack, addArrangedSubview: subview];
    }
}

/// Removes all arranged subviews.
pub(crate) unsafe fn remove_all_native_stack_view_arranged_subviews(stack: id) {
    unsafe {
        let subviews: id = msg_send![stack, arrangedSubviews];
        let count: usize = msg_send![subviews, count];
        for i in (0..count).rev() {
            let subview: id = msg_send![subviews, objectAtIndex: i];
            let _: () = msg_send![stack, removeArrangedSubview: subview];
            let _: () = msg_send![subview, removeFromSuperview];
        }
    }
}

/// No-op on iOS (UIStackView always detaches hidden subviews by default).
pub(crate) unsafe fn set_native_stack_view_detach_hidden(_view: id, _detach: bool) {}

/// Releases a UIStackView.
pub(crate) unsafe fn release_native_stack_view(view: id) {
    unsafe {
        if !view.is_null() {
            let _: () = msg_send![view, release];
        }
    }
}
