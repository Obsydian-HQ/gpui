use super::{id, nil, ns_string, CALLBACK_IVAR};
use ctor::ctor;
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Sel},
    sel, sel_impl,
};
use std::{ffi::c_void, ptr};

const DATA_IVAR: &str = "dataPtr";

static mut OUTLINE_DATA_SOURCE_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_outline_data_source_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUIiOSOutlineDataSource", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>(DATA_IVAR);
        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);
        decl.add_method(
            sel!(tableView:numberOfRowsInSection:),
            number_of_rows as extern "C" fn(&Object, Sel, id, isize) -> isize,
        );
        decl.add_method(
            sel!(tableView:cellForRowAtIndexPath:),
            cell_for_row as extern "C" fn(&Object, Sel, id, id) -> id,
        );
        decl.add_method(
            sel!(tableView:didSelectRowAtIndexPath:),
            did_select_row as extern "C" fn(&Object, Sel, id, id),
        );
        OUTLINE_DATA_SOURCE_CLASS = decl.register();
    }
}

extern "C" fn number_of_rows(this: &Object, _sel: Sel, _table: id, _section: isize) -> isize {
    unsafe {
        let data_ptr: *mut c_void = *this.get_ivar(DATA_IVAR);
        if data_ptr.is_null() {
            return 0;
        }

        let rows = &*(data_ptr as *const Vec<String>);
        rows.len() as isize
    }
}

extern "C" fn cell_for_row(this: &Object, _sel: Sel, table: id, index_path: id) -> id {
    unsafe {
        let row_index: isize = msg_send![index_path, row];
        let cell_id = ns_string("GPUIOutlineCell");
        let mut cell: id = msg_send![table, dequeueReusableCellWithIdentifier: cell_id];
        if cell == nil {
            cell = msg_send![class!(UITableViewCell), alloc];
            cell = msg_send![cell, initWithStyle: 0i64 reuseIdentifier: cell_id];
        }

        let data_ptr: *mut c_void = *this.get_ivar(DATA_IVAR);
        if !data_ptr.is_null() {
            let rows = &*(data_ptr as *const Vec<String>);
            if let Some(title) = rows.get(row_index as usize) {
                let text_label: id = msg_send![cell, textLabel];
                let _: () = msg_send![text_label, setText: ns_string(title)];
            }
        }

        cell
    }
}

extern "C" fn did_select_row(this: &Object, _sel: Sel, _table: id, index_path: id) {
    unsafe {
        let callback_ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if callback_ptr.is_null() {
            return;
        }

        let data_ptr: *mut c_void = *this.get_ivar(DATA_IVAR);
        if data_ptr.is_null() {
            return;
        }

        let row_index: isize = msg_send![index_path, row];
        let rows = &*(data_ptr as *const Vec<String>);
        let Some(title) = rows.get(row_index as usize) else {
            return;
        };

        let callback = &*(callback_ptr as *const Box<dyn Fn((usize, String))>);
        callback((row_index as usize, title.clone()));
    }
}

pub(crate) unsafe fn create_native_outline_view() -> id {
    unsafe {
        let table: id = msg_send![class!(UITableView), alloc];
        let table: id = msg_send![table, initWithFrame:
            ((0.0f64, 0.0f64), (320.0f64, 480.0f64))
            style: 1i64
        ];
        table
    }
}

pub(crate) unsafe fn set_native_outline_items(
    table: id,
    items: Vec<String>,
    on_select: Option<Box<dyn Fn((usize, String))>>,
    selected_row: Option<usize>,
) -> *mut c_void {
    unsafe {
        let source: id = msg_send![OUTLINE_DATA_SOURCE_CLASS, alloc];
        let source: id = msg_send![source, init];

        let data_ptr = Box::into_raw(Box::new(items)) as *mut c_void;
        (*source).set_ivar::<*mut c_void>(DATA_IVAR, data_ptr);
        let callback_ptr = on_select
            .map(|callback| Box::into_raw(Box::new(callback)) as *mut c_void)
            .unwrap_or(std::ptr::null_mut());
        (*source).set_ivar::<*mut c_void>(CALLBACK_IVAR, callback_ptr);

        let _: () = msg_send![table, setDataSource: source];
        let _: () = msg_send![table, setDelegate: source];
        let _: () = msg_send![table, reloadData];

        if let Some(selected_row) = selected_row {
            let index_path: id = msg_send![class!(NSIndexPath),
                indexPathForRow: selected_row as isize
                inSection: 0isize
            ];
            let _: () = msg_send![table,
                selectRowAtIndexPath: index_path
                animated: false as i8
                scrollPosition: 0i64
            ];
        }

        source as *mut c_void
    }
}

pub(crate) unsafe fn sync_native_outline_column_width(_table: id) {}

pub(crate) unsafe fn set_native_outline_highlight_style(_table: id, _style: i64) {}

pub(crate) unsafe fn set_native_outline_row_height(table: id, row_height: f64) {
    unsafe {
        let _: () = msg_send![table, setRowHeight: row_height];
    }
}

pub(crate) unsafe fn release_native_outline_target(target: *mut c_void) {
    unsafe {
        if !target.is_null() {
            let source = target as id;
            let data_ptr: *mut c_void = *(*source).get_ivar(DATA_IVAR);
            if !data_ptr.is_null() {
                let _ = Box::from_raw(data_ptr as *mut Vec<String>);
            }
            let callback_ptr: *mut c_void = *(*source).get_ivar(CALLBACK_IVAR);
            if !callback_ptr.is_null() {
                let _ = Box::from_raw(callback_ptr as *mut Box<dyn Fn((usize, String))>);
            }
            let _: () = msg_send![source, release];
        }
    }
}

pub(crate) unsafe fn release_native_outline_view(table: id) {
    unsafe {
        if !table.is_null() {
            let _: () = msg_send![table, release];
        }
    }
}
