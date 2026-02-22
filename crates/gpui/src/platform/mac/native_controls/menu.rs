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
use std::{cell::Cell, ffi::c_void, ptr, rc::Rc};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum NativeMenuItemData {
    Action {
        title: String,
        enabled: bool,
        icon: Option<String>,
    },
    Submenu {
        title: String,
        enabled: bool,
        icon: Option<String>,
        items: Vec<NativeMenuItemData>,
    },
    Separator,
}

struct MenuCallbacks {
    menu: id,
    on_select: Option<Box<dyn Fn(usize)>>,
}

impl Drop for MenuCallbacks {
    fn drop(&mut self) {
        unsafe {
            if self.menu != nil {
                let _: () = msg_send![self.menu, release];
            }
        }
    }
}

static mut MENU_TARGET_CLASS: *const Class = ptr::null();
static mut CONTEXT_BUTTON_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_menu_classes() {
    unsafe {
        let mut target_decl = ClassDecl::new("GPUINativeMenuTarget", class!(NSObject)).unwrap();
        target_decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);
        target_decl.add_method(
            sel!(menuButtonAction:),
            menu_button_action as extern "C" fn(&Object, Sel, id),
        );
        target_decl.add_method(
            sel!(menuItemAction:),
            menu_item_action as extern "C" fn(&Object, Sel, id),
        );
        MENU_TARGET_CLASS = target_decl.register();
    }

    unsafe {
        let mut context_decl =
            ClassDecl::new("GPUINativeContextMenuButton", class!(NSButton)).unwrap();
        context_decl.add_method(
            sel!(rightMouseDown:),
            context_right_mouse_down as extern "C" fn(&Object, Sel, id),
        );
        CONTEXT_BUTTON_CLASS = context_decl.register();
    }
}

extern "C" fn menu_button_action(this: &Object, _sel: Sel, sender: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if ptr.is_null() {
            return;
        }
        let callbacks = &*(ptr as *const MenuCallbacks);

        let point = NSPoint::new(0.0, 0.0);
        let _: i8 = msg_send![
            callbacks.menu,
            popUpMenuPositioningItem: nil
            atLocation: point
            inView: sender
        ];
    }
}

extern "C" fn menu_item_action(this: &Object, _sel: Sel, sender: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if ptr.is_null() {
            return;
        }
        let callbacks = &*(ptr as *const MenuCallbacks);
        if let Some(ref on_select) = callbacks.on_select {
            let tag: i64 = msg_send![sender, tag];
            if tag >= 0 {
                on_select(tag as usize);
            }
        }
    }
}

extern "C" fn context_right_mouse_down(this: &Object, _sel: Sel, _event: id) {
    unsafe {
        let action: Sel = msg_send![this, action];
        let target: id = msg_send![this, target];
        let _: () = msg_send![this, sendAction: action to: target];
    }
}

