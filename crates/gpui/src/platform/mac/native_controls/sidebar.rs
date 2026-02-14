use cocoa::{
    base::{id, nil},
    foundation::NSUInteger,
};
use objc::{class, msg_send, sel, sel_impl};

#[derive(Clone, Copy)]
pub(crate) struct NativeSidebarInstallation {
    pub controller: id,
    pub sidebar_view: id,
}

unsafe fn first_split_item(split_controller: id) -> id {
    let items: id = unsafe { msg_send![split_controller, splitViewItems] };
    if items.is_null() {
        return nil;
    }

    let count: NSUInteger = unsafe { msg_send![items, count] };
    if count == 0 {
        return nil;
    }

    unsafe { msg_send![items, objectAtIndex: 0usize] }
}

unsafe fn split_item_at(split_controller: id, index: usize) -> id {
    let items: id = unsafe { msg_send![split_controller, splitViewItems] };
    if items.is_null() {
        return nil;
    }

    let count: NSUInteger = unsafe { msg_send![items, count] };
    if index >= count as usize {
        return nil;
    }

    unsafe { msg_send![items, objectAtIndex: index] }
}

pub(crate) unsafe fn install_native_sidebar_on_window(
    window: id,
    detail_view: id,
    min_thickness: f64,
    max_thickness: f64,
    autosave_name: Option<&str>,
) -> NativeSidebarInstallation {
    use super::super::ns_string;

    let split_controller: id = unsafe { msg_send![class!(NSSplitViewController), alloc] };
    let split_controller: id = unsafe { msg_send![split_controller, init] };

    let sidebar_view_controller: id = unsafe { msg_send![class!(NSViewController), alloc] };
    let sidebar_view_controller: id = unsafe { msg_send![sidebar_view_controller, init] };
    let sidebar_view: id = unsafe { msg_send![class!(NSView), alloc] };
    let sidebar_view: id = unsafe { msg_send![sidebar_view, init] };
    let _: () = unsafe { msg_send![sidebar_view_controller, setView: sidebar_view] };

    let detail_view_controller: id = unsafe { msg_send![class!(NSViewController), alloc] };
    let detail_view_controller: id = unsafe { msg_send![detail_view_controller, init] };
    // Reparent GPUI's root view into the split item detail controller.
    let _: () = unsafe { msg_send![detail_view, removeFromSuperview] };
    let _: () = unsafe { msg_send![detail_view_controller, setView: detail_view] };

    let sidebar_item: id = unsafe {
        msg_send![class!(NSSplitViewItem), sidebarWithViewController: sidebar_view_controller]
    };
    let detail_item: id = unsafe {
        msg_send![class!(NSSplitViewItem), splitViewItemWithViewController: detail_view_controller]
    };

    let _: () = unsafe { msg_send![sidebar_item, setCanCollapse: true as i8] };
    let _: () = unsafe { msg_send![sidebar_item, setCanCollapseFromWindowResize: true as i8] };
    let _: () = unsafe { msg_send![sidebar_item, setSpringLoaded: true as i8] };
    let _: () = unsafe { msg_send![sidebar_item, setMinimumThickness: min_thickness] };
    let _: () = unsafe { msg_send![sidebar_item, setMaximumThickness: max_thickness] };
    let _: () =
        unsafe { msg_send![detail_item, setAutomaticallyAdjustsSafeAreaInsets: true as i8] };

    let _: () = unsafe { msg_send![split_controller, addSplitViewItem: sidebar_item] };
    let _: () = unsafe { msg_send![split_controller, addSplitViewItem: detail_item] };

    let split_view: id = unsafe { msg_send![split_controller, splitView] };
    let _: () = unsafe { msg_send![split_view, setVertical: true as i8] };
    if let Some(name) = autosave_name {
        let _: () = unsafe { msg_send![split_view, setAutosaveName: ns_string(name)] };
    }

    let _: () = unsafe { msg_send![window, setContentViewController: split_controller] };
    let _: () = unsafe { msg_send![window, makeFirstResponder: detail_view] };

    // Split items retain their view controllers; release local ownership.
    let _: () = unsafe { msg_send![sidebar_view_controller, release] };
    let _: () = unsafe { msg_send![detail_view_controller, release] };
    // Window retains its contentViewController; release local ownership.
    let _: () = unsafe { msg_send![split_controller, release] };

    NativeSidebarInstallation {
        controller: split_controller,
        sidebar_view,
    }
}

pub(crate) unsafe fn toggle_native_sidebar(split_controller: id) {
    if split_controller.is_null() {
        return;
    }

    let _: () = unsafe { msg_send![split_controller, toggleSidebar: nil] };
}

pub(crate) unsafe fn set_native_sidebar_collapsed(split_controller: id, collapsed: bool) -> bool {
    if split_controller.is_null() {
        return false;
    }

    let sidebar_item = unsafe { first_split_item(split_controller) };
    if sidebar_item.is_null() {
        return false;
    }

    let _: () = unsafe { msg_send![sidebar_item, setCollapsed: collapsed as i8] };
    true
}

pub(crate) unsafe fn is_native_sidebar_collapsed(split_controller: id) -> Option<bool> {
    if split_controller.is_null() {
        return None;
    }

    let sidebar_item = unsafe { first_split_item(split_controller) };
    if sidebar_item.is_null() {
        return None;
    }

    let collapsed: i8 = unsafe { msg_send![sidebar_item, isCollapsed] };
    Some(collapsed != 0)
}

pub(crate) unsafe fn set_native_sidebar_minimum_thickness_for_inline(
    split_controller: id,
    thickness: f64,
) {
    if split_controller.is_null() {
        return;
    }

    let _: () =
        unsafe { msg_send![split_controller, setMinimumThicknessForInlineSidebars: thickness] };
}

pub(crate) unsafe fn set_native_sidebar_thickness(split_controller: id, thickness: f64) -> bool {
    if split_controller.is_null() {
        return false;
    }

    let split_view: id = unsafe { msg_send![split_controller, splitView] };
    if split_view.is_null() {
        return false;
    }

    let detail_item = unsafe { split_item_at(split_controller, 1) };
    if detail_item.is_null() {
        return false;
    }

    let detail_collapsed: i8 = unsafe { msg_send![detail_item, isCollapsed] };
    if detail_collapsed != 0 {
        return false;
    }

    let _: () = unsafe { msg_send![split_view, setPosition: thickness ofDividerAtIndex: 0usize] };
    true
}

pub(crate) unsafe fn native_sidebar_thickness(split_controller: id) -> Option<f64> {
    if split_controller.is_null() {
        return None;
    }

    let split_view: id = unsafe { msg_send![split_controller, splitView] };
    if split_view.is_null() {
        return None;
    }

    let subviews: id = unsafe { msg_send![split_view, subviews] };
    if subviews.is_null() {
        return None;
    }

    let count: NSUInteger = unsafe { msg_send![subviews, count] };
    if count == 0 {
        return None;
    }

    let sidebar_view: id = unsafe { msg_send![subviews, objectAtIndex: 0usize] };
    let frame: cocoa::foundation::NSRect = unsafe { msg_send![sidebar_view, frame] };
    Some(frame.size.width)
}
