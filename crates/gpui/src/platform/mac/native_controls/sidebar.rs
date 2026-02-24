use super::CALLBACK_IVAR;
use crate::{dispatch_get_main_queue, dispatch_sys::dispatch_async_f};
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
    detail_content_view: id,
    sidebar_container: id,
    window: id,
    embedded_content_view: id,
    previous_content_view_controller: id,
    previous_toolbar: id,
    sidebar_toolbar: id,
    previous_content_min_size: NSSize,
    previous_content_max_size: NSSize,
    min_width: f64,
    max_width: f64,
    embed_in_sidebar: bool,
    header_view: id,
    header_button_targets: Vec<id>,
    header_on_click: Option<Box<dyn Fn(usize)>>,
    scroll_view_top_constraint: id,
    scroll_view_using_autolayout: bool,
}

struct SidebarCallbacks {
    items: Vec<String>,
    on_select: Option<Box<dyn Fn((usize, String))>>,
    table_view: id,
    detail_label: id,
}

static mut SIDEBAR_HOST_VIEW_CLASS: *const Class = ptr::null();
static mut SIDEBAR_DELEGATE_CLASS: *const Class = ptr::null();
static mut SIDEBAR_HEADER_BUTTON_CLASS: *const Class = ptr::null();

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

#[ctor]
unsafe fn build_sidebar_header_button_class() {
    unsafe {
        let mut decl =
            ClassDecl::new("GPUINativeSidebarHeaderButton", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>("hostViewPtr");
        decl.add_ivar::<i64>("buttonIndex");
        decl.add_method(
            sel!(headerButtonAction:),
            header_button_action as extern "C" fn(&Object, Sel, id),
        );
        SIDEBAR_HEADER_BUTTON_CLASS = decl.register();
    }
}

extern "C" fn header_button_action(this: &Object, _sel: Sel, _sender: id) {
    unsafe {
        let host_view_ptr: *mut c_void = *this.get_ivar("hostViewPtr");
        let index: i64 = *this.get_ivar("buttonIndex");
        if !host_view_ptr.is_null() {
            if let Some(host_data) = host_data_mut(host_view_ptr as id) {
                if let Some(ref on_click) = host_data.header_on_click {
                    on_click(index as usize);
                }
            }
        }
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
        let toolbar: id =
            msg_send![toolbar, initWithIdentifier: ns_string("GPUINativeSidebarToolbar")];
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
    embed_in_sidebar: bool,
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

        // The sidebar pane's view hierarchy depends on embed_in_sidebar:
        //
        // Non-embed (source list):
        //   sidebar_container (NSVisualEffectView)
        //     └── NSScrollView → NSTableView
        //
        // Embed (GPUI content):
        //   sidebar_container (plain NSView)  ← sidebar VC's view; MTKView target
        //     └── NSVisualEffectView           ← background vibrancy only
        //
        // The MTKView cannot be a direct child of NSVisualEffectView because its
        // special layer compositing breaks Metal rendering (shows black).

        let (sidebar_container, sidebar_visual_effect) = if embed_in_sidebar {
            // Plain NSView wrapper — safe target for the MTKView.
            let wrapper: id = msg_send![class!(NSView), alloc];
            let wrapper: id = msg_send![wrapper, initWithFrame: NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(initial_width, 420.0),
            )];
            let _: () =
                msg_send![wrapper, setAutoresizingMask: NSViewWidthSizable | NSViewHeightSizable];

            // On macOS 26+ (Liquid Glass), the sidebar glass material is applied
            // automatically by NSSplitViewItem. Adding a legacy NSVisualEffectView
            // blocks the glass from showing through, so we skip it.
            let has_liquid_glass = Class::get("NSBackgroundExtensionView").is_some();

            let vfx = if has_liquid_glass {
                nil
            } else {
                // Pre-macOS 26: NSVisualEffectView as a background subview for
                // native vibrancy.
                let vfx: id = msg_send![class!(NSVisualEffectView), alloc];
                let vfx: id = msg_send![vfx, initWithFrame: NSRect::new(
                    NSPoint::new(0.0, 0.0),
                    NSSize::new(initial_width, 420.0),
                )];
                NSVisualEffectView::setMaterial_(vfx, NSVisualEffectMaterial::Sidebar);
                NSVisualEffectView::setBlendingMode_(vfx, NSVisualEffectBlendingMode::BehindWindow);
                NSVisualEffectView::setState_(vfx, NSVisualEffectState::FollowsWindowActiveState);
                let _: () =
                    msg_send![vfx, setAutoresizingMask: NSViewWidthSizable | NSViewHeightSizable];
                let _: () = msg_send![wrapper, addSubview: vfx positioned: -1i64 /* NSWindowBelow */ relativeTo: nil];
                let _: () = msg_send![vfx, release];
                vfx
            };

            (wrapper, vfx)
        } else {
            // Classic source-list mode: NSVisualEffectView is both the sidebar
            // VC's view and the container for the scroll/table.
            let vfx: id = msg_send![class!(NSVisualEffectView), alloc];
            let vfx: id = msg_send![vfx, initWithFrame: NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(initial_width, 420.0),
            )];
            NSVisualEffectView::setMaterial_(vfx, NSVisualEffectMaterial::Sidebar);
            NSVisualEffectView::setBlendingMode_(vfx, NSVisualEffectBlendingMode::BehindWindow);
            NSVisualEffectView::setState_(vfx, NSVisualEffectState::FollowsWindowActiveState);
            let _: () =
                msg_send![vfx, setAutoresizingMask: NSViewWidthSizable | NSViewHeightSizable];
            (vfx, vfx)
        };
        let _ = sidebar_visual_effect; // retained by sidebar_container as subview

        // Only create the source-list table when NOT embedding custom content in the sidebar.
        let (scroll, table) = if !embed_in_sidebar {
            let scroll: id = msg_send![class!(NSScrollView), alloc];
            let scroll: id = msg_send![scroll, initWithFrame: NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(initial_width, 420.0),
            )];
            let _: () = msg_send![scroll, setHasVerticalScroller: 1i8];
            let _: () = msg_send![scroll, setHasHorizontalScroller: 0i8];
            let _: () = msg_send![scroll, setAutohidesScrollers: 1i8];
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
            let _: () = msg_send![table, setColumnAutoresizingStyle: 5u64];
            let _: () = msg_send![table, setHeaderView: nil];
            let _: () = msg_send![table, setFocusRingType: 1i64];
            let _: () = msg_send![table, setStyle: 3i64];
            let _: () =
                msg_send![table, setAutoresizingMask: NSViewWidthSizable | NSViewHeightSizable];

            let column: id = msg_send![class!(NSTableColumn), alloc];
            let column: id = msg_send![column, initWithIdentifier: ns_string("sidebar-item")];
            let _: () = msg_send![column, setWidth: initial_width];
            let _: () = msg_send![column, setResizingMask: 1u64];
            let _: () = msg_send![column, setEditable: 0i8];
            let _: () = msg_send![table, addTableColumn: column];
            let _: () = msg_send![column, release];

            let _: () = msg_send![scroll, setDocumentView: table];
            let _: () = msg_send![table, release];
            let _: () = msg_send![sidebar_container, addSubview: scroll];
            let _: () = msg_send![scroll, release];

            (scroll, table)
        } else {
            // In embed mode, the sidebar container stays empty — GPUI content
            // will be embedded here via configure_native_sidebar_window.
            (nil, nil)
        };

        let content_view: id = msg_send![class!(NSView), alloc];
        let content_view: id = msg_send![content_view, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(520.0, 420.0),
        )];
        let _: () =
            msg_send![content_view, setAutoresizingMask: NSViewWidthSizable | NSViewHeightSizable];

        // On macOS 26+, wrap the content view in NSBackgroundExtensionView so
        // the content appearance extends seamlessly under the floating glass
        // sidebar. Use automaticallyPlacesContentView=NO and pin the content
        // to top/bottom/trailing so the extension only fills the leading
        // (sidebar) edge, not the titlebar area.
        let has_liquid_glass = Class::get("NSBackgroundExtensionView").is_some();
        let content_vc_view = if has_liquid_glass && embed_in_sidebar {
            let bg_ext_cls = Class::get("NSBackgroundExtensionView").expect("checked above");
            let bg_ext: id = msg_send![bg_ext_cls, alloc];
            let bg_ext: id = msg_send![bg_ext, initWithFrame: NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(520.0, 420.0),
            )];
            let _: () =
                msg_send![bg_ext, setAutoresizingMask: NSViewWidthSizable | NSViewHeightSizable];

            // Disable automatic safe-area placement so we can control
            // exactly which edges get the background extension effect.
            let _: () = msg_send![bg_ext, setAutomaticallyPlacesContentView: 0i8];
            let _: () = msg_send![bg_ext, setContentView: content_view];

            // Pin content to top, bottom, trailing — leave leading free so
            // the extension fills only the sidebar overlap area.
            let _: () = msg_send![
                content_view,
                setTranslatesAutoresizingMaskIntoConstraints: 0i8
            ];
            let cv_top: id = msg_send![content_view, topAnchor];
            let cv_bottom: id = msg_send![content_view, bottomAnchor];
            let cv_trailing: id = msg_send![content_view, trailingAnchor];
            let cv_leading: id = msg_send![content_view, leadingAnchor];

            let ext_top: id = msg_send![bg_ext, topAnchor];
            let ext_bottom: id = msg_send![bg_ext, bottomAnchor];
            let ext_trailing: id = msg_send![bg_ext, trailingAnchor];

            // Use safeAreaLayoutGuide for the leading edge so the content
            // starts after the sidebar overlap, leaving room for extension.
            let has_safe_area: bool = msg_send![
                bg_ext,
                respondsToSelector: sel!(safeAreaLayoutGuide)
            ];
            let safe_leading: id = if has_safe_area {
                let guide: id = msg_send![bg_ext, safeAreaLayoutGuide];
                msg_send![guide, leadingAnchor]
            } else {
                msg_send![bg_ext, leadingAnchor]
            };

            let c1: id = msg_send![cv_top, constraintEqualToAnchor: ext_top];
            let c2: id = msg_send![cv_bottom, constraintEqualToAnchor: ext_bottom];
            let c3: id = msg_send![cv_trailing, constraintEqualToAnchor: ext_trailing];
            let c4: id = msg_send![cv_leading, constraintEqualToAnchor: safe_leading];

            let _: () = msg_send![c1, setActive: 1i8];
            let _: () = msg_send![c2, setActive: 1i8];
            let _: () = msg_send![c3, setActive: 1i8];
            let _: () = msg_send![c4, setActive: 1i8];

            bg_ext
        } else {
            content_view
        };

        let detail_label: id = if !embed_in_sidebar {
            let label: id =
                msg_send![class!(NSTextField), labelWithString: ns_string("Select an item")];
            let _: () = msg_send![label, setFrame: NSRect::new(
                NSPoint::new(20.0, 360.0),
                NSSize::new(480.0, 24.0),
            )];
            let _: () = msg_send![label, setAutoresizingMask: NSViewWidthSizable];
            let _: () = msg_send![content_view, addSubview: label];
            label
        } else {
            nil
        };

        let sidebar_vc: id = msg_send![class!(NSViewController), alloc];
        let sidebar_vc: id = msg_send![sidebar_vc, init];
        let _: () = msg_send![sidebar_vc, setView: sidebar_container];

        let content_vc: id = msg_send![class!(NSViewController), alloc];
        let content_vc: id = msg_send![content_vc, init];
        let _: () = msg_send![content_vc, setView: content_vc_view];

        let sidebar_item: id =
            msg_send![class!(NSSplitViewItem), sidebarWithViewController: sidebar_vc];
        configure_sidebar_item(sidebar_item, min_width, max_width);

        let content_item: id =
            msg_send![class!(NSSplitViewItem), splitViewItemWithViewController: content_vc];
        configure_content_item(content_item);

        let _: () = msg_send![split_view_controller, addSplitViewItem: sidebar_item];
        let _: () = msg_send![split_view_controller, addSplitViewItem: content_item];

        // Enable safe area inset propagation so the sidebar overlap is
        // reflected in the content item's safe area. Our manual constraints
        // above only use the safe area for the leading edge, so this won't
        // cause a titlebar extension.
        if has_liquid_glass && embed_in_sidebar {
            let responds: bool = msg_send![
                content_item,
                respondsToSelector: sel!(setAutomaticallyAdjustsSafeAreaInsets:)
            ];
            if responds {
                let _: () = msg_send![content_item, setAutomaticallyAdjustsSafeAreaInsets: 1i8];
            }
        }

        let split_controller_view: id = msg_send![split_view_controller, view];
        let _: () = msg_send![split_controller_view, setFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(760.0, 420.0),
        )];
        let _: () = msg_send![split_controller_view, setAutoresizingMask: NSViewWidthSizable | NSViewHeightSizable];
        let _: () = msg_send![host_view, addSubview: split_controller_view];

        // Retain sidebar_container so we can embed GPUI content into it later.
        let _: () = msg_send![sidebar_container, retain];

        let host_data = SidebarHostData {
            split_view_controller,
            split_view,
            sidebar_item,
            scroll_view: scroll,
            table_view: table,
            detail_label,
            detail_content_view: content_view,
            sidebar_container,
            window: nil,
            embedded_content_view: nil,
            previous_content_view_controller: nil,
            previous_toolbar: nil,
            sidebar_toolbar: nil,
            previous_content_min_size: NSSize::new(0.0, 0.0),
            previous_content_max_size: NSSize::new(0.0, 0.0),
            min_width,
            max_width,
            embed_in_sidebar,
            header_view: nil,
            header_button_targets: Vec::new(),
            header_on_click: None,
            scroll_view_top_constraint: nil,
            scroll_view_using_autolayout: false,
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

pub(crate) unsafe fn configure_native_sidebar_window(
    host_view: id,
    parent_view: id,
    embed_in_sidebar: bool,
    manage_window_chrome: bool,
    manage_toolbar: bool,
) {
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
                if host_data.sidebar_toolbar != nil && current_toolbar == host_data.sidebar_toolbar
                {
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
                // Re-add to the old window's content view so it stays alive
                if host_data.window != nil {
                    let old_content_view: id = msg_send![host_data.window, contentView];
                    if old_content_view != nil {
                        let _: () = msg_send![
                            host_data.embedded_content_view,
                            setTranslatesAutoresizingMaskIntoConstraints: 1i8
                        ];
                        let _: () = msg_send![
                            host_data.embedded_content_view,
                            setAutoresizingMask: NSViewWidthSizable | NSViewHeightSizable
                        ];
                        let _: () = msg_send![
                            old_content_view,
                            addSubview: host_data.embedded_content_view
                        ];
                        // Defer resize (same reason as release_native_sidebar_view)
                        let _: () = msg_send![host_data.embedded_content_view, retain];
                        dispatch_async_f(
                            dispatch_get_main_queue(),
                            host_data.embedded_content_view as *mut c_void,
                            Some(deferred_resize_embedded_view),
                        );
                    }
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

        if manage_window_chrome {
            let current_content_view_controller: id = msg_send![window, contentViewController];
            if current_content_view_controller != host_data.split_view_controller {
                // Save window styling set by GPUI (set_background_appearance)
                // before setContentViewController: which may reset them.
                let saved_opaque: i8 = msg_send![window, isOpaque];
                let saved_bg_color: id = msg_send![window, backgroundColor];
                if saved_bg_color != nil {
                    let _: () = msg_send![saved_bg_color, retain];
                }

                let _: () =
                    msg_send![window, setContentViewController: host_data.split_view_controller];

                // Restore window styling.
                let _: () = msg_send![window, setOpaque: saved_opaque];
                if saved_bg_color != nil {
                    let _: () = msg_send![window, setBackgroundColor: saved_bg_color];
                    let _: () = msg_send![saved_bg_color, release];
                }

                let _: () = msg_send![window, setContentSize: content_size];
                let _: () =
                    msg_send![window, setContentMinSize: host_data.previous_content_min_size];
                let _: () =
                    msg_send![window, setContentMaxSize: host_data.previous_content_max_size];
                let _: () = msg_send![host_data.split_view, adjustSubviews];
                let split_view_controller_view: id =
                    msg_send![host_data.split_view_controller, view];
                let _: () = msg_send![split_view_controller_view, layoutSubtreeIfNeeded];
            }

            // When the host app manages its own toolbar, the content (detail)
            // pane should NOT extend behind the titlebar — only the sidebar
            // needs full-height layout. This prevents NSBackgroundExtensionView
            // from mirroring content under the titlebar.
            if !manage_toolbar {
                let items: id = msg_send![host_data.split_view_controller, splitViewItems];
                if items != nil {
                    let count: u64 = msg_send![items, count];
                    if count >= 2 {
                        let content_item: id = msg_send![items, objectAtIndex: 1u64];
                        let supports_full_height: bool = msg_send![
                            content_item,
                            respondsToSelector: sel!(setAllowsFullHeightLayout:)
                        ];
                        if supports_full_height {
                            let _: () =
                                msg_send![content_item, setAllowsFullHeightLayout: 0i8];
                        }
                    }
                }
            }
        } else {
            let window_content_view: id = msg_send![window, contentView];
            if window_content_view != nil {
                let host_superview: id = msg_send![host_view, superview];
                if host_superview != window_content_view {
                    if host_superview != nil {
                        let _: () = msg_send![host_view, removeFromSuperview];
                    }
                    let parent_bounds: NSRect = msg_send![window_content_view, bounds];
                    let _: () = msg_send![host_view, setFrame: parent_bounds];
                    let _: () = msg_send![
                        host_view,
                        setAutoresizingMask: NSViewWidthSizable | NSViewHeightSizable
                    ];
                    let _: () = msg_send![window_content_view, addSubview: host_view];
                }
            }

        }

        let host_attached = if manage_window_chrome {
            true
        } else {
            let host_superview: id = msg_send![host_view, superview];
            host_superview != nil
        };

        // Embed the GPUI content view into the appropriate pane.
        if host_data.embedded_content_view != nil && host_attached {
            let target_pane = if embed_in_sidebar {
                // Embed in the sidebar (left) pane — the NSVisualEffectView container.
                host_data.sidebar_container
            } else {
                // Embed in the detail (right) pane.
                host_data.detail_content_view
            };

            if target_pane != nil {
                let current_superview: id = msg_send![host_data.embedded_content_view, superview];
                if current_superview != target_pane {
                    if current_superview != nil {
                        let _: () = msg_send![host_data.embedded_content_view, removeFromSuperview];
                    }

                    if embed_in_sidebar {
                        // Use Auto Layout to pin below the titlebar safe area.
                        let _: () = msg_send![
                            host_data.embedded_content_view,
                            setTranslatesAutoresizingMaskIntoConstraints: 0i8
                        ];
                        let _: () = msg_send![
                            target_pane,
                            addSubview: host_data.embedded_content_view
                        ];

                        let has_safe_area: bool = msg_send![
                            target_pane,
                            respondsToSelector: sel!(safeAreaLayoutGuide)
                        ];
                        let guide_top: id = if has_safe_area {
                            let guide: id = msg_send![target_pane, safeAreaLayoutGuide];
                            msg_send![guide, topAnchor]
                        } else {
                            msg_send![target_pane, topAnchor]
                        };

                        let pane_leading: id = msg_send![target_pane, leadingAnchor];
                        let pane_trailing: id = msg_send![target_pane, trailingAnchor];
                        let pane_bottom: id = msg_send![target_pane, bottomAnchor];

                        let view_top: id = msg_send![host_data.embedded_content_view, topAnchor];
                        let view_leading: id =
                            msg_send![host_data.embedded_content_view, leadingAnchor];
                        let view_trailing: id =
                            msg_send![host_data.embedded_content_view, trailingAnchor];
                        let view_bottom: id =
                            msg_send![host_data.embedded_content_view, bottomAnchor];

                        let c1: id = msg_send![view_top, constraintEqualToAnchor: guide_top];
                        let c2: id = msg_send![view_leading, constraintEqualToAnchor: pane_leading];
                        let c3: id =
                            msg_send![view_trailing, constraintEqualToAnchor: pane_trailing];
                        let c4: id = msg_send![view_bottom, constraintEqualToAnchor: pane_bottom];

                        let _: () = msg_send![c1, setActive: 1i8];
                        let _: () = msg_send![c2, setActive: 1i8];
                        let _: () = msg_send![c3, setActive: 1i8];
                        let _: () = msg_send![c4, setActive: 1i8];

                        let _: () = msg_send![target_pane, layoutSubtreeIfNeeded];

                        // Make the Metal layer non-opaque so the
                        // NSVisualEffectView vibrancy shows through
                        // transparent areas of the GPUI content.
                        let layer: id = msg_send![host_data.embedded_content_view, layer];
                        if layer != nil {
                            let _: () = msg_send![layer, setOpaque: 0i8];
                        }
                    } else {
                        let pane_bounds: NSRect = msg_send![target_pane, bounds];
                        let _: () = msg_send![
                            host_data.embedded_content_view, setFrame: pane_bounds
                        ];
                        let _: () = msg_send![
                            host_data.embedded_content_view,
                            setAutoresizingMask: NSViewWidthSizable | NSViewHeightSizable
                        ];
                        let _: () = msg_send![
                            target_pane,
                            addSubview: host_data.embedded_content_view
                        ];
                    }

                    // Restore first responder after reparenting so the view
                    // continues to receive keyboard events.
                    let _: () = msg_send![window, makeFirstResponder: host_data.embedded_content_view];
                }
            }

            // Hide the placeholder label when content is embedded in the detail pane.
            if !embed_in_sidebar && host_data.detail_label != nil {
                let _: () = msg_send![host_data.detail_label, setHidden: 1i8];
            }
        }

        if !embed_in_sidebar {
            sync_sidebar_table_width(host_data);
        }

        if manage_window_chrome && manage_toolbar {
            if host_data.sidebar_toolbar == nil {
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
                    let _: () = msg_send![window, setToolbarStyle: 3i64];
                }

                let supports_separator_style: bool =
                    msg_send![window, respondsToSelector: sel!(setTitlebarSeparatorStyle:)];
                if supports_separator_style {
                    let _: () = msg_send![window, setTitlebarSeparatorStyle: 0i64];
                }

                let window_bg: id = msg_send![class!(NSColor), windowBackgroundColor];
                if window_bg != nil {
                    let _: () = msg_send![window, setBackgroundColor: window_bg];
                }
            }

            if host_data.sidebar_toolbar != nil {
                let active_toolbar: id = msg_send![window, toolbar];
                if active_toolbar != host_data.sidebar_toolbar {
                    let _: () = msg_send![window, setToolbar: host_data.sidebar_toolbar];
                }
                ensure_sidebar_toggle_items(host_data.sidebar_toolbar);
            }
        } else if host_data.sidebar_toolbar != nil {
            let active_toolbar: id = msg_send![window, toolbar];
            if active_toolbar == host_data.sidebar_toolbar {
                let _: () = msg_send![window, setToolbar: host_data.previous_toolbar];
            }
            let _: () = msg_send![host_data.sidebar_toolbar, release];
            host_data.sidebar_toolbar = nil;
            if host_data.previous_toolbar != nil {
                let _: () = msg_send![host_data.previous_toolbar, release];
                host_data.previous_toolbar = nil;
            }
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
        if host_data.window != nil {
            let _: () = msg_send![host_data.split_view, adjustSubviews];
        }
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

/// Updates the stored header click callback without rebuilding the header structure.
pub(crate) unsafe fn update_native_sidebar_header_callback(
    host_view: id,
    callback: Option<Box<dyn Fn(usize)>>,
) {
    unsafe {
        let Some(host_data) = host_data_mut(host_view) else {
            return;
        };
        host_data.header_on_click = callback;
    }
}

/// Builds or rebuilds the native sidebar header with a title label and/or buttons.
/// Only works in source-list mode (when scroll_view is present).
pub(crate) unsafe fn set_native_sidebar_header(
    host_view: id,
    title: Option<&str>,
    button_symbols: &[&str],
) {
    unsafe {
        use super::super::ns_string;

        let Some(host_data) = host_data_mut(host_view) else {
            return;
        };

        if host_data.scroll_view == nil || host_data.sidebar_container == nil {
            return;
        }

        // Release old header button targets
        for target in host_data.header_button_targets.drain(..) {
            let _: () = msg_send![target, release];
        }

        // Remove old header view
        if host_data.header_view != nil {
            let _: () = msg_send![host_data.header_view, removeFromSuperview];
            let _: () = msg_send![host_data.header_view, release];
            host_data.header_view = nil;
        }

        // Deactivate old scroll_view top constraint
        if host_data.scroll_view_top_constraint != nil {
            let _: () = msg_send![host_data.scroll_view_top_constraint, setActive: 0i8];
            let _: () = msg_send![host_data.scroll_view_top_constraint, release];
            host_data.scroll_view_top_constraint = nil;
        }

        let container = host_data.sidebar_container;
        let scroll_view = host_data.scroll_view;
        let has_header = title.is_some() || !button_symbols.is_empty();

        // Switch scroll_view to Auto Layout (one-time setup)
        if !host_data.scroll_view_using_autolayout {
            let _: () =
                msg_send![scroll_view, setTranslatesAutoresizingMaskIntoConstraints: 0i8];

            let scroll_leading: id = msg_send![scroll_view, leadingAnchor];
            let scroll_trailing: id = msg_send![scroll_view, trailingAnchor];
            let scroll_bottom: id = msg_send![scroll_view, bottomAnchor];
            let container_leading: id = msg_send![container, leadingAnchor];
            let container_trailing: id = msg_send![container, trailingAnchor];
            let container_bottom: id = msg_send![container, bottomAnchor];

            let c1: id =
                msg_send![scroll_leading, constraintEqualToAnchor: container_leading];
            let c2: id =
                msg_send![scroll_trailing, constraintEqualToAnchor: container_trailing];
            let c3: id =
                msg_send![scroll_bottom, constraintEqualToAnchor: container_bottom];
            let _: () = msg_send![c1, setActive: 1i8];
            let _: () = msg_send![c2, setActive: 1i8];
            let _: () = msg_send![c3, setActive: 1i8];

            host_data.scroll_view_using_autolayout = true;
        }

        // Get the safe area top anchor
        let has_safe_area: bool =
            msg_send![container, respondsToSelector: sel!(safeAreaLayoutGuide)];
        let guide_top: id = if has_safe_area {
            let guide: id = msg_send![container, safeAreaLayoutGuide];
            msg_send![guide, topAnchor]
        } else {
            msg_send![container, topAnchor]
        };

        if !has_header {
            // No header: pin scroll_view.top directly to safe area top
            let scroll_top: id = msg_send![scroll_view, topAnchor];
            let constraint: id =
                msg_send![scroll_top, constraintEqualToAnchor: guide_top];
            let _: () = msg_send![constraint, setActive: 1i8];
            let _: () = msg_send![constraint, retain];
            host_data.scroll_view_top_constraint = constraint;
            let _: () = msg_send![container, layoutSubtreeIfNeeded];
            return;
        }

        // Create header container
        let header: id = msg_send![class!(NSView), alloc];
        let header: id = msg_send![header, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(240.0, 28.0),
        )];
        let _: () =
            msg_send![header, setTranslatesAutoresizingMaskIntoConstraints: 0i8];

        // Title label
        if let Some(title_text) = title {
            let label: id =
                msg_send![class!(NSTextField), labelWithString: ns_string(title_text)];
            let _: () =
                msg_send![label, setTranslatesAutoresizingMaskIntoConstraints: 0i8];

            let font: id =
                msg_send![class!(NSFont), systemFontOfSize: 11.0f64 weight: 0.3f64];
            let _: () = msg_send![label, setFont: font];

            let color: id = msg_send![class!(NSColor), secondaryLabelColor];
            let _: () = msg_send![label, setTextColor: color];

            let _: () = msg_send![header, addSubview: label];

            let label_leading: id = msg_send![label, leadingAnchor];
            let label_center_y: id = msg_send![label, centerYAnchor];
            let header_leading: id = msg_send![header, leadingAnchor];
            let header_center_y: id = msg_send![header, centerYAnchor];

            let c1: id = msg_send![label_leading, constraintEqualToAnchor: header_leading constant: 12.0f64];
            let c2: id = msg_send![label_center_y, constraintEqualToAnchor: header_center_y];
            let _: () = msg_send![c1, setActive: 1i8];
            let _: () = msg_send![c2, setActive: 1i8];
        }

        // Buttons (laid out right-to-left)
        let mut targets = Vec::new();
        let header_center_y: id = msg_send![header, centerYAnchor];
        let mut prev_anchor: id = msg_send![header, trailingAnchor];
        let mut first_button = true;

        for (index, symbol) in button_symbols.iter().enumerate().rev() {
            let button: id = msg_send![class!(NSButton), alloc];
            let button: id = msg_send![button, initWithFrame: NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(20.0, 20.0),
            )];
            let _: () =
                msg_send![button, setTranslatesAutoresizingMaskIntoConstraints: 0i8];

            let image: id = msg_send![
                class!(NSImage),
                imageWithSystemSymbolName: ns_string(symbol)
                accessibilityDescription: nil
            ];
            if image != nil {
                let _: () = msg_send![button, setImage: image];
                // NSImageOnly = 1
                let _: () = msg_send![button, setImagePosition: 1i64];
            }

            // Borderless toolbar-style appearance
            let _: () = msg_send![button, setBezelStyle: 0i64];
            let _: () = msg_send![button, setBordered: 0i8];

            // Target/action via header button class
            let target: id = msg_send![SIDEBAR_HEADER_BUTTON_CLASS, alloc];
            let target: id = msg_send![target, init];
            (*(target as *mut Object))
                .set_ivar::<*mut c_void>("hostViewPtr", host_view as *mut c_void);
            (*(target as *mut Object)).set_ivar::<i64>("buttonIndex", index as i64);
            let _: () = msg_send![button, setTarget: target];
            let _: () = msg_send![button, setAction: sel!(headerButtonAction:)];
            targets.push(target);

            let _: () = msg_send![header, addSubview: button];

            let button_trailing: id = msg_send![button, trailingAnchor];
            let button_center_y: id = msg_send![button, centerYAnchor];
            let button_width: id = msg_send![button, widthAnchor];
            let button_height: id = msg_send![button, heightAnchor];

            let offset = if first_button { -8.0f64 } else { -2.0f64 };
            first_button = false;

            let c1: id = msg_send![button_trailing, constraintEqualToAnchor: prev_anchor constant: offset];
            let c2: id = msg_send![button_center_y, constraintEqualToAnchor: header_center_y];
            let c3: id = msg_send![button_width, constraintEqualToConstant: 20.0f64];
            let c4: id = msg_send![button_height, constraintEqualToConstant: 20.0f64];
            let _: () = msg_send![c1, setActive: 1i8];
            let _: () = msg_send![c2, setActive: 1i8];
            let _: () = msg_send![c3, setActive: 1i8];
            let _: () = msg_send![c4, setActive: 1i8];

            prev_anchor = msg_send![button, leadingAnchor];
        }

        // Separator line at bottom of header
        let separator: id = msg_send![class!(NSBox), alloc];
        let separator: id = msg_send![separator, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(240.0, 1.0),
        )];
        // NSBoxSeparator = 2
        let _: () = msg_send![separator, setBoxType: 2u64];
        let _: () =
            msg_send![separator, setTranslatesAutoresizingMaskIntoConstraints: 0i8];
        let _: () = msg_send![header, addSubview: separator];

        let sep_leading: id = msg_send![separator, leadingAnchor];
        let sep_trailing: id = msg_send![separator, trailingAnchor];
        let sep_bottom: id = msg_send![separator, bottomAnchor];
        let h_leading: id = msg_send![header, leadingAnchor];
        let h_trailing: id = msg_send![header, trailingAnchor];
        let h_bottom: id = msg_send![header, bottomAnchor];
        let c1: id = msg_send![sep_leading, constraintEqualToAnchor: h_leading];
        let c2: id = msg_send![sep_trailing, constraintEqualToAnchor: h_trailing];
        let c3: id = msg_send![sep_bottom, constraintEqualToAnchor: h_bottom];
        let _: () = msg_send![c1, setActive: 1i8];
        let _: () = msg_send![c2, setActive: 1i8];
        let _: () = msg_send![c3, setActive: 1i8];

        // Add header to sidebar container
        let _: () = msg_send![container, addSubview: header];

        let header_top: id = msg_send![header, topAnchor];
        let header_leading: id = msg_send![header, leadingAnchor];
        let header_trailing: id = msg_send![header, trailingAnchor];
        let header_height: id = msg_send![header, heightAnchor];
        let container_leading: id = msg_send![container, leadingAnchor];
        let container_trailing: id = msg_send![container, trailingAnchor];

        let c1: id = msg_send![header_top, constraintEqualToAnchor: guide_top];
        let c2: id =
            msg_send![header_leading, constraintEqualToAnchor: container_leading];
        let c3: id =
            msg_send![header_trailing, constraintEqualToAnchor: container_trailing];
        let c4: id =
            msg_send![header_height, constraintEqualToConstant: 28.0f64];
        let _: () = msg_send![c1, setActive: 1i8];
        let _: () = msg_send![c2, setActive: 1i8];
        let _: () = msg_send![c3, setActive: 1i8];
        let _: () = msg_send![c4, setActive: 1i8];

        // Pin scroll_view.top below header
        let scroll_top: id = msg_send![scroll_view, topAnchor];
        let header_bottom_anchor: id = msg_send![header, bottomAnchor];
        let constraint: id =
            msg_send![scroll_top, constraintEqualToAnchor: header_bottom_anchor];
        let _: () = msg_send![constraint, setActive: 1i8];
        let _: () = msg_send![constraint, retain];
        host_data.scroll_view_top_constraint = constraint;

        let _: () = msg_send![header, retain];
        host_data.header_view = header;
        host_data.header_button_targets = targets;

        let _: () = msg_send![container, layoutSubtreeIfNeeded];
    }
}

/// Embed a secondary view (e.g. GPUISurfaceView) into the sidebar (left) pane
/// using Auto Layout constraints pinned below the titlebar safe area.
/// The sidebar must have been created with embed_in_sidebar=true to have
/// the correct container (plain NSView + NSVisualEffectView background).
pub(crate) unsafe fn embed_surface_view_in_sidebar(host_view: id, surface_view: id) {
    unsafe {
        if host_view == nil || surface_view == nil {
            return;
        }

        let Some(host_data) = host_data_mut(host_view) else {
            return;
        };

        let target_pane = host_data.sidebar_container;
        if target_pane == nil {
            return;
        }

        // Check if already embedded
        let current_superview: id = msg_send![surface_view, superview];
        if current_superview == target_pane {
            return;
        }

        // Remove from any previous superview
        if current_superview != nil {
            let _: () = msg_send![surface_view, removeFromSuperview];
        }

        // Use Auto Layout to pin below the titlebar safe area
        let _: () = msg_send![
            surface_view,
            setTranslatesAutoresizingMaskIntoConstraints: 0i8
        ];
        let _: () = msg_send![target_pane, addSubview: surface_view];

        let has_safe_area: bool =
            msg_send![target_pane, respondsToSelector: sel!(safeAreaLayoutGuide)];
        let guide_top: id = if has_safe_area {
            let guide: id = msg_send![target_pane, safeAreaLayoutGuide];
            msg_send![guide, topAnchor]
        } else {
            msg_send![target_pane, topAnchor]
        };

        let pane_leading: id = msg_send![target_pane, leadingAnchor];
        let pane_trailing: id = msg_send![target_pane, trailingAnchor];
        let pane_bottom: id = msg_send![target_pane, bottomAnchor];

        let view_top: id = msg_send![surface_view, topAnchor];
        let view_leading: id = msg_send![surface_view, leadingAnchor];
        let view_trailing: id = msg_send![surface_view, trailingAnchor];
        let view_bottom: id = msg_send![surface_view, bottomAnchor];

        let c1: id = msg_send![view_top, constraintEqualToAnchor: guide_top];
        let c2: id = msg_send![view_leading, constraintEqualToAnchor: pane_leading];
        let c3: id = msg_send![view_trailing, constraintEqualToAnchor: pane_trailing];
        let c4: id = msg_send![view_bottom, constraintEqualToAnchor: pane_bottom];

        let _: () = msg_send![c1, setActive: 1i8];
        let _: () = msg_send![c2, setActive: 1i8];
        let _: () = msg_send![c3, setActive: 1i8];
        let _: () = msg_send![c4, setActive: 1i8];

        let _: () = msg_send![target_pane, layoutSubtreeIfNeeded];

        // Make the Metal layer non-opaque so the NSVisualEffectView vibrancy
        // shows through transparent areas of the GPUI content.
        let layer: id = msg_send![surface_view, layer];
        if layer != nil {
            let _: () = msg_send![layer, setOpaque: 0i8];
        }
    }
}

/// Applies a solid background color to the entire sidebar container — both the
/// content area and the titlebar region above the safe area. Sets the container's
/// CALayer background color and hides any NSVisualEffectView subviews so vibrancy
/// does not paint over the solid color. The transparent GPUI Metal surface renders
/// on top, so GPUI content is visible and transparent areas show the solid layer.
pub(crate) unsafe fn set_native_sidebar_background_color(
    host_view: id,
    r: f64,
    g: f64,
    b: f64,
    a: f64,
) {
    unsafe {
        let Some(host_data) = host_data_mut(host_view) else {
            return;
        };

        let container = host_data.sidebar_container;
        if container == nil {
            return;
        }

        let _: () = msg_send![container, setWantsLayer: 1i8];
        let layer: id = msg_send![container, layer];
        if layer != nil {
            let ns_color: id = msg_send![
                class!(NSColor),
                colorWithRed: r green: g blue: b alpha: a
            ];
            let cg_color: id = msg_send![ns_color, CGColor];
            let _: () = msg_send![layer, setBackgroundColor: cg_color];
        }

        // Hide NSVisualEffectView subviews so vibrancy does not paint over the solid color.
        let subviews: id = msg_send![container, subviews];
        let count: usize = msg_send![subviews, count];
        for i in 0..count {
            let subview: id = msg_send![subviews, objectAtIndex: i];
            let is_vfx: bool =
                msg_send![subview, isKindOfClass: class!(NSVisualEffectView)];
            if is_vfx {
                let _: () = msg_send![subview, setHidden: 1i8];
            }
        }
    }
}

/// Clears the solid background color from the sidebar container, restoring
/// native glass or vibrancy by unhiding any NSVisualEffectView subviews.
pub(crate) unsafe fn clear_native_sidebar_background_color(host_view: id) {
    unsafe {
        let Some(host_data) = host_data_mut(host_view) else {
            return;
        };

        let container = host_data.sidebar_container;
        if container == nil {
            return;
        }

        let layer: id = msg_send![container, layer];
        if layer != nil {
            let _: () = msg_send![layer, setBackgroundColor: nil];
        }

        // Unhide NSVisualEffectView subviews to restore native vibrancy.
        let subviews: id = msg_send![container, subviews];
        let count: usize = msg_send![subviews, count];
        for i in 0..count {
            let subview: id = msg_send![subviews, objectAtIndex: i];
            let is_vfx: bool =
                msg_send![subview, isKindOfClass: class!(NSVisualEffectView)];
            if is_vfx {
                let _: () = msg_send![subview, setHidden: 0i8];
            }
        }
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

/// GCD callback that resizes the embedded content view to fill its
/// superview. Called asynchronously after sidebar teardown so the GPUI
/// render cycle has finished and the window is no longer borrowed.
unsafe extern "C" fn deferred_resize_embedded_view(context: *mut c_void) {
    unsafe {
        let view = context as id;
        let superview: id = msg_send![view, superview];
        if superview != nil {
            let bounds: NSRect = msg_send![superview, bounds];
            let _: () = msg_send![view, setFrameOrigin: bounds.origin];
            let _: () = msg_send![view, setFrameSize: bounds.size];
        }
        // Release the retain we took before dispatch_async_f
        let _: () = msg_send![view, release];
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
            let host_superview: id = msg_send![host_view, superview];
            if host_superview != nil {
                let _: () = msg_send![host_view, removeFromSuperview];
            }
            if host_data.window != nil {
                let _: () = msg_send![
                    host_data.window,
                    setContentMinSize: host_data.previous_content_min_size
                ];
                let _: () = msg_send![
                    host_data.window,
                    setContentMaxSize: host_data.previous_content_max_size
                ];

                // Remove the embedded content view from the split view
                // hierarchy before replacing the content view controller.
                if host_data.embedded_content_view != nil {
                    let _: () = msg_send![
                        host_data.embedded_content_view,
                        setTranslatesAutoresizingMaskIntoConstraints: 1i8
                    ];
                    let _: () = msg_send![
                        host_data.embedded_content_view,
                        setAutoresizingMask: NSViewWidthSizable | NSViewHeightSizable
                    ];
                    let current_superview: id =
                        msg_send![host_data.embedded_content_view, superview];
                    if current_superview != nil {
                        let _: () = msg_send![host_data.embedded_content_view, removeFromSuperview];
                    }
                }

                // Restore the content view controller / content view.
                let current_content_view_controller: id =
                    msg_send![host_data.window, contentViewController];
                if current_content_view_controller == host_data.split_view_controller {
                    if host_data.previous_content_view_controller != nil {
                        let _: () = msg_send![
                            host_data.window,
                            setContentViewController: host_data.previous_content_view_controller
                        ];
                    } else {
                        // No previous VC — create a plain content view to
                        // replace the split view controller's view.
                        let window_frame: NSRect = msg_send![host_data.window, frame];
                        let content_rect: NSRect =
                            msg_send![host_data.window, contentRectForFrameRect: window_frame];
                        let new_content_view: id = msg_send![class!(NSView), alloc];
                        let new_content_view: id = msg_send![new_content_view,
                            initWithFrame: NSRect::new(
                                NSPoint::new(0.0, 0.0),
                                content_rect.size,
                            )
                        ];
                        let _: () = msg_send![host_data.window, setContentView: new_content_view];
                        let _: () = msg_send![new_content_view, release];
                    }
                }

                // Re-add the embedded content view to the (now restored)
                // window content view so it stays alive and visible.
                if host_data.embedded_content_view != nil {
                    let content_view: id = msg_send![host_data.window, contentView];
                    if content_view != nil {
                        // Add with old frame — we defer the resize below.
                        let _: () = msg_send![
                            content_view,
                            addSubview: host_data.embedded_content_view
                        ];
                    }
                    // Restore first responder so keyboard events reach
                    // the embedded view (e.g. the GPUI native view).
                    let _: () = msg_send![
                        host_data.window,
                        makeFirstResponder: host_data.embedded_content_view
                    ];

                    // Defer the frame resize to the next main-queue
                    // iteration. During sidebar teardown we're inside a
                    // GPUI render cycle, so the view's setFrameSize:
                    // handler can't trigger the resize callback (the
                    // window is borrowed). Dispatching asynchronously
                    // ensures the callback fires when the App is idle.
                    let _: () = msg_send![host_data.embedded_content_view, retain];
                    dispatch_async_f(
                        dispatch_get_main_queue(),
                        host_data.embedded_content_view as *mut c_void,
                        Some(deferred_resize_embedded_view),
                    );

                    // Release the sidebar's retain (the content view's
                    // subview list now holds the reference).
                    let _: () = msg_send![host_data.embedded_content_view, release];
                }

                let toolbar: id = msg_send![host_data.window, toolbar];
                if host_data.sidebar_toolbar != nil && toolbar == host_data.sidebar_toolbar {
                    let _: () = msg_send![host_data.window, setToolbar: host_data.previous_toolbar];
                }
            }
            // Release header resources
            for target in &host_data.header_button_targets {
                let _: () = msg_send![*target, release];
            }
            if host_data.header_view != nil {
                let _: () = msg_send![host_data.header_view, removeFromSuperview];
                let _: () = msg_send![host_data.header_view, release];
            }
            if host_data.scroll_view_top_constraint != nil {
                let _: () =
                    msg_send![host_data.scroll_view_top_constraint, setActive: 0i8];
                let _: () = msg_send![host_data.scroll_view_top_constraint, release];
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
            if host_data.table_view != nil {
                let _: () = msg_send![host_data.table_view, setDataSource: nil];
                let _: () = msg_send![host_data.table_view, setDelegate: nil];
            }
            if host_data.sidebar_container != nil {
                let _: () = msg_send![host_data.sidebar_container, release];
            }

            let _: () = msg_send![host_data.split_view_controller, release];
        }

        let object = host_view as *mut Object;
        (*object).set_ivar::<*mut c_void>(HOST_DATA_IVAR, ptr::null_mut());
        let _: () = msg_send![host_view, release];
    }
}
