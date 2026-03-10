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

struct TableCallbacks {
    items: Vec<String>,
    on_select: Option<Box<dyn Fn(usize)>>,
}

static mut TABLE_DELEGATE_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_table_delegate_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUINativeTableDelegate", class!(NSObject)).unwrap();
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

        TABLE_DELEGATE_CLASS = decl.register();
    }
}

extern "C" fn number_of_rows(this: &Object, _sel: Sel, _table: id) -> i64 {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if ptr.is_null() {
            return 0;
        }
        let callbacks = &*(ptr as *const TableCallbacks);
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
        let callbacks = &*(ptr as *const TableCallbacks);
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
        let callbacks = &*(ptr as *const TableCallbacks);
        if let Some(ref on_select) = callbacks.on_select {
            let table: id = msg_send![notification, object];
            let row: i64 = msg_send![table, selectedRow];
            if row >= 0 {
                on_select(row as usize);
            }
        }
    }
}

unsafe fn table_from_scroll(scroll_view: id) -> id {
    unsafe { msg_send![scroll_view, documentView] }
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

pub(crate) unsafe fn create_native_table_view() -> id {
    unsafe {
        use super::super::ns_string;

        let table: id = msg_send![class!(NSTableView), alloc];
        let table: id = msg_send![table, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(320.0, 220.0),
        )];
        let _: () = msg_send![table, setUsesAlternatingRowBackgroundColors: 1i8];
        let _: () = msg_send![table, setAllowsMultipleSelection: 0i8];
        let _: () = msg_send![table, setAutoresizingMask: 0u64];

        let column: id = msg_send![class!(NSTableColumn), alloc];
        let column: id = msg_send![column, initWithIdentifier: ns_string("value")];
        let _: () = msg_send![column, setWidth: 320.0f64];
        let _: () = msg_send![column, setEditable: 0i8];
        let _: () = msg_send![table, addTableColumn: column];
        let _: () = msg_send![column, release];

        let scroll: id = msg_send![class!(NSScrollView), alloc];
        let scroll: id = msg_send![scroll, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(320.0, 220.0),
        )];
        let _: () = msg_send![scroll, setHasVerticalScroller: 1i8];
        let _: () = msg_send![scroll, setHasHorizontalScroller: 0i8];
        let _: () = msg_send![scroll, setBorderType: 1u64];
        let _: () = msg_send![scroll, setDocumentView: table];
        let _: () = msg_send![scroll, setAutoresizingMask: 0u64];

        scroll
    }
}

pub(crate) unsafe fn set_native_table_column_title(scroll_view: id, title: &str) {
    unsafe {
        use super::super::ns_string;
        let table = table_from_scroll(scroll_view);
        let column = primary_table_column(table);
        if column != nil {
            let header_cell: id = msg_send![column, headerCell];
            if header_cell != nil {
                let _: () = msg_send![header_cell, setStringValue: ns_string(title)];
            }
        }
    }
}

pub(crate) unsafe fn set_native_table_column_width(scroll_view: id, width: f64) {
    unsafe {
        let table = table_from_scroll(scroll_view);
        let column = primary_table_column(table);
        if column != nil {
            let _: () = msg_send![column, setWidth: width.max(80.0)];
        }
    }
}

pub(crate) unsafe fn set_native_table_items(
    scroll_view: id,
    items: &[&str],
    selected_index: Option<usize>,
    on_select: Option<Box<dyn Fn(usize)>>,
) -> *mut c_void {
    unsafe {
        let table = table_from_scroll(scroll_view);

        let delegate: id = msg_send![TABLE_DELEGATE_CLASS, alloc];
        let delegate: id = msg_send![delegate, init];

        let callbacks = TableCallbacks {
            items: items.iter().map(|item| item.to_string()).collect(),
            on_select,
        };
        let callbacks_ptr = Box::into_raw(Box::new(callbacks)) as *mut c_void;
        (*delegate).set_ivar::<*mut c_void>(CALLBACK_IVAR, callbacks_ptr);

        let _: () = msg_send![table, setDataSource: delegate];
        let _: () = msg_send![table, setDelegate: delegate];
        let _: () = msg_send![table, reloadData];

        if let Some(index) = selected_index {
            let row_count: i64 = msg_send![table, numberOfRows];
            if row_count > 0 {
                let clamped = (index as i64).min(row_count - 1).max(0);
                let index_set: id =
                    msg_send![class!(NSIndexSet), indexSetWithIndex: clamped as u64];
                let _: () = msg_send![table, selectRowIndexes: index_set byExtendingSelection: 0i8];
            }
        }

        delegate as *mut c_void
    }
}

