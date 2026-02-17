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
use std::ffi::c_void;

const COLLECTION_ITEM_IDENTIFIER: &str = "GPUINativeCollectionItem";
const ITEM_CARD_TAG: i64 = 1001;
const ITEM_LABEL_TAG: i64 = 1002;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeCollectionItemStyleData {
    Label,
    Card,
}

struct CollectionCallbacks {
    items: Vec<String>,
    selected: Option<usize>,
    item_style: NativeCollectionItemStyleData,
    on_select: Option<Box<dyn Fn(usize)>>,
}

static mut COLLECTION_DELEGATE_CLASS: *const Class = std::ptr::null();

#[ctor]
unsafe fn build_collection_delegate_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUINativeCollectionDelegate", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);

        decl.add_method(
            sel!(collectionView:numberOfItemsInSection:),
            collection_number_of_items as extern "C" fn(&Object, Sel, id, i64) -> i64,
        );
        decl.add_method(
            sel!(collectionView:itemForRepresentedObjectAtIndexPath:),
            collection_item_for_index_path as extern "C" fn(&Object, Sel, id, id) -> id,
        );
        decl.add_method(
            sel!(collectionView:didSelectItemsAtIndexPaths:),
            collection_did_select_items_at_index_paths as extern "C" fn(&Object, Sel, id, id),
        );

        COLLECTION_DELEGATE_CLASS = decl.register();
    }
}

extern "C" fn collection_number_of_items(
    this: &Object,
    _sel: Sel,
    _view: id,
    _section: i64,
) -> i64 {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if ptr.is_null() {
            return 0;
        }
        let callbacks = &*(ptr as *const CollectionCallbacks);
        callbacks.items.len() as i64
    }
}

unsafe fn find_subview_with_tag(parent: id, tag: i64) -> id {
    unsafe { msg_send![parent, viewWithTag: tag] }
}

unsafe fn ensure_collection_item_view(item: id) -> id {
    unsafe {
        let current_view: id = msg_send![item, view];
        if current_view != nil {
            return current_view;
        }

        let view: id = msg_send![class!(NSView), alloc];
        let view: id = msg_send![view, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(160.0, 72.0),
        )];
        let _: () = msg_send![view, setAutoresizingMask: 0u64];

        let _: () = msg_send![item, setView: view];
        let _: () = msg_send![view, release];

        view
    }
}

unsafe fn ensure_label(parent: id, tag: i64) -> id {
    unsafe {
        let existing = find_subview_with_tag(parent, tag);
        if existing != nil {
            return existing;
        }

        let label: id =
            msg_send![class!(NSTextField), labelWithString: super::super::ns_string("")];
        let _: () = msg_send![label, setTag: tag];
        // NSTextAlignmentCenter = 2
        let _: () = msg_send![label, setAlignment: 2u64];
        let _: () = msg_send![label, setAutoresizingMask: 18u64]; // width + height sizable
        let _: () = msg_send![parent, addSubview: label];

        label
    }
}

unsafe fn ensure_card_field(parent: id, tag: i64) -> id {
    unsafe {
        let existing = find_subview_with_tag(parent, tag);
        if existing != nil {
            return existing;
        }

        let field: id = msg_send![class!(NSTextField), alloc];
        let field: id = msg_send![field, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(160.0, 72.0),
        )];
        let _: () = msg_send![field, setTag: tag];
        let _: () = msg_send![field, setEditable: 0i8];
        let _: () = msg_send![field, setSelectable: 0i8];
        let _: () = msg_send![field, setBezeled: 1i8];
        let _: () = msg_send![field, setBordered: 1i8];
        let _: () = msg_send![field, setDrawsBackground: 1i8];
        let _: () = msg_send![field, setAlignment: 2u64];
        let _: () = msg_send![field, setAutoresizingMask: 18u64];
        let _: () = msg_send![parent, addSubview: field];
        let _: () = msg_send![field, release];

        field
    }
}

unsafe fn configure_collection_item_label(item: id, item_view: id, title: &str, selected: bool) {
    unsafe {
        let card = find_subview_with_tag(item_view, ITEM_CARD_TAG);
        if card != nil {
            let _: () = msg_send![card, setHidden: 1i8];
        }

        let label = ensure_label(item_view, ITEM_LABEL_TAG);
        let _: () = msg_send![label, setHidden: 0i8];

        let bounds: NSRect = msg_send![item_view, bounds];
        let _: () = msg_send![label, setFrame: bounds];
        let _: () = msg_send![label, setStringValue: super::super::ns_string(title)];

        let color: id = if selected {
            msg_send![class!(NSColor), selectedTextColor]
        } else {
            msg_send![class!(NSColor), labelColor]
        };
        let _: () = msg_send![label, setTextColor: color];

        let _: () = msg_send![item, setTextField: label];
    }
}

