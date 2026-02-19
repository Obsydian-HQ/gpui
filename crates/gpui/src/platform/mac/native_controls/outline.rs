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
    runtime::{Class, Object, Sel},
    sel, sel_impl,
};
use std::{ffi::c_void, ptr};

const SUPPRESS_HIGHLIGHT_IVAR: &str = "suppressHighlight";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeOutlineNodeData {
    pub title: String,
    pub children: Vec<NativeOutlineNodeData>,
}

struct OutlineCallbacks {
    roots: id,
    on_select: Option<Box<dyn Fn((usize, String))>>,
}

impl Drop for OutlineCallbacks {
    fn drop(&mut self) {
        unsafe {
            if self.roots != nil {
                let _: () = msg_send![self.roots, release];
            }
        }
    }
}

static mut OUTLINE_VIEW_CLASS: *const Class = ptr::null();
static mut OUTLINE_DELEGATE_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_outline_view_class() {
    unsafe {
        let mut decl =
            ClassDecl::new("GPUINativeOutlineView", class!(NSOutlineView)).unwrap();
        decl.add_ivar::<i8>(SUPPRESS_HIGHLIGHT_IVAR);

        decl.add_method(
            sel!(highlightSelectionInClipRect:),
            highlight_selection_in_clip_rect as extern "C" fn(&Object, Sel, NSRect),
        );

        OUTLINE_VIEW_CLASS = decl.register();
    }
}

extern "C" fn highlight_selection_in_clip_rect(this: &Object, _sel: Sel, _clip_rect: NSRect) {
    unsafe {
        let suppress: i8 = *this.get_ivar(SUPPRESS_HIGHLIGHT_IVAR);
        if suppress != 0 {
            return;
        }
        // Call super
        let superclass = class!(NSOutlineView);
        let _: () = msg_send![super(this, superclass), highlightSelectionInClipRect: _clip_rect];
    }
}

#[ctor]
unsafe fn build_outline_delegate_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUINativeOutlineDelegate", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);

        decl.add_method(
            sel!(outlineView:numberOfChildrenOfItem:),
            number_of_children as extern "C" fn(&Object, Sel, id, id) -> i64,
        );
        decl.add_method(
            sel!(outlineView:isItemExpandable:),
            is_item_expandable as extern "C" fn(&Object, Sel, id, id) -> i8,
        );
        decl.add_method(
            sel!(outlineView:child:ofItem:),
            child_of_item as extern "C" fn(&Object, Sel, id, i64, id) -> id,
        );
        decl.add_method(
            sel!(outlineView:objectValueForTableColumn:byItem:),
            object_value_for_item as extern "C" fn(&Object, Sel, id, id, id) -> id,
        );
        decl.add_method(
            sel!(outlineViewSelectionDidChange:),
            selection_did_change as extern "C" fn(&Object, Sel, id),
        );

        OUTLINE_DELEGATE_CLASS = decl.register();
    }
}

unsafe fn string_from_ns_string(ns_string: id) -> String {
    unsafe {
        let cstr: *const std::os::raw::c_char = msg_send![ns_string, UTF8String];
        if cstr.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(cstr)
                .to_string_lossy()
                .into_owned()
        }
    }
}

unsafe fn children_array(roots: id, item: id) -> id {
    unsafe {
        use super::super::ns_string;

        if item == nil {
            roots
        } else {
            msg_send![item, objectForKey: ns_string("children")]
        }
    }
}

extern "C" fn number_of_children(this: &Object, _sel: Sel, _outline: id, item: id) -> i64 {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if ptr.is_null() {
            return 0;
        }

        let callbacks = &*(ptr as *const OutlineCallbacks);
        let children = children_array(callbacks.roots, item);
        let count: u64 = msg_send![children, count];
        count as i64
    }
}

extern "C" fn is_item_expandable(this: &Object, _sel: Sel, _outline: id, item: id) -> i8 {
    unsafe {
        if item == nil {
            return 1;
        }

        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if ptr.is_null() {
            return 0;
        }

        let callbacks = &*(ptr as *const OutlineCallbacks);
        let children = children_array(callbacks.roots, item);
        let count: u64 = msg_send![children, count];
        (count > 0) as i8
    }
}

extern "C" fn child_of_item(this: &Object, _sel: Sel, _outline: id, index: i64, item: id) -> id {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if ptr.is_null() || index < 0 {
            return nil;
        }

        let callbacks = &*(ptr as *const OutlineCallbacks);
        let children = children_array(callbacks.roots, item);
        let count: u64 = msg_send![children, count];
        if (index as u64) >= count {
            return nil;
        }

        msg_send![children, objectAtIndex: index as u64]
    }
}