unsafe fn build_menu(
    title: &str,
    items: &[NativeMenuItemData],
    target: id,
    next_action_index: &mut usize,
) -> id {
    unsafe {
        use super::super::ns_string;

        let menu: id = msg_send![class!(NSMenu), alloc];
        let menu: id = msg_send![menu, initWithTitle: ns_string(title)];
        // Keep menu item enabled state explicit; avoid AppKit auto-validation
        // disabling submenu items without a target/action.
        let _: () = msg_send![menu, setAutoenablesItems: 0i8];

        for item in items {
            match item {
                NativeMenuItemData::Separator => {
                    let sep: id = msg_send![class!(NSMenuItem), separatorItem];
                    let _: () = msg_send![menu, addItem: sep];
                }
                NativeMenuItemData::Action {
                    title,
                    enabled,
                    icon,
                } => {
                    let menu_item: id = msg_send![class!(NSMenuItem), alloc];
                    let menu_item: id = msg_send![
                        menu_item,
                        initWithTitle: ns_string(title)
                        action: sel!(menuItemAction:)
                        keyEquivalent: ns_string("")
                    ];
                    let _: () = msg_send![menu_item, setTag: *next_action_index as i64];
                    let _: () = msg_send![menu_item, setTarget: target];
                    let _: () = msg_send![menu_item, setEnabled: *enabled as i8];
                    if let Some(icon) = icon {
                        let symbol_name = ns_string(icon);
                        let image: id = msg_send![
                            class!(NSImage),
                            imageWithSystemSymbolName: symbol_name
                            accessibilityDescription: nil
                        ];
                        if image != nil {
                            let _: () = msg_send![menu_item, setImage: image];
                        }
                    }
                    let _: () = msg_send![menu, addItem: menu_item];
                    let _: () = msg_send![menu_item, release];
                    *next_action_index += 1;
                }
                NativeMenuItemData::Submenu {
                    title,
                    enabled,
                    icon,
                    items,
                } => {
                    let submenu = build_menu(title, items, target, next_action_index);

                    let parent_item: id = msg_send![class!(NSMenuItem), alloc];
                    let null_sel: Sel = std::mem::transmute(0usize);
                    let parent_item: id = msg_send![
                        parent_item,
                        initWithTitle: ns_string(title)
                        action: null_sel
                        keyEquivalent: ns_string("")
                    ];
                    let _: () = msg_send![parent_item, setEnabled: *enabled as i8];
                    let _: () = msg_send![parent_item, setTarget: nil];
                    let _: () = msg_send![parent_item, setSubmenu: submenu];
                    if let Some(icon) = icon {
                        let symbol_name = ns_string(icon);
                        let image: id = msg_send![
                            class!(NSImage),
                            imageWithSystemSymbolName: symbol_name
                            accessibilityDescription: nil
                        ];
                        if image != nil {
                            let _: () = msg_send![parent_item, setImage: image];
                        }
                    }
                    let _: () = msg_send![menu, addItem: parent_item];

                    let _: () = msg_send![submenu, release];
                    let _: () = msg_send![parent_item, release];
                }
            }
        }

        menu
    }
}

pub(crate) unsafe fn create_native_menu_button(title: &str) -> id {
    unsafe {
        use super::super::ns_string;

        let button: id = msg_send![class!(NSButton), alloc];
        let button: id = msg_send![button, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(140.0, 24.0),
        )];
        let _: () = msg_send![button, setTitle: ns_string(title)];
        let _: () = msg_send![button, setBezelStyle: 1i64];
        let _: () = msg_send![button, setAutoresizingMask: 0u64];
        button
    }
}

pub(crate) unsafe fn create_native_context_menu_button(title: &str) -> id {
    unsafe {
        use super::super::ns_string;

        let button: id = msg_send![CONTEXT_BUTTON_CLASS, alloc];
        let button: id = msg_send![button, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(180.0, 26.0),
        )];
        let _: () = msg_send![button, setTitle: ns_string(title)];
        let _: () = msg_send![button, setBezelStyle: 1i64];
        let _: () = msg_send![button, setAutoresizingMask: 0u64];
        button
    }
}

pub(crate) unsafe fn set_native_menu_button_title(button: id, title: &str) {
    unsafe {
        use super::super::ns_string;
        let _: () = msg_send![button, setTitle: ns_string(title)];
    }
}

pub(crate) unsafe fn set_native_menu_button_items(
    button: id,
    items: &[NativeMenuItemData],
    on_select: Option<Box<dyn Fn(usize)>>,
) -> *mut c_void {
    unsafe {
        let target: id = msg_send![MENU_TARGET_CLASS, alloc];
        let target: id = msg_send![target, init];

        let mut next_action_index = 0;
        let menu = build_menu("menu", items, target, &mut next_action_index);

        let callbacks = MenuCallbacks { menu, on_select };
        let callbacks_ptr = Box::into_raw(Box::new(callbacks)) as *mut c_void;
        (*target).set_ivar::<*mut c_void>(CALLBACK_IVAR, callbacks_ptr);

        let _: () = msg_send![button, setTarget: target];
        let _: () = msg_send![button, setAction: sel!(menuButtonAction:)];

        target as *mut c_void
    }
}