unsafe fn configure_collection_item_card(item: id, item_view: id, title: &str, selected: bool) {
    unsafe {
        let plain_label = find_subview_with_tag(item_view, ITEM_LABEL_TAG);
        if plain_label != nil {
            let _: () = msg_send![plain_label, setHidden: 1i8];
        }

        let card_field = ensure_card_field(item_view, ITEM_CARD_TAG);
        let _: () = msg_send![card_field, setHidden: 0i8];

        let bounds: NSRect = msg_send![item_view, bounds];
        let frame = NSRect::new(
            NSPoint::new(4.0, 4.0),
            NSSize::new(
                (bounds.size.width - 8.0).max(1.0),
                (bounds.size.height - 8.0).max(1.0),
            ),
        );
        let _: () = msg_send![card_field, setFrame: frame];
        let _: () = msg_send![card_field, setStringValue: super::super::ns_string(title)];

        let (bg, text): (id, id) = if selected {
            (
                msg_send![class!(NSColor), selectedControlColor],
                msg_send![class!(NSColor), alternateSelectedControlTextColor],
            )
        } else {
            (
                msg_send![class!(NSColor), controlBackgroundColor],
                msg_send![class!(NSColor), labelColor],
            )
        };
        let _: () = msg_send![card_field, setBackgroundColor: bg];
        let _: () = msg_send![card_field, setTextColor: text];
        let _: () = msg_send![item, setTextField: card_field];
    }
}

extern "C" fn collection_item_for_index_path(
    this: &Object,
    _sel: Sel,
    collection_view: id,
    index_path: id,
) -> id {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if ptr.is_null() {
            return nil;
        }

        let identifier = super::super::ns_string(COLLECTION_ITEM_IDENTIFIER);
        let item: id = msg_send![
            collection_view,
            makeItemWithIdentifier: identifier
            forIndexPath: index_path
        ];
        if item == nil {
            return nil;
        }

        let callbacks = &*(ptr as *const CollectionCallbacks);
        let index: i64 = msg_send![index_path, item];
        if index < 0 || (index as usize) >= callbacks.items.len() {
            return item;
        }

        let item_view = ensure_collection_item_view(item);
        let title = &callbacks.items[index as usize];
        let selected = callbacks.selected == Some(index as usize);
        match callbacks.item_style {
            NativeCollectionItemStyleData::Label => {
                configure_collection_item_label(item, item_view, title, selected)
            }
            NativeCollectionItemStyleData::Card => {
                configure_collection_item_card(item, item_view, title, selected)
            }
        }

        item
    }
}

extern "C" fn collection_did_select_items_at_index_paths(
    this: &Object,
    _sel: Sel,
    _collection_view: id,
    index_paths: id,
) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if ptr.is_null() {
            return;
        }
        let callbacks = &*(ptr as *const CollectionCallbacks);
        if let Some(ref on_select) = callbacks.on_select {
            let any_path: id = msg_send![index_paths, anyObject];
            if any_path != nil {
                let index: i64 = msg_send![any_path, item];
                if index >= 0 {
                    on_select(index as usize);
                }
            }
        }
    }
}

unsafe fn flow_layout_from_collection(collection: id) -> id {
    unsafe {
        let layout: id = msg_send![collection, collectionViewLayout];
        let flow_layout_class = class!(NSCollectionViewFlowLayout);
        let is_flow: i8 = msg_send![layout, isKindOfClass: flow_layout_class];
        if is_flow != 0 { layout } else { nil }
    }
}

unsafe fn set_flow_layout_spacing(layout: id, spacing: f64) {
    unsafe {
        let _: () = msg_send![layout, setMinimumInteritemSpacing: spacing];
        let _: () = msg_send![layout, setMinimumLineSpacing: spacing];
    }
}

unsafe fn set_flow_layout_item_size(layout: id, size: NSSize) {
    unsafe {
        let _: () = msg_send![layout, setItemSize: size];
    }
}

unsafe fn set_flow_layout_scroll_direction(layout: id, direction: i64) {
    unsafe {
        let _: () = msg_send![layout, setScrollDirection: direction];
    }
}

