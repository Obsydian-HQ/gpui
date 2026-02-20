use super::CALLBACK_IVAR;
use cocoa::{
    base::id,
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

/// Delegate callbacks stored on NSComboBox delegate target.
pub(crate) struct ComboBoxCallbacks {
    pub on_select: Option<Box<dyn Fn(usize)>>,
    pub on_change: Option<Box<dyn Fn(String)>>,
    pub on_submit: Option<Box<dyn Fn(String)>>,
}

static mut COMBO_BOX_DELEGATE_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_combo_box_delegate_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUINativeComboBoxDelegate", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);

        decl.add_method(
            sel!(comboBoxSelectionDidChange:),
            combo_box_selection_did_change as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(controlTextDidChange:),
            combo_box_text_did_change as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(control:textView:doCommandBySelector:),
            combo_box_do_command as extern "C" fn(&Object, Sel, id, id, Sel) -> i8,
        );

        COMBO_BOX_DELEGATE_CLASS = decl.register();
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

extern "C" fn combo_box_selection_did_change(this: &Object, _sel: Sel, notification: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if !ptr.is_null() {
            let callbacks = &*(ptr as *const ComboBoxCallbacks);
            if let Some(ref on_select) = callbacks.on_select {
                let combo_box: id = msg_send![notification, object];
                let selected: i64 = msg_send![combo_box, indexOfSelectedItem];
                if selected >= 0 {
                    on_select(selected as usize);
                }
            }
        }
    }
}

extern "C" fn combo_box_text_did_change(this: &Object, _sel: Sel, notification: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if !ptr.is_null() {
            let callbacks = &*(ptr as *const ComboBoxCallbacks);
            if let Some(ref on_change) = callbacks.on_change {
                let combo_box: id = msg_send![notification, object];
                let ns_str: id = msg_send![combo_box, stringValue];
                on_change(string_from_ns_string(ns_str));
            }
        }
    }
}

extern "C" fn combo_box_do_command(
    this: &Object,
    _sel: Sel,
    control: id,
    _text_view: id,
    command_selector: Sel,
) -> i8 {
    unsafe {
        if command_selector == sel!(insertNewline:) {
            let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
            if !ptr.is_null() {
                let callbacks = &*(ptr as *const ComboBoxCallbacks);
                if let Some(ref on_submit) = callbacks.on_submit {
                    let ns_str: id = msg_send![control, stringValue];
                    let text = string_from_ns_string(ns_str);
                    on_submit(text);
                    return 1; // Handled
                }
            }
        }
        0
    }
}

/// Creates a new NSComboBox with optional initial items.
pub(crate) unsafe fn create_native_combo_box(
    items: &[&str],
    selected_index: usize,
    editable: bool,
) -> id {
    unsafe {
        let combo: id = msg_send![class!(NSComboBox), alloc];
        let combo: id = msg_send![combo, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(220.0, 26.0),
        )];
        let _: () = msg_send![combo, setAutoresizingMask: 0u64];
        let _: () = msg_send![combo, setUsesDataSource: 0i8];
        let _: () = msg_send![combo, setHasVerticalScroller: 1i8];
        let _: () = msg_send![combo, setNumberOfVisibleItems: 12i64];
        set_native_combo_box_editable(combo, editable);
        set_native_combo_box_items(combo, items);
        if !items.is_empty() {
            set_native_combo_box_selected(combo, selected_index.min(items.len() - 1));
        }
        combo
    }
}

/// Sets all combo box items, replacing existing ones.
pub(crate) unsafe fn set_native_combo_box_items(combo: id, items: &[&str]) {
    unsafe {
        use super::super::ns_string;
        let _: () = msg_send![combo, removeAllItems];
        for item in items {
            let _: () = msg_send![combo, addItemWithObjectValue: ns_string(item)];
        }
    }
}

/// Selects an item by index.
pub(crate) unsafe fn set_native_combo_box_selected(combo: id, index: usize) {
    unsafe {
        let _: () = msg_send![combo, selectItemAtIndex: index as i64];
    }
}

/// Sets the current text value.
pub(crate) unsafe fn set_native_combo_box_string_value(combo: id, value: &str) {
    unsafe {
        use super::super::ns_string;
        let _: () = msg_send![combo, setStringValue: ns_string(value)];
    }
}

/// Gets the current text value.
pub(crate) unsafe fn get_native_combo_box_string_value(combo: id) -> String {
    unsafe {
        let ns_str: id = msg_send![combo, stringValue];
        string_from_ns_string(ns_str)
    }
}

/// Sets whether combo box is editable.
pub(crate) unsafe fn set_native_combo_box_editable(combo: id, editable: bool) {
    unsafe {
        let flag = editable as i8;
        let _: () = msg_send![combo, setEditable: flag];
        let _: () = msg_send![combo, setSelectable: flag];
        let _: () = msg_send![combo, setDrawsBackground: flag];
    }
}

/// Sets whether combo box autocompletes while typing.
pub(crate) unsafe fn set_native_combo_box_completes(combo: id, completes: bool) {
    unsafe {
        let _: () = msg_send![combo, setCompletes: completes as i8];
    }
}

/// Sets delegate callbacks for the combo box. Returns delegate ptr.
pub(crate) unsafe fn set_native_combo_box_delegate(
    combo: id,
    callbacks: ComboBoxCallbacks,
) -> *mut c_void {
    unsafe {
        let delegate: id = msg_send![COMBO_BOX_DELEGATE_CLASS, alloc];
        let delegate: id = msg_send![delegate, init];

        let callbacks_ptr = Box::into_raw(Box::new(callbacks)) as *mut c_void;
        (*delegate).set_ivar::<*mut c_void>(CALLBACK_IVAR, callbacks_ptr);

        let _: () = msg_send![combo, setDelegate: delegate];

        delegate as *mut c_void
    }
}

/// Releases combo box delegate and stored callbacks.
pub(crate) unsafe fn release_native_combo_box_delegate(delegate_ptr: *mut c_void) {
    unsafe {
        if !delegate_ptr.is_null() {
            let delegate = delegate_ptr as id;
            let callbacks_ptr: *mut c_void = *(*delegate).get_ivar(CALLBACK_IVAR);
            if !callbacks_ptr.is_null() {
                let _ = Box::from_raw(callbacks_ptr as *mut ComboBoxCallbacks);
            }
            let _: () = msg_send![delegate, release];
        }
    }
}

/// Releases an NSComboBox.
pub(crate) unsafe fn release_native_combo_box(combo: id) {
    unsafe {
        let _: () = msg_send![combo, release];
    }
}