pub(crate) unsafe fn set_native_table_row_height(scroll_view: id, row_height: f64) {
    unsafe {
        let table = table_from_scroll(scroll_view);
        let _: () = msg_send![table, setRowHeight: row_height.max(16.0)];
    }
}

pub(crate) unsafe fn set_native_table_row_size_style(scroll_view: id, row_size_style: i64) {
    unsafe {
        let table = table_from_scroll(scroll_view);
        let _: () = msg_send![table, setRowSizeStyle: row_size_style];
    }
}

pub(crate) unsafe fn set_native_table_style(scroll_view: id, style: i64) {
    unsafe {
        let table = table_from_scroll(scroll_view);
        let _: () = msg_send![table, setStyle: style];
    }
}

pub(crate) unsafe fn set_native_table_selection_highlight_style(
    scroll_view: id,
    highlight_style: i64,
) {
    unsafe {
        let table = table_from_scroll(scroll_view);
        let _: () = msg_send![table, setSelectionHighlightStyle: highlight_style];
    }
}

pub(crate) unsafe fn set_native_table_grid_style(scroll_view: id, grid_style_mask: u64) {
    unsafe {
        let table = table_from_scroll(scroll_view);
        let _: () = msg_send![table, setGridStyleMask: grid_style_mask];
    }
}

pub(crate) unsafe fn set_native_table_uses_alternating_rows(scroll_view: id, uses: bool) {
    unsafe {
        let table = table_from_scroll(scroll_view);
        let _: () = msg_send![table, setUsesAlternatingRowBackgroundColors: uses as i8];
    }
}

pub(crate) unsafe fn set_native_table_allows_multiple_selection(
    scroll_view: id,
    allows_multiple: bool,
) {
    unsafe {
        let table = table_from_scroll(scroll_view);
        let _: () = msg_send![table, setAllowsMultipleSelection: allows_multiple as i8];
    }
}

pub(crate) unsafe fn set_native_table_show_header(scroll_view: id, show_header: bool) {
    unsafe {
        let table = table_from_scroll(scroll_view);
        if show_header {
            let current: id = msg_send![table, headerView];
            if current == nil {
                let frame: NSRect = msg_send![table, frame];
                let header: id = msg_send![class!(NSTableHeaderView), alloc];
                let header: id = msg_send![header, initWithFrame: NSRect::new(
                    NSPoint::new(0.0, 0.0),
                    NSSize::new(frame.size.width, 17.0),
                )];
                let _: () = msg_send![table, setHeaderView: header];
                let _: () = msg_send![header, release];
            }
        } else {
            let _: () = msg_send![table, setHeaderView: nil];
        }
    }
}

pub(crate) unsafe fn release_native_table_target(target: *mut c_void) {
    unsafe {
        if target.is_null() {
            return;
        }

        let delegate = target as id;
        let callbacks_ptr: *mut c_void = *(*delegate).get_ivar(CALLBACK_IVAR);
        if !callbacks_ptr.is_null() {
            let _ = Box::from_raw(callbacks_ptr as *mut TableCallbacks);
        }
        let _: () = msg_send![delegate, release];
    }
}

pub(crate) unsafe fn release_native_table_view(scroll_view: id) {
    unsafe {
        let table = table_from_scroll(scroll_view);
        let _: () = msg_send![table, setDataSource: ptr::null_mut::<c_void>() as id];
        let _: () = msg_send![table, setDelegate: ptr::null_mut::<c_void>() as id];
        let _: () = msg_send![scroll_view, release];
    }
}
