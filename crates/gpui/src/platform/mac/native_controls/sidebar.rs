use super::CALLBACK_IVAR;
use cocoa::{
    appkit::{
        NSEventModifierFlags, NSViewHeightSizable, NSViewWidthSizable, NSVisualEffectBlendingMode,
        NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindowStyleMask,
        NSWindowTitleVisibility,
    },
    base::{id, nil},
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

const HOST_DATA_IVAR: &str = "sidebarHostDataPtr";

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {
    static NSToolbarFlexibleSpaceItemIdentifier: id;
    static NSToolbarToggleSidebarItemIdentifier: id;
    static NSToolbarSidebarTrackingSeparatorItemIdentifier: id;
}

struct SidebarHostData {
    split_view_controller: id,
    split_view: id,
    sidebar_item: id,
    scroll_view: id,
    table_view: id,
    detail_label: id,
    window: id,
    embedded_content_view: id,
    previous_content_view_controller: id,
    previous_toolbar: id,
    sidebar_toolbar: id,
    previous_content_min_size: NSSize,
    previous_content_max_size: NSSize,
    min_width: f64,
    max_width: f64,
}

struct SidebarCallbacks {
    items: Vec<String>,
    on_select: Option<Box<dyn Fn((usize, String))>>,
    table_view: id,
    detail_label: id,
}

static mut SIDEBAR_HOST_VIEW_CLASS: *const Class = ptr::null();
static mut SIDEBAR_DELEGATE_CLASS: *const Class = ptr::null();

#[inline]
unsafe fn toolbar_flexible_space_identifier() -> id {
    unsafe { NSToolbarFlexibleSpaceItemIdentifier }
}

#[inline]
unsafe fn toolbar_toggle_sidebar_identifier() -> id {
    unsafe { NSToolbarToggleSidebarItemIdentifier }
}

#[inline]
unsafe fn toolbar_sidebar_tracking_separator_identifier() -> id {
    unsafe { NSToolbarSidebarTrackingSeparatorItemIdentifier }
}

#[ctor]
unsafe fn build_sidebar_host_view_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUINativeSidebarHostView", class!(NSView)).unwrap();
        decl.add_ivar::<*mut c_void>(HOST_DATA_IVAR);
        decl.add_method(
            sel!(performKeyEquivalent:),
            host_view_perform_key_equivalent as extern "C" fn(&Object, Sel, id) -> i8,
        );
        SIDEBAR_HOST_VIEW_CLASS = decl.register();
    }
}

#[ctor]
unsafe fn build_sidebar_delegate_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUINativeSidebarDelegate", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);

        decl.add_method(
            sel!(numberOfRowsInTableView:),
            number_of_rows as extern "C" fn(&Object, Sel, id) -> i64,
        );
        decl.add_method(
            sel!(tableView:objectValueForTableColumn:row:),
            object_value_for_row as extern "C" fn(&Object, Sel, id, id, i64) -> id,
        );
        decl.add_method(
            sel!(tableViewSelectionDidChange:),
            selection_did_change as extern "C" fn(&Object, Sel, id),
        );

        SIDEBAR_DELEGATE_CLASS = decl.register();
    }
}

unsafe fn host_data_ptr(host_view: id) -> *mut SidebarHostData {
    unsafe {
        if host_view == nil {
            return ptr::null_mut();
        }
        let object = host_view as *mut Object;
        let ptr: *mut c_void = *(*object).get_ivar(HOST_DATA_IVAR);
        ptr as *mut SidebarHostData
    }
}

unsafe fn host_data_mut(host_view: id) -> Option<&'static mut SidebarHostData> {
    unsafe {
        let ptr = host_data_ptr(host_view);
        if ptr.is_null() { None } else { Some(&mut *ptr) }
    }
}

unsafe fn primary_table_column(table: id) -> id {
    unsafe {
        let columns: id = msg_send![table, tableColumns];
        let count: u64 = msg_send![columns, count];
        if count == 0 {
            nil
        } else {
            msg_send![columns, objectAtIndex: 0u64]
        }
    }
}