/// Creates a GPUINativeMenuTarget with an NSMenu built from the given items.
/// Returns `(menu, target_ptr)`. The caller owns the menu (retained) and
/// must eventually call `release_native_menu_button_target` on the target.
pub(crate) unsafe fn create_native_menu_target(
    items: &[NativeMenuItemData],
    on_select: Option<Box<dyn Fn(usize)>>,
) -> (id, *mut c_void) {
    unsafe {
        let target: id = msg_send![MENU_TARGET_CLASS, alloc];
        let target: id = msg_send![target, init];

        let mut next_action_index = 0;
        let menu = build_menu("menu", items, target, &mut next_action_index);

        let callbacks = MenuCallbacks { menu, on_select };
        let callbacks_ptr = Box::into_raw(Box::new(callbacks)) as *mut c_void;
        (*target).set_ivar::<*mut c_void>(CALLBACK_IVAR, callbacks_ptr);

        // Retain the menu for the caller (MenuCallbacks also holds a reference)
        let _: () = msg_send![menu, retain];

        (menu, target as *mut c_void)
    }
}

pub(crate) unsafe fn release_native_menu_button_target(target: *mut c_void) {
    unsafe {
        if target.is_null() {
            return;
        }

        let target = target as id;
        let callbacks_ptr: *mut c_void = *(*target).get_ivar(CALLBACK_IVAR);
        if !callbacks_ptr.is_null() {
            let _ = Box::from_raw(callbacks_ptr as *mut MenuCallbacks);
        }

        let _: () = msg_send![target, release];
    }
}

pub(crate) unsafe fn release_native_menu_button(button: id) {
    unsafe {
        let _: () = msg_send![button, release];
    }
}

struct DeferredPopupContext {
    menu: id,
    target_ptr: *mut c_void,
    view: id,
    gpui_x: f64,
    gpui_y: f64,
    selected_index: Rc<Cell<Option<usize>>>,
    on_result: Box<dyn FnOnce(Option<usize>)>,
}

/// Shows a popup context menu at the given pixel position, deferred to the
/// next main-queue iteration so it doesn't conflict with GPUI's active borrows.
///
/// `gpui_x` / `gpui_y` are in GPUI's top-down coordinate system. The
/// `on_result` callback fires after the menu closes with the selected action
/// index (or `None` if dismissed).
pub(crate) unsafe fn show_popup_menu_deferred(
    items: &[NativeMenuItemData],
    view: id,
    gpui_x: f64,
    gpui_y: f64,
    on_result: Box<dyn FnOnce(Option<usize>)>,
) {
    unsafe {
        let selected_index = Rc::new(Cell::new(None));
        let selected_clone = selected_index.clone();

        let (menu, target_ptr) = create_native_menu_target(
            items,
            Some(Box::new(move |idx| {
                selected_clone.set(Some(idx));
            })),
        );

        let context = Box::new(DeferredPopupContext {
            menu,
            target_ptr,
            view,
            gpui_x,
            gpui_y,
            selected_index,
            on_result,
        });

        let context_ptr = Box::into_raw(context) as *mut c_void;

        use super::super::dispatcher::{dispatch_get_main_queue, dispatch_sys::dispatch_async_f};
        dispatch_async_f(
            dispatch_get_main_queue(),
            context_ptr,
            Some(popup_menu_trampoline),
        );
    }
}

unsafe extern "C" fn popup_menu_trampoline(context: *mut c_void) {
    unsafe {
        let ctx = Box::from_raw(context as *mut DeferredPopupContext);

        let parent_frame: NSRect = msg_send![ctx.view, frame];
        let ns_point = NSPoint::new(ctx.gpui_x, parent_frame.size.height - ctx.gpui_y);

        let _: i8 = msg_send![
            ctx.menu,
            popUpMenuPositioningItem: nil
            atLocation: ns_point
            inView: ctx.view
        ];

        let result = ctx.selected_index.get();

        release_native_menu_button_target(ctx.target_ptr);
        let _: () = msg_send![ctx.menu, release];

        (ctx.on_result)(result);
    }
}
