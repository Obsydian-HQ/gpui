// UICollectionView wrapper for iOS (equivalent to NSCollectionView on macOS).
// Simplified implementation with a flow layout.

use super::{id, ns_string, CALLBACK_IVAR};
use ctor::ctor;
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Sel},
    sel, sel_impl,
};
use std::{ffi::c_void, ptr};

/// Item data for the collection.
pub(crate) struct IosCollectionItem {
    pub text: String,
}

const DATA_IVAR: &str = "dataPtr";
const CELL_ID: &str = "GPUICollectionCell";

static mut COLLECTION_DATA_SOURCE_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_collection_data_source_class() {
    unsafe {
        let mut decl =
            ClassDecl::new("GPUIiOSCollectionDataSource", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>(DATA_IVAR);
        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);

        decl.add_method(
            sel!(collectionView:numberOfItemsInSection:),
            number_of_items as extern "C" fn(&Object, Sel, id, isize) -> isize,
        );
        decl.add_method(
            sel!(collectionView:cellForItemAtIndexPath:),
            cell_for_item as extern "C" fn(&Object, Sel, id, id) -> id,
        );
        decl.add_method(
            sel!(collectionView:didSelectItemAtIndexPath:),
            did_select_item as extern "C" fn(&Object, Sel, id, id),
        );

        COLLECTION_DATA_SOURCE_CLASS = decl.register();
    }
}

extern "C" fn number_of_items(this: &Object, _sel: Sel, _cv: id, _section: isize) -> isize {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(DATA_IVAR);
        if ptr.is_null() {
            return 0;
        }
        let items = &*(ptr as *const Vec<IosCollectionItem>);
        items.len() as isize
    }
}

extern "C" fn cell_for_item(_this: &Object, _sel: Sel, cv: id, index_path: id) -> id {
    unsafe {
        let cell: id = msg_send![cv,
            dequeueReusableCellWithReuseIdentifier: ns_string(CELL_ID)
            forIndexPath: index_path
        ];
        let row_index: isize = msg_send![index_path, item];
        let data_source: id = msg_send![cv, dataSource];
        if !data_source.is_null() {
            let ptr: *mut c_void = *(*data_source).get_ivar(DATA_IVAR);
            if !ptr.is_null() {
                let items = &*(ptr as *const Vec<IosCollectionItem>);
                if let Some(item) = items.get(row_index as usize) {
                    let _: () = msg_send![cell, setAccessibilityLabel: ns_string(&item.text)];
                }
            }
        }
        cell
    }
}

extern "C" fn did_select_item(this: &Object, _sel: Sel, _cv: id, index_path: id) {
    unsafe {
        let callback_ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if callback_ptr.is_null() {
            return;
        }

        let item_index: isize = msg_send![index_path, item];
        let callback = &*(callback_ptr as *const Box<dyn Fn(usize)>);
        callback(item_index as usize);
    }
}

/// Creates a UICollectionView with a flow layout.
pub(crate) unsafe fn create_native_collection_view() -> id {
    unsafe {
        let layout: id = msg_send![class!(UICollectionViewFlowLayout), alloc];
        let layout: id = msg_send![layout, init];

        let cv: id = msg_send![class!(UICollectionView), alloc];
        let cv: id = msg_send![cv, initWithFrame:
            ((0.0f64, 0.0f64), (320.0f64, 480.0f64))
            collectionViewLayout: layout
        ];

        // Register a basic cell class
        let _: () = msg_send![cv,
            registerClass: class!(UICollectionViewCell)
            forCellWithReuseIdentifier: ns_string(CELL_ID)
        ];

        let _: () = msg_send![layout, release];
        cv
    }
}

/// Configures the collection layout (item size, spacing).
pub(crate) unsafe fn set_native_collection_layout(
    cv: id,
    item_width: f64,
    item_height: f64,
    spacing: f64,
) {
    unsafe {
        let layout: id = msg_send![cv, collectionViewLayout];
        let size: (f64, f64) = (item_width, item_height);
        let _: () = msg_send![layout, setItemSize: size];
        let _: () = msg_send![layout, setMinimumInteritemSpacing: spacing];
        let _: () = msg_send![layout, setMinimumLineSpacing: spacing];
    }
}

/// Sets the collection data source.
pub(crate) unsafe fn set_native_collection_data_source(
    cv: id,
    items: Vec<IosCollectionItem>,
    on_select: Option<Box<dyn Fn(usize)>>,
    selected: Option<usize>,
) -> *mut c_void {
    unsafe {
        let source: id = msg_send![COLLECTION_DATA_SOURCE_CLASS, alloc];
        let source: id = msg_send![source, init];

        let data = Box::into_raw(Box::new(items)) as *mut c_void;
        (*source).set_ivar::<*mut c_void>(DATA_IVAR, data);
        let callback_ptr = on_select
            .map(|callback| Box::into_raw(Box::new(callback)) as *mut c_void)
            .unwrap_or(std::ptr::null_mut());
        (*source).set_ivar::<*mut c_void>(CALLBACK_IVAR, callback_ptr);

        let _: () = msg_send![cv, setDataSource: source];
        let _: () = msg_send![cv, setDelegate: source];
        let _: () = msg_send![cv, reloadData];

        if let Some(selected) = selected {
            let index_path: id = msg_send![class!(NSIndexPath),
                indexPathForItem: selected as isize
                inSection: 0isize
            ];
            let _: () = msg_send![cv,
                selectItemAtIndexPath: index_path
                animated: false as i8
                scrollPosition: 0u64
            ];
        }

        source as *mut c_void
    }
}

/// Releases the collection data source.
pub(crate) unsafe fn release_native_collection_target(target: *mut c_void) {
    unsafe {
        if !target.is_null() {
            let source = target as id;
            let data_ptr: *mut c_void = *(*source).get_ivar(DATA_IVAR);
            if !data_ptr.is_null() {
                let _ = Box::from_raw(data_ptr as *mut Vec<IosCollectionItem>);
            }
            let callback_ptr: *mut c_void = *(*source).get_ivar(CALLBACK_IVAR);
            if !callback_ptr.is_null() {
                let _ = Box::from_raw(callback_ptr as *mut Box<dyn Fn(usize)>);
            }
            let _: () = msg_send![source, release];
        }
    }
}

/// Releases a UICollectionView.
pub(crate) unsafe fn release_native_collection_view(cv: id) {
    unsafe {
        if !cv.is_null() {
            let _: () = msg_send![cv, release];
        }
    }
}