unsafe fn set_detail_label_text(detail_label: id, text: &str) {
    unsafe {
        use super::super::ns_string;
        if detail_label != nil {
            let _: () = msg_send![detail_label, setStringValue: ns_string(text)];
        }
    }
}

unsafe fn sync_sidebar_table_width(host_data: &SidebarHostData) {
    unsafe {
        if host_data.scroll_view == nil || host_data.table_view == nil {
            return;
        }

        let clip_view: id = msg_send![host_data.scroll_view, contentView];
        if clip_view == nil {
            return;
        }

        let clip_bounds: NSRect = msg_send![clip_view, bounds];
        let table_width = (clip_bounds.size.width - 1.0).max(0.0);
        if table_width <= 0.0 {
            return;
        }

        let column = primary_table_column(host_data.table_view);
        if column != nil {
            let _: () = msg_send![column, setWidth: table_width];
        }

        let table_frame: NSRect = msg_send![host_data.table_view, frame];
        let _: () = msg_send![
            host_data.table_view,
            setFrameSize: NSSize::new(table_width, table_frame.size.height)
        ];

        let _: () = msg_send![
            clip_view,
            scrollToPoint: NSPoint::new(0.0, clip_bounds.origin.y)
        ];
        let _: () = msg_send![host_data.scroll_view, reflectScrolledClipView: clip_view];
    }
}

fn clamp_min_max(min_width: f64, max_width: f64) -> (f64, f64) {
    let min = min_width.max(120.0);
    (min, max_width.max(min))
}

fn clamped_sidebar_width(split_view: id, width: f64, min_width: f64, max_width: f64) -> f64 {
    unsafe {
        let frame: NSRect = msg_send![split_view, frame];
        let split_width = frame.size.width.max(0.0);
        let width = width.max(min_width).min(max_width);

        if split_width > 0.0 {
            let max_for_split = (split_width - 120.0).max(min_width);
            width.min(max_for_split)
        } else {
            width
        }
    }
}

unsafe fn ns_string_equals(lhs: id, rhs: id) -> bool {
    unsafe {
        if lhs == nil || rhs == nil {
            return false;
        }
        let eq: i8 = msg_send![lhs, isEqualToString: rhs];
        eq != 0
    }
}

unsafe fn toolbar_has_identifier(toolbar: id, identifier: id) -> bool {
    unsafe {
        if toolbar == nil || identifier == nil {
            return false;
        }
        let items: id = msg_send![toolbar, items];
        let count: u64 = msg_send![items, count];
        for i in 0..count {
            let item: id = msg_send![items, objectAtIndex: i];
            if item != nil {
                let item_identifier: id = msg_send![item, itemIdentifier];
                if ns_string_equals(item_identifier, identifier) {
                    return true;
                }
            }
        }
        false
    }
}

unsafe fn ensure_sidebar_toggle_items(toolbar: id) {
    unsafe {
        if toolbar == nil {
            return;
        }

        let can_insert: bool =
            msg_send![toolbar, respondsToSelector: sel!(insertItemWithItemIdentifier:atIndex:)];
        if !can_insert {
            return;
        }

        let flexible = toolbar_flexible_space_identifier();
        let toggle = toolbar_toggle_sidebar_identifier();
        let separator = toolbar_sidebar_tracking_separator_identifier();

        if !toolbar_has_identifier(toolbar, flexible) {
            let _: () = msg_send![toolbar, insertItemWithItemIdentifier: flexible atIndex: 0u64];
        }
        if !toolbar_has_identifier(toolbar, toggle) {
            let index: u64 = if toolbar_has_identifier(toolbar, flexible) {
                1
            } else {
                0
            };
            let _: () = msg_send![toolbar, insertItemWithItemIdentifier: toggle atIndex: index];
        }
        if !toolbar_has_identifier(toolbar, separator) {
            let items: id = msg_send![toolbar, items];
            let count: u64 = if items != nil {
                msg_send![items, count]
            } else {
                0
            };
            let insert_index = count.min(2);
            let _: () = msg_send![
                toolbar,
                insertItemWithItemIdentifier: separator
                atIndex: insert_index
            ];
        }

        let _: () = msg_send![toolbar, validateVisibleItems];
    }
}