pub(crate) unsafe fn create_native_collection_view() -> id {
    unsafe {
        let layout: id = msg_send![class!(NSCollectionViewFlowLayout), alloc];
        let layout: id = msg_send![layout, init];
        set_flow_layout_scroll_direction(layout, 0i64);
        set_flow_layout_spacing(layout, 8.0);

        let collection: id = msg_send![class!(NSCollectionView), alloc];
        let collection: id = msg_send![collection, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(320.0, 200.0),
        )];
        let _: () = msg_send![collection, setCollectionViewLayout: layout];
        let _: () = msg_send![collection, setSelectable: 1i8];
        let _: () = msg_send![collection, setAllowsEmptySelection: 1i8];
        let _: () = msg_send![collection, setAllowsMultipleSelection: 0i8];
        let _: () = msg_send![collection, setAutoresizingMask: 0u64];

        let identifier = super::super::ns_string(COLLECTION_ITEM_IDENTIFIER);
        let _: () = msg_send![collection, registerClass: class!(NSCollectionViewItem) forItemWithIdentifier: identifier];

        let scroll: id = msg_send![class!(NSScrollView), alloc];
        let scroll: id = msg_send![scroll, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(320.0, 200.0),
        )];
        let _: () = msg_send![scroll, setHasVerticalScroller: 1i8];
        let _: () = msg_send![scroll, setHasHorizontalScroller: 0i8];
        let _: () = msg_send![scroll, setBorderType: 1u64];
        let _: () = msg_send![scroll, setDocumentView: collection];
        let _: () = msg_send![scroll, setAutoresizingMask: 0u64];

        let _: () = msg_send![layout, release];
        scroll
    }
}

pub(crate) unsafe fn set_native_collection_layout(
    scroll_view: id,
    width: f64,
    columns: usize,
    item_height: f64,
    spacing: f64,
) {
    unsafe {
        let collection = collection_from_scroll(scroll_view);
        let layout = flow_layout_from_collection(collection);
        if layout == nil {
            return;
        }

        let columns = columns.max(1) as f64;
        let spacing = spacing.max(0.0);
        // Reserve conservative horizontal padding to avoid invalid-size warnings.
        let usable_width = (width - 24.0).max(80.0);
        let total_spacing = spacing * (columns - 1.0);
        let item_width = ((usable_width - total_spacing) / columns).max(80.0);

        set_flow_layout_spacing(layout, spacing);

        let size = NSSize::new(item_width, item_height.max(48.0));
        set_flow_layout_item_size(layout, size);
    }
}

unsafe fn collection_from_scroll(scroll_view: id) -> id {
    unsafe { msg_send![scroll_view, documentView] }
}

unsafe fn apply_collection_selected(collection: id, selected: Option<usize>, len: usize) {
    unsafe {
        let index_paths: id = msg_send![class!(NSMutableSet), set];

        if let Some(index) = selected {
            if index < len {
                let index_path: id =
                    msg_send![class!(NSIndexPath), indexPathForItem: index as i64 inSection: 0i64];
                let _: () = msg_send![index_paths, addObject: index_path];
            }
        }

        let _: () = msg_send![collection, setSelectionIndexPaths: index_paths];
    }
}

pub(crate) unsafe fn set_native_collection_data_source(
    scroll_view: id,
    items: &[&str],
    selected: Option<usize>,
    item_style: NativeCollectionItemStyleData,
    on_select: Option<Box<dyn Fn(usize)>>,
) -> *mut c_void {
    unsafe {
        let collection = collection_from_scroll(scroll_view);

        let delegate: id = msg_send![COLLECTION_DELEGATE_CLASS, alloc];
        let delegate: id = msg_send![delegate, init];

        let callbacks = CollectionCallbacks {
            items: items.iter().map(|item| item.to_string()).collect(),
            selected,
            item_style,
            on_select,
        };

        let callbacks_ptr = Box::into_raw(Box::new(callbacks)) as *mut c_void;
        (*delegate).set_ivar::<*mut c_void>(CALLBACK_IVAR, callbacks_ptr);

        let _: () = msg_send![collection, setDataSource: delegate];
        let _: () = msg_send![collection, setDelegate: delegate];
        let _: () = msg_send![collection, reloadData];

        apply_collection_selected(collection, selected, items.len());

        delegate as *mut c_void
    }
}

pub(crate) unsafe fn release_native_collection_target(target: *mut c_void) {
    unsafe {
        if target.is_null() {
            return;
        }

        let delegate = target as id;
        let callbacks_ptr: *mut c_void = *(*delegate).get_ivar(CALLBACK_IVAR);
        if !callbacks_ptr.is_null() {
            let _ = Box::from_raw(callbacks_ptr as *mut CollectionCallbacks);
        }
        let _: () = msg_send![delegate, release];
    }
}

pub(crate) unsafe fn release_native_collection_view(scroll_view: id) {
    unsafe {
        let collection = collection_from_scroll(scroll_view);
        let _: () = msg_send![collection, setDataSource: nil];
        let _: () = msg_send![collection, setDelegate: nil];
        let _: () = msg_send![scroll_view, release];
    }
}