extern "C" fn object_value_for_item(
    this: &Object,
    _sel: Sel,
    _outline: id,
    _column: id,
    item: id,
) -> id {
    unsafe {
        use super::super::ns_string;

        if item == nil {
            return ns_string("");
        }

        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if ptr.is_null() {
            return ns_string("");
        }

        msg_send![item, objectForKey: ns_string("title")]
    }
}

extern "C" fn selection_did_change(this: &Object, _sel: Sel, notification: id) {
    unsafe {
        use super::super::ns_string;

        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if ptr.is_null() {
            return;
        }

        let callbacks = &*(ptr as *const OutlineCallbacks);
        if let Some(ref on_select) = callbacks.on_select {
            let outline: id = msg_send![notification, object];
            let row: i64 = msg_send![outline, selectedRow];
            if row >= 0 {
                let item: id = msg_send![outline, itemAtRow: row];
                if item != nil {
                    let title_obj: id = msg_send![item, objectForKey: ns_string("title")];
                    on_select((row as usize, string_from_ns_string(title_obj)));
                }
            }
        }
    }
}

unsafe fn node_to_dictionary(node: &NativeOutlineNodeData) -> id {
    unsafe {
        use super::super::ns_string;

        let dict: id = msg_send![class!(NSMutableDictionary), dictionary];
        let _: () = msg_send![dict, setObject: ns_string(&node.title) forKey: ns_string("title")];

        let children: id =
            msg_send![class!(NSMutableArray), arrayWithCapacity: node.children.len() as u64];
        for child in &node.children {
            let child_dict = node_to_dictionary(child);
            let _: () = msg_send![children, addObject: child_dict];
        }

        let _: () = msg_send![dict, setObject: children forKey: ns_string("children")];
        dict
    }
}

unsafe fn outline_from_scroll(scroll_view: id) -> id {
    unsafe { msg_send![scroll_view, documentView] }
}

pub(crate) unsafe fn create_native_outline_view() -> id {
    unsafe {
        use super::super::ns_string;

        let outline: id = msg_send![OUTLINE_VIEW_CLASS, alloc];
        let outline: id = msg_send![outline, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(200.0, 220.0),
        )];
        // Default: highlight enabled
        (*outline).set_ivar::<i8>(SUPPRESS_HIGHLIGHT_IVAR, 0);
        let _: () = msg_send![outline, setHeaderView: ptr::null_mut::<c_void>() as id];
        let _: () = msg_send![outline, setIndentationPerLevel: 14.0f64];
        let _: () = msg_send![outline, setAutoresizingMask: 0u64];
        // NSOutlineViewUniformColumnAutoresizingStyle (1) — resize column to fill
        let _: () = msg_send![outline, setColumnAutoresizingStyle: 1u64];

        let column: id = msg_send![class!(NSTableColumn), alloc];
        let column: id = msg_send![column, initWithIdentifier: ns_string("title")];
        let _: () = msg_send![column, setWidth: 100.0f64];
        let _: () = msg_send![column, setMinWidth: 20.0f64];
        // NSTableColumnAutoresizingMask (1) — allow column to auto-resize
        let _: () = msg_send![column, setResizingMask: 1u64];
        let _: () = msg_send![outline, addTableColumn: column];
        let _: () = msg_send![outline, setOutlineTableColumn: column];
        let _: () = msg_send![column, release];

        let scroll: id = msg_send![class!(NSScrollView), alloc];
        let scroll: id = msg_send![scroll, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(200.0, 220.0),
        )];
        let _: () = msg_send![scroll, setHasVerticalScroller: 1i8];
        let _: () = msg_send![scroll, setHasHorizontalScroller: 0i8];
        let _: () = msg_send![scroll, setAutohidesScrollers: 1i8];
        let _: () = msg_send![scroll, setBorderType: 0u64]; // no border
        let _: () = msg_send![scroll, setDrawsBackground: 0i8]; // transparent for glass
        let _: () = msg_send![scroll, setDocumentView: outline];
        let _: () = msg_send![scroll, setAutoresizingMask: 0u64];
        let _: () = msg_send![scroll, setHorizontalScrollElasticity: 1i64]; // none

        // Make the outline view background transparent too
        let clear_color: id = msg_send![class!(NSColor), clearColor];
        let _: () = msg_send![outline, setBackgroundColor: clear_color];

        // Fit the column to the scroll view width
        let _: () = msg_send![outline, sizeLastColumnToFit];

        scroll
    }
}