unsafe fn create_sidebar_toolbar() -> id {
    unsafe {
        use super::super::ns_string;

        let toolbar: id = msg_send![class!(NSToolbar), alloc];
        let toolbar: id = msg_send![toolbar, initWithIdentifier: ns_string("GPUINativeSidebarToolbar")];
        let _: () = msg_send![toolbar, setAllowsUserCustomization: 0i8];
        let _: () = msg_send![toolbar, setAutosavesConfiguration: 0i8];
        // NSToolbarDisplayModeIconOnly
        let _: () = msg_send![toolbar, setDisplayMode: 2u64];

        toolbar
    }
}

unsafe fn configure_sidebar_item(sidebar_item: id, min_width: f64, max_width: f64) {
    unsafe {
        let _: () = msg_send![sidebar_item, setCanCollapse: 1i8];
        let _: () = msg_send![sidebar_item, setSpringLoaded: 1i8];
        let _: () = msg_send![sidebar_item, setMinimumThickness: min_width];
        let _: () = msg_send![sidebar_item, setMaximumThickness: max_width];
        // Mirrors Obsidian and AppKit examples: keep window size fixed, resize siblings.
        let _: () = msg_send![sidebar_item, setCollapseBehavior: 2i64];

        let supports_full_height: bool =
            msg_send![sidebar_item, respondsToSelector: sel!(setAllowsFullHeightLayout:)];
        if supports_full_height {
            let _: () = msg_send![sidebar_item, setAllowsFullHeightLayout: 1i8];
        }

        let supports_separator_style: bool =
            msg_send![sidebar_item, respondsToSelector: sel!(setTitlebarSeparatorStyle:)];
        if supports_separator_style {
            let _: () = msg_send![sidebar_item, setTitlebarSeparatorStyle: 0i64];
        }
    }
}

unsafe fn configure_content_item(content_item: id) {
    unsafe {
        let _: () = msg_send![content_item, setCanCollapse: 0i8];
        let supports_full_height: bool =
            msg_send![content_item, respondsToSelector: sel!(setAllowsFullHeightLayout:)];
        if supports_full_height {
            let _: () = msg_send![content_item, setAllowsFullHeightLayout: 1i8];
        }
    }
}

extern "C" fn number_of_rows(this: &Object, _sel: Sel, _table: id) -> i64 {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if ptr.is_null() {
            return 0;
        }
        let callbacks = &*(ptr as *const SidebarCallbacks);
        callbacks.items.len() as i64
    }
}

extern "C" fn object_value_for_row(
    this: &Object,
    _sel: Sel,
    _table: id,
    _column: id,
    row: i64,
) -> id {
    unsafe {
        use super::super::ns_string;

        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if ptr.is_null() {
            return ns_string("");
        }
        let callbacks = &*(ptr as *const SidebarCallbacks);
        if row < 0 || (row as usize) >= callbacks.items.len() {
            return ns_string("");
        }

        ns_string(&callbacks.items[row as usize])
    }
}

extern "C" fn selection_did_change(this: &Object, _sel: Sel, notification: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if ptr.is_null() {
            return;
        }
        let callbacks = &*(ptr as *const SidebarCallbacks);

        let table: id = msg_send![notification, object];
        let row: i64 = msg_send![table, selectedRow];
        if row < 0 || (row as usize) >= callbacks.items.len() {
            return;
        }

        let title = callbacks.items[row as usize].clone();
        set_detail_label_text(callbacks.detail_label, &title);

        if let Some(ref on_select) = callbacks.on_select {
            on_select((row as usize, title));
        }
    }
}

extern "C" fn host_view_perform_key_equivalent(this: &Object, _sel: Sel, event: id) -> i8 {
    unsafe {
        if event != nil {
            let raw_modifiers: u64 = msg_send![event, modifierFlags];
            let modifiers = NSEventModifierFlags::from_bits_truncate(raw_modifiers);
            let is_sidebar_shortcut = modifiers.contains(NSEventModifierFlags::NSCommandKeyMask)
                && modifiers.contains(NSEventModifierFlags::NSAlternateKeyMask)
                && !modifiers.contains(NSEventModifierFlags::NSControlKeyMask)
                && !modifiers.contains(NSEventModifierFlags::NSFunctionKeyMask);

            if is_sidebar_shortcut {
                let key_code: u16 = msg_send![event, keyCode];
                if key_code == 1 {
                    // Hardware keycode 1 maps to the physical "S" key across layouts.
                    let window: id = msg_send![this, window];
                    let sender = if window != nil {
                        window
                    } else {
                        this as *const Object as id
                    };
                    let app: id = msg_send![class!(NSApplication), sharedApplication];
                    let handled: i8 =
                        msg_send![app, sendAction: sel!(toggleSidebar:) to: nil from: sender];
                    if handled != 0 {
                        return 1;
                    }
                }
            }
        }

        msg_send![super(this, class!(NSView)), performKeyEquivalent: event]
    }
}