pub(crate) unsafe fn set_native_outline_items(
    scroll_view: id,
    nodes: &[NativeOutlineNodeData],
    selected_row: Option<usize>,
    expand_all: bool,
    on_select: Option<Box<dyn Fn((usize, String))>>,
) -> *mut c_void {
    unsafe {
        let outline = outline_from_scroll(scroll_view);

        let roots: id = msg_send![class!(NSMutableArray), arrayWithCapacity: nodes.len() as u64];
        for node in nodes {
            let dict = node_to_dictionary(node);
            let _: () = msg_send![roots, addObject: dict];
        }
        let roots: id = msg_send![roots, retain];

        let callbacks = OutlineCallbacks { roots, on_select };

        let delegate: id = msg_send![OUTLINE_DELEGATE_CLASS, alloc];
        let delegate: id = msg_send![delegate, init];

        let callbacks_ptr = Box::into_raw(Box::new(callbacks)) as *mut c_void;
        (*delegate).set_ivar::<*mut c_void>(CALLBACK_IVAR, callbacks_ptr);

        let _: () = msg_send![outline, setDataSource: delegate];
        let _: () = msg_send![outline, setDelegate: delegate];
        let _: () = msg_send![outline, reloadData];

        if expand_all {
            let _: () = msg_send![outline, expandItem: nil expandChildren: 1i8];
        }

        if let Some(selected) = selected_row {
            let row_count: i64 = msg_send![outline, numberOfRows];
            if row_count > 0 {
                let clamped = (selected as i64).min(row_count - 1).max(0);
                let index_set: id =
                    msg_send![class!(NSIndexSet), indexSetWithIndex: clamped as u64];
                let _: () =
                    msg_send![outline, selectRowIndexes: index_set byExtendingSelection: 0i8];
            }
        }

        delegate as *mut c_void
    }
}

/// Syncs the outline column width to match the scroll view's visible width.
/// Call after `set_native_view_frame` to keep the column from overflowing.
pub(crate) unsafe fn sync_native_outline_column_width(scroll_view: id) {
    unsafe {
        let outline = outline_from_scroll(scroll_view);
        if outline == nil {
            return;
        }

        let clip_view: id = msg_send![scroll_view, contentView];
        if clip_view == nil {
            return;
        }

        let clip_bounds: NSRect = msg_send![clip_view, bounds];
        let available_width = clip_bounds.size.width;
        if available_width <= 0.0 {
            return;
        }

        // Get the first (only) column and resize it to fill the visible width
        let columns: id = msg_send![outline, tableColumns];
        let count: u64 = msg_send![columns, count];
        if count > 0 {
            let column: id = msg_send![columns, objectAtIndex: 0u64];
            if column != nil {
                let _: () = msg_send![column, setWidth: available_width];
            }
        }

        let _: () = msg_send![outline, sizeLastColumnToFit];
    }
}

/// Configures the outline view highlight and focus ring.
/// `style`: 0 = Regular, 1 = SourceList, -1 = None (suppress both).
pub(crate) unsafe fn set_native_outline_highlight_style(scroll_view: id, style: i64) {
    unsafe {
        let outline = outline_from_scroll(scroll_view);
        let suppress: i8 = if style == -1 { 1 } else { 0 };
        (*outline).set_ivar::<i8>(SUPPRESS_HIGHLIGHT_IVAR, suppress);

        // NSFocusRingTypeNone = 1, NSFocusRingTypeDefault = 0
        let ring_type: u64 = if style == -1 { 1 } else { 0 };
        let _: () = msg_send![outline, setFocusRingType: ring_type];
        let _: () = msg_send![scroll_view, setFocusRingType: ring_type];

        let _: () = msg_send![outline, setNeedsDisplay: 1i8];
    }
}

pub(crate) unsafe fn set_native_outline_row_height(scroll_view: id, row_height: f64) {
    unsafe {
        let outline = outline_from_scroll(scroll_view);
        let _: () = msg_send![outline, setRowHeight: row_height.max(16.0)];
    }
}

pub(crate) unsafe fn release_native_outline_target(target: *mut c_void) {
    unsafe {
        if target.is_null() {
            return;
        }

        let delegate = target as id;
        let callbacks_ptr: *mut c_void = *(*delegate).get_ivar(CALLBACK_IVAR);
        if !callbacks_ptr.is_null() {
            let _ = Box::from_raw(callbacks_ptr as *mut OutlineCallbacks);
        }
        let _: () = msg_send![delegate, release];
    }
}

pub(crate) unsafe fn release_native_outline_view(scroll_view: id) {
    unsafe {
        let outline = outline_from_scroll(scroll_view);
        let _: () = msg_send![outline, setDataSource: ptr::null_mut::<c_void>() as id];
        let _: () = msg_send![outline, setDelegate: ptr::null_mut::<c_void>() as id];
        let _: () = msg_send![scroll_view, release];
    }
}