pub(crate) unsafe fn create_native_sidebar_view(
    sidebar_width: f64,
    min_width: f64,
    max_width: f64,
) -> id {
    unsafe {
        use super::super::ns_string;

        let (min_width, max_width) = clamp_min_max(min_width, max_width);
        let initial_width = sidebar_width.max(min_width).min(max_width);

        let host_view: id = msg_send![SIDEBAR_HOST_VIEW_CLASS, alloc];
        let host_view: id = msg_send![host_view, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(760.0, 420.0),
        )];
        let _: () =
            msg_send![host_view, setAutoresizingMask: NSViewWidthSizable | NSViewHeightSizable];

        let split_view_controller: id = msg_send![class!(NSSplitViewController), alloc];
        let split_view_controller: id = msg_send![split_view_controller, init];
        let split_view: id = msg_send![split_view_controller, splitView];
        let _: () = msg_send![split_view, setVertical: 1i8];
        let _: () = msg_send![split_view, setDividerStyle: 1u64];

        let sidebar_container: id = msg_send![class!(NSVisualEffectView), alloc];
        let sidebar_container: id = msg_send![sidebar_container, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(initial_width, 420.0),
        )];
        NSVisualEffectView::setMaterial_(sidebar_container, NSVisualEffectMaterial::Sidebar);
        NSVisualEffectView::setBlendingMode_(
            sidebar_container,
            NSVisualEffectBlendingMode::BehindWindow,
        );
        NSVisualEffectView::setState_(
            sidebar_container,
            NSVisualEffectState::FollowsWindowActiveState,
        );
        let _: () = msg_send![sidebar_container, setAutoresizingMask: NSViewWidthSizable | NSViewHeightSizable];

        let scroll: id = msg_send![class!(NSScrollView), alloc];
        let scroll: id = msg_send![scroll, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(initial_width, 420.0),
        )];
        let _: () = msg_send![scroll, setHasVerticalScroller: 1i8];
        let _: () = msg_send![scroll, setHasHorizontalScroller: 0i8];
        let _: () = msg_send![scroll, setAutohidesScrollers: 1i8];
        // NSScrollElasticityNone
        let _: () = msg_send![scroll, setHorizontalScrollElasticity: 2i64];
        let _: () = msg_send![scroll, setBorderType: 0u64];
        let _: () = msg_send![scroll, setDrawsBackground: 0i8];
        let clip_view: id = msg_send![scroll, contentView];
        if clip_view != nil {
            let _: () = msg_send![clip_view, setDrawsBackground: 0i8];
        }
        let _: () =
            msg_send![scroll, setAutoresizingMask: NSViewWidthSizable | NSViewHeightSizable];

        let table: id = msg_send![class!(NSTableView), alloc];
        let table: id = msg_send![table, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(initial_width, 420.0),
        )];
        let clear_color: id = msg_send![class!(NSColor), clearColor];
        let _: () = msg_send![table, setBackgroundColor: clear_color];
        let _: () = msg_send![table, setUsesAlternatingRowBackgroundColors: 0i8];
        let _: () = msg_send![table, setAllowsMultipleSelection: 0i8];
        let _: () = msg_send![table, setAllowsColumnSelection: 0i8];
        let _: () = msg_send![table, setAllowsColumnReordering: 0i8];
        let _: () = msg_send![table, setAllowsColumnResizing: 0i8];
        let _: () = msg_send![table, setIntercellSpacing: NSSize::new(0.0, 2.0)];
        // NSTableViewFirstColumnOnlyAutoresizingStyle
        let _: () = msg_send![table, setColumnAutoresizingStyle: 5u64];
        let _: () = msg_send![table, setHeaderView: nil];
        // NSFocusRingTypeNone
        let _: () = msg_send![table, setFocusRingType: 1i64];
        // NSTableViewStyleSourceList
        let _: () = msg_send![table, setStyle: 3i64];
        let _: () = msg_send![table, setAutoresizingMask: NSViewWidthSizable | NSViewHeightSizable];

        let column: id = msg_send![class!(NSTableColumn), alloc];
        let column: id = msg_send![column, initWithIdentifier: ns_string("sidebar-item")];
        let _: () = msg_send![column, setWidth: initial_width];
        // NSTableColumnAutoresizingMask
        let _: () = msg_send![column, setResizingMask: 1u64];
        let _: () = msg_send![column, setEditable: 0i8];
        let _: () = msg_send![table, addTableColumn: column];
        let _: () = msg_send![column, release];

        let _: () = msg_send![scroll, setDocumentView: table];
        let _: () = msg_send![table, release];
        let _: () = msg_send![sidebar_container, addSubview: scroll];
        let _: () = msg_send![scroll, release];

        let content_view: id = msg_send![class!(NSView), alloc];
        let content_view: id = msg_send![content_view, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(520.0, 420.0),
        )];
        let _: () =
            msg_send![content_view, setAutoresizingMask: NSViewWidthSizable | NSViewHeightSizable];

        let detail_label: id =
            msg_send![class!(NSTextField), labelWithString: ns_string("Select an item")];
        let _: () = msg_send![detail_label, setFrame: NSRect::new(
            NSPoint::new(20.0, 360.0),
            NSSize::new(480.0, 24.0),
        )];
        let _: () = msg_send![detail_label, setAutoresizingMask: NSViewWidthSizable];
        let _: () = msg_send![content_view, addSubview: detail_label];

        let sidebar_vc: id = msg_send![class!(NSViewController), alloc];
        let sidebar_vc: id = msg_send![sidebar_vc, init];
        let _: () = msg_send![sidebar_vc, setView: sidebar_container];

        let content_vc: id = msg_send![class!(NSViewController), alloc];
        let content_vc: id = msg_send![content_vc, init];
        let _: () = msg_send![content_vc, setView: content_view];

        let sidebar_item: id =
            msg_send![class!(NSSplitViewItem), sidebarWithViewController: sidebar_vc];
        configure_sidebar_item(sidebar_item, min_width, max_width);

        let content_item: id =
            msg_send![class!(NSSplitViewItem), splitViewItemWithViewController: content_vc];
        configure_content_item(content_item);

        let _: () = msg_send![split_view_controller, addSplitViewItem: sidebar_item];
        let _: () = msg_send![split_view_controller, addSplitViewItem: content_item];

        let split_controller_view: id = msg_send![split_view_controller, view];
        let _: () = msg_send![split_controller_view, setFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(760.0, 420.0),
        )];
        let _: () = msg_send![split_controller_view, setAutoresizingMask: NSViewWidthSizable | NSViewHeightSizable];
        let _: () = msg_send![host_view, addSubview: split_controller_view];

        let host_data = SidebarHostData {
            split_view_controller,
            split_view,
            sidebar_item,
            scroll_view: scroll,
            table_view: table,
            detail_label,
            window: nil,
            embedded_content_view: nil,
            previous_content_view_controller: nil,
            previous_toolbar: nil,
            sidebar_toolbar: nil,
            previous_content_min_size: NSSize::new(0.0, 0.0),
            previous_content_max_size: NSSize::new(0.0, 0.0),
            min_width,
            max_width,
        };
        let host_data_ptr = Box::into_raw(Box::new(host_data)) as *mut c_void;
        (*(host_view as *mut Object)).set_ivar::<*mut c_void>(HOST_DATA_IVAR, host_data_ptr);

        let _: () = msg_send![sidebar_vc, release];
        let _: () = msg_send![content_vc, release];
        let _: () = msg_send![sidebar_container, release];
        let _: () = msg_send![content_view, release];

        set_native_sidebar_width(host_view, initial_width, min_width, max_width);
        host_view
    }
}

pub(crate) unsafe fn configure_native_sidebar_window(host_view: id, parent_view: id) {
    unsafe {
        if host_view == nil || parent_view == nil {
            return;
        }

        let Some(host_data) = host_data_mut(host_view) else {
            return;
        };

        let window: id = msg_send![parent_view, window];
        if window == nil {
            return;
        }

        if host_data.window != window {
            if host_data.window != nil {
                let current_toolbar: id = msg_send![host_data.window, toolbar];
                if host_data.sidebar_toolbar != nil && current_toolbar == host_data.sidebar_toolbar {
                    let _: () = msg_send![host_data.window, setToolbar: host_data.previous_toolbar];
                }
            }

            if host_data.sidebar_toolbar != nil {
                let _: () = msg_send![host_data.sidebar_toolbar, release];
                host_data.sidebar_toolbar = nil;
            }
            if host_data.embedded_content_view != nil {
                let current_superview: id = msg_send![host_data.embedded_content_view, superview];
                if current_superview != nil {
                    let _: () = msg_send![host_data.embedded_content_view, removeFromSuperview];
                }
                let _: () = msg_send![host_data.embedded_content_view, release];
                host_data.embedded_content_view = nil;
            }
            if host_data.previous_toolbar != nil {
                let _: () = msg_send![host_data.previous_toolbar, release];
                host_data.previous_toolbar = nil;
            }
            if host_data.previous_content_view_controller != nil {
                let _: () = msg_send![host_data.previous_content_view_controller, release];
                host_data.previous_content_view_controller = nil;
            }

            host_data.window = window;
            host_data.previous_content_min_size = msg_send![window, contentMinSize];
            host_data.previous_content_max_size = msg_send![window, contentMaxSize];

            let previous_content_view_controller: id = msg_send![window, contentViewController];
            if previous_content_view_controller != nil
                && previous_content_view_controller != host_data.split_view_controller
            {
                let _: () = msg_send![previous_content_view_controller, retain];
                host_data.previous_content_view_controller = previous_content_view_controller;
            }

            let previous_toolbar: id = msg_send![window, toolbar];
            if previous_toolbar != nil {
                let _: () = msg_send![previous_toolbar, retain];
                host_data.previous_toolbar = previous_toolbar;
            }

            let toolbar = create_sidebar_toolbar();
            host_data.sidebar_toolbar = toolbar;

            let style_mask: NSWindowStyleMask = msg_send![window, styleMask];
            if !style_mask.contains(NSWindowStyleMask::NSFullSizeContentViewWindowMask) {
                let _: () = msg_send![
                    window,
                    setStyleMask: style_mask | NSWindowStyleMask::NSFullSizeContentViewWindowMask
                ];
            }
            let _: () = msg_send![window, setTitleVisibility: NSWindowTitleVisibility::NSWindowTitleHidden];
            let _: () = msg_send![window, setTitlebarAppearsTransparent: 1i8];

            let supports_toolbar_style: bool =
                msg_send![window, respondsToSelector: sel!(setToolbarStyle:)];
            if supports_toolbar_style {
                // NSWindowToolbarStyleUnified
                let _: () = msg_send![window, setToolbarStyle: 3i64];
            }

            let supports_separator_style: bool =
                msg_send![window, respondsToSelector: sel!(setTitlebarSeparatorStyle:)];
            if supports_separator_style {
                // NSTitlebarSeparatorStyleAutomatic
                let _: () = msg_send![window, setTitlebarSeparatorStyle: 0i64];
            }

            let window_bg: id = msg_send![class!(NSColor), windowBackgroundColor];
            if window_bg != nil {
                let _: () = msg_send![window, setBackgroundColor: window_bg];
            }
        }

        if host_data.embedded_content_view != parent_view {
            if host_data.embedded_content_view != nil {
                let current_superview: id = msg_send![host_data.embedded_content_view, superview];
                if current_superview != nil {
                    let _: () = msg_send![host_data.embedded_content_view, removeFromSuperview];
                }
                let _: () = msg_send![host_data.embedded_content_view, release];
                host_data.embedded_content_view = nil;
            }
            let _: () = msg_send![parent_view, retain];
            host_data.embedded_content_view = parent_view;
        }

        let content_size: NSSize = {
            let content_view: id = msg_send![window, contentView];
            if content_view != nil {
                let frame: NSRect = msg_send![content_view, frame];
                frame.size
            } else {
                NSSize::new(760.0, 420.0)
            }
        };

        let current_content_view_controller: id = msg_send![window, contentViewController];
        if current_content_view_controller != host_data.split_view_controller {
            let _: () = msg_send![window, setContentViewController: host_data.split_view_controller];
            let _: () = msg_send![window, setContentSize: content_size];
            let _: () = msg_send![window, setContentMinSize: host_data.previous_content_min_size];
            let _: () = msg_send![window, setContentMaxSize: host_data.previous_content_max_size];
            let _: () = msg_send![host_data.split_view, adjustSubviews];
            let split_view_controller_view: id = msg_send![host_data.split_view_controller, view];
            let _: () = msg_send![split_view_controller_view, layoutSubtreeIfNeeded];
        }

        sync_sidebar_table_width(host_data);

        if host_data.sidebar_toolbar != nil {
            let active_toolbar: id = msg_send![window, toolbar];
            if active_toolbar != host_data.sidebar_toolbar {
                let _: () = msg_send![window, setToolbar: host_data.sidebar_toolbar];
            }
            ensure_sidebar_toggle_items(host_data.sidebar_toolbar);
        }
    }
}

pub(crate) unsafe fn set_native_sidebar_width(
    host_view: id,
    sidebar_width: f64,
    min_width: f64,
    max_width: f64,
) {
    unsafe {
        let Some(host_data) = host_data_mut(host_view) else {
            return;
        };

        let (min_width, max_width) = clamp_min_max(min_width, max_width);
        host_data.min_width = min_width;
        host_data.max_width = max_width;

        let _: () = msg_send![host_data.sidebar_item, setMinimumThickness: min_width];
        let _: () = msg_send![host_data.sidebar_item, setMaximumThickness: max_width];

        let width =
            clamped_sidebar_width(host_data.split_view, sidebar_width, min_width, max_width);
        let _: () = msg_send![host_data.split_view, setPosition: width ofDividerAtIndex: 0i64];
        let _: () = msg_send![host_data.split_view, adjustSubviews];
        sync_sidebar_table_width(host_data);
    }
}

pub(crate) unsafe fn set_native_sidebar_collapsed(
    host_view: id,
    collapsed: bool,
    expanded_width: f64,
    min_width: f64,
    max_width: f64,
) {
    unsafe {
        let Some(host_data) = host_data_mut(host_view) else {
            return;
        };
        let _: () = msg_send![host_data.sidebar_item, setCollapsed: collapsed as i8];

        if !collapsed {
            set_native_sidebar_width(host_view, expanded_width, min_width, max_width);
        }
    }
}

pub(crate) unsafe fn set_native_sidebar_items(
    host_view: id,
    items: &[&str],
    selected_index: Option<usize>,
    min_width: f64,
    max_width: f64,
    on_select: Option<Box<dyn Fn((usize, String))>>,
) -> *mut c_void {
    unsafe {
        let Some(host_data) = host_data_mut(host_view) else {
            return ptr::null_mut();
        };

        let (min_width, max_width) = clamp_min_max(min_width, max_width);
        host_data.min_width = min_width;
        host_data.max_width = max_width;
        let _: () = msg_send![host_data.sidebar_item, setMinimumThickness: min_width];
        let _: () = msg_send![host_data.sidebar_item, setMaximumThickness: max_width];

        let delegate: id = msg_send![SIDEBAR_DELEGATE_CLASS, alloc];
        let delegate: id = msg_send![delegate, init];

        let callbacks = SidebarCallbacks {
            items: items.iter().map(|item| item.to_string()).collect(),
            on_select,
            table_view: host_data.table_view,
            detail_label: host_data.detail_label,
        };
        let callbacks_ptr = Box::into_raw(Box::new(callbacks)) as *mut c_void;
        (*delegate).set_ivar::<*mut c_void>(CALLBACK_IVAR, callbacks_ptr);

        let _: () = msg_send![host_data.table_view, setDataSource: delegate];
        let _: () = msg_send![host_data.table_view, setDelegate: delegate];
        let _: () = msg_send![host_data.table_view, reloadData];
        sync_sidebar_table_width(host_data);

        let row_count: i64 = msg_send![host_data.table_view, numberOfRows];
        if row_count > 0 {
            if let Some(index) = selected_index {
                let clamped = (index as i64).min(row_count - 1).max(0);
                let index_set: id =
                    msg_send![class!(NSIndexSet), indexSetWithIndex: clamped as u64];
                let _: () = msg_send![host_data.table_view, selectRowIndexes: index_set byExtendingSelection: 0i8];
                set_detail_label_text(host_data.detail_label, items[clamped as usize]);
            } else {
                let _: () = msg_send![host_data.table_view, deselectAll: nil];
                set_detail_label_text(host_data.detail_label, "Select an item");
            }
        } else {
            set_detail_label_text(host_data.detail_label, "No items");
        }

        delegate as *mut c_void
    }
}

pub(crate) unsafe fn release_native_sidebar_target(target: *mut c_void) {
    unsafe {
        if target.is_null() {
            return;
        }

        let delegate = target as id;
        let callbacks_ptr: *mut c_void = *(*delegate).get_ivar(CALLBACK_IVAR);
        if !callbacks_ptr.is_null() {
            let callbacks = Box::from_raw(callbacks_ptr as *mut SidebarCallbacks);
            if callbacks.table_view != nil {
                let _: () = msg_send![callbacks.table_view, setDataSource: nil];
                let _: () = msg_send![callbacks.table_view, setDelegate: nil];
            }
        }
        let _: () = msg_send![delegate, release];
    }
}

pub(crate) unsafe fn release_native_sidebar_view(host_view: id) {
    unsafe {
        if host_view == nil {
            return;
        }

        let host_data_ptr = host_data_ptr(host_view);
        if !host_data_ptr.is_null() {
            let host_data = Box::from_raw(host_data_ptr);
            if host_data.window != nil {
                let _: () = msg_send![
                    host_data.window,
                    setContentMinSize: host_data.previous_content_min_size
                ];
                let _: () = msg_send![
                    host_data.window,
                    setContentMaxSize: host_data.previous_content_max_size
                ];
                let current_content_view_controller: id =
                    msg_send![host_data.window, contentViewController];
                if current_content_view_controller == host_data.split_view_controller {
                    let _: () = msg_send![
                        host_data.window,
                        setContentViewController: host_data.previous_content_view_controller
                    ];
                }
                let toolbar: id = msg_send![host_data.window, toolbar];
                if host_data.sidebar_toolbar != nil && toolbar == host_data.sidebar_toolbar {
                    let _: () = msg_send![host_data.window, setToolbar: host_data.previous_toolbar];
                }
            }
            if host_data.sidebar_toolbar != nil {
                let _: () = msg_send![host_data.sidebar_toolbar, release];
            }
            if host_data.previous_toolbar != nil {
                let _: () = msg_send![host_data.previous_toolbar, release];
            }
            if host_data.previous_content_view_controller != nil {
                let _: () = msg_send![host_data.previous_content_view_controller, release];
            }
            if host_data.embedded_content_view != nil {
                let current_superview: id = msg_send![host_data.embedded_content_view, superview];
                if current_superview != nil {
                    let _: () = msg_send![host_data.embedded_content_view, removeFromSuperview];
                }
                let _: () = msg_send![host_data.embedded_content_view, release];
            }
            if host_data.table_view != nil {
                let _: () = msg_send![host_data.table_view, setDataSource: nil];
                let _: () = msg_send![host_data.table_view, setDelegate: nil];
            }

            let _: () = msg_send![host_data.split_view_controller, release];
        }

        let object = host_view as *mut Object;
        (*object).set_ivar::<*mut c_void>(HOST_DATA_IVAR, ptr::null_mut());
        let _: () = msg_send![host_view, release];
    }
}
