use super::{
    create_native_button, create_native_segmented_control, create_native_switch,
    release_native_button, release_native_button_target, release_native_segmented_control,
    release_native_segmented_target, release_native_switch, release_native_switch_target,
    set_native_button_action, set_native_button_bordered, set_native_button_sf_symbol,
    set_native_segmented_action, set_native_segmented_image, set_native_switch_action,
    set_native_switch_state,
};
use crate::{
    WindowToolbarButtonOptions, WindowToolbarDisplayMode, WindowToolbarGroupControlRepresentation,
    WindowToolbarGroupOptions, WindowToolbarGroupSelectionMode, WindowToolbarItem,
    WindowToolbarItemIdentifier, WindowToolbarItemKind, WindowToolbarOptions,
    WindowToolbarSearchFieldOptions, WindowToolbarSegmentedControlOptions,
    WindowToolbarSwitchOptions, WindowToolbarTrackingSeparatorOptions,
};
use cocoa::{
    base::{id, nil},
    foundation::{NSInteger, NSRect, NSUInteger},
};
use ctor::ctor;
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Sel},
    sel, sel_impl,
};
use std::{
    collections::HashMap,
    ffi::{CStr, c_char, c_void},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

const TOOLBAR_RUNTIME_IVAR: &str = "toolbarRuntimePtr";
const CALLBACK_IVAR: &str = "callbackPtr";

static mut TOOLBAR_DELEGATE_CLASS: *const Class = ptr::null();
static mut TOOLBAR_GROUP_TARGET_CLASS: *const Class = ptr::null();
static mut TOOLBAR_SEARCH_TARGET_CLASS: *const Class = ptr::null();

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {
    static NSToolbarSpaceItemIdentifier: id;
    static NSToolbarFlexibleSpaceItemIdentifier: id;
    static NSToolbarSeparatorItemIdentifier: id;
    static NSToolbarToggleSidebarItemIdentifier: id;
    static NSToolbarSidebarTrackingSeparatorItemIdentifier: id;
}

#[ctor]
unsafe fn build_toolbar_classes() {
    unsafe {
        TOOLBAR_DELEGATE_CLASS = {
            let mut decl = ClassDecl::new("GPUINativeToolbarDelegate", class!(NSObject)).unwrap();
            decl.add_ivar::<*mut c_void>(TOOLBAR_RUNTIME_IVAR);

            decl.add_method(
                sel!(dealloc),
                toolbar_delegate_dealloc as extern "C" fn(&mut Object, Sel),
            );
            decl.add_method(
                sel!(toolbarAllowedItemIdentifiers:),
                toolbar_allowed_item_identifiers as extern "C" fn(&Object, Sel, id) -> id,
            );
            decl.add_method(
                sel!(toolbarDefaultItemIdentifiers:),
                toolbar_default_item_identifiers as extern "C" fn(&Object, Sel, id) -> id,
            );
            decl.add_method(
                sel!(toolbarSelectableItemIdentifiers:),
                toolbar_selectable_item_identifiers as extern "C" fn(&Object, Sel, id) -> id,
            );
            decl.add_method(
                sel!(toolbar:itemForItemIdentifier:willBeInsertedIntoToolbar:),
                toolbar_item_for_identifier as extern "C" fn(&Object, Sel, id, id, i8) -> id,
            );

            decl.register()
        };

        TOOLBAR_GROUP_TARGET_CLASS = {
            let mut decl =
                ClassDecl::new("GPUINativeToolbarGroupTarget", class!(NSObject)).unwrap();
            decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);
            decl.add_method(
                sel!(groupAction:),
                toolbar_group_action as extern "C" fn(&Object, Sel, id),
            );
            decl.register()
        };

        TOOLBAR_SEARCH_TARGET_CLASS = {
            let mut decl =
                ClassDecl::new("GPUINativeToolbarSearchTarget", class!(NSObject)).unwrap();
            decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);
            decl.add_method(
                sel!(searchAction:),
                toolbar_search_action as extern "C" fn(&Object, Sel, id),
            );
            decl.add_method(
                sel!(controlTextDidChange:),
                toolbar_search_text_did_change as extern "C" fn(&Object, Sel, id),
            );
            decl.register()
        };
    }
}

struct SearchCallbacks {
    on_change: Option<Box<dyn Fn(String) + 'static>>,
    on_submit: Option<Box<dyn Fn(String) + 'static>>,
}

extern "C" fn toolbar_group_action(this: &Object, _: Sel, sender: id) {
    unsafe {
        let callback_ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if callback_ptr.is_null() {
            return;
        }

        let callback = &*(callback_ptr as *const Box<dyn Fn(usize)>);
        let selected_index: NSInteger = msg_send![sender, selectedIndex];
        if selected_index >= 0 {
            callback(selected_index as usize);
        }
    }
}

extern "C" fn toolbar_search_action(this: &Object, _: Sel, sender: id) {
    unsafe {
        let callback_ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if callback_ptr.is_null() {
            return;
        }

        let callbacks = &*(callback_ptr as *const SearchCallbacks);
        if let Some(callback) = callbacks.on_submit.as_ref() {
            callback(control_string_value(sender));
        }
    }
}

extern "C" fn toolbar_search_text_did_change(this: &Object, _: Sel, notification: id) {
    unsafe {
        let callback_ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if callback_ptr.is_null() {
            return;
        }

        let callbacks = &*(callback_ptr as *const SearchCallbacks);
        let sender: id = msg_send![notification, object];
        if sender.is_null() {
            return;
        }

        if let Some(callback) = callbacks.on_change.as_ref() {
            callback(control_string_value(sender));
        }
    }
}

unsafe fn create_toolbar_group_target(callback: Box<dyn Fn(usize) + 'static>) -> *mut c_void {
    unsafe {
        let target: id = msg_send![TOOLBAR_GROUP_TARGET_CLASS, alloc];
        let target: id = msg_send![target, init];

        let callback_ptr = Box::into_raw(Box::new(callback)) as *mut c_void;
        (*target).set_ivar::<*mut c_void>(CALLBACK_IVAR, callback_ptr);
        target as *mut c_void
    }
}

unsafe fn release_toolbar_group_target(target: *mut c_void) {
    unsafe {
        if target.is_null() {
            return;
        }

        let target = target as id;
        let callback_ptr: *mut c_void = *(*target).get_ivar(CALLBACK_IVAR);
        if !callback_ptr.is_null() {
            let _ = Box::from_raw(callback_ptr as *mut Box<dyn Fn(usize)>);
        }
        let _: () = msg_send![target, release];
    }
}

unsafe fn create_toolbar_search_target(
    on_change: Option<Box<dyn Fn(String) + 'static>>,
    on_submit: Option<Box<dyn Fn(String) + 'static>>,
) -> *mut c_void {
    unsafe {
        let target: id = msg_send![TOOLBAR_SEARCH_TARGET_CLASS, alloc];
        let target: id = msg_send![target, init];

        let callbacks = Box::new(SearchCallbacks {
            on_change,
            on_submit,
        });
        let callback_ptr = Box::into_raw(callbacks) as *mut c_void;
        (*target).set_ivar::<*mut c_void>(CALLBACK_IVAR, callback_ptr);

        target as *mut c_void
    }
}

unsafe fn release_toolbar_search_target(target: *mut c_void) {
    unsafe {
        if target.is_null() {
            return;
        }

        let target = target as id;
        let callback_ptr: *mut c_void = *(*target).get_ivar(CALLBACK_IVAR);
        if !callback_ptr.is_null() {
            let _ = Box::from_raw(callback_ptr as *mut SearchCallbacks);
        }
        let _: () = msg_send![target, release];
    }
}

pub(crate) struct NativeToolbarState {
    toolbar: id,
    delegate: id,
}

impl Drop for NativeToolbarState {
    fn drop(&mut self) {
        unsafe {
            if !self.toolbar.is_null() {
                let _: () = msg_send![self.toolbar, setDelegate: nil];
            }
            if !self.delegate.is_null() {
                let _: () = msg_send![self.delegate, release];
            }
            if !self.toolbar.is_null() {
                let _: () = msg_send![self.toolbar, release];
            }
        }
    }
}

enum OwnedNativeControl {
    Button { control: id, target: *mut c_void },
    Segmented { control: id, target: *mut c_void },
    Switch { control: id, target: *mut c_void },
    GroupTarget { target: *mut c_void },
    SearchTarget { target: *mut c_void },
}

impl OwnedNativeControl {
    unsafe fn release(&mut self) {
        unsafe {
            match self {
                Self::Button { control, target } => {
                    release_native_button_target(*target);
                    release_native_button(*control);
                }
                Self::Segmented { control, target } => {
                    release_native_segmented_target(*target);
                    release_native_segmented_control(*control);
                }
                Self::Switch { control, target } => {
                    release_native_switch_target(*target);
                    release_native_switch(*control);
                }
                Self::GroupTarget { target } => {
                    release_toolbar_group_target(*target);
                }
                Self::SearchTarget { target } => {
                    release_toolbar_search_target(*target);
                }
            }
        }
    }
}

struct ToolbarRuntime {
    item_definitions: HashMap<String, WindowToolbarItem>,
    created_items: HashMap<String, id>,
    owned_native_controls: Vec<OwnedNativeControl>,
    allowed_item_identifiers: Vec<WindowToolbarItemIdentifier>,
    default_item_identifiers: Vec<WindowToolbarItemIdentifier>,
    selectable_item_identifiers: Vec<WindowToolbarItemIdentifier>,
}

impl Drop for ToolbarRuntime {
    fn drop(&mut self) {
        unsafe {
            for control in &mut self.owned_native_controls {
                control.release();
            }
            self.owned_native_controls.clear();

            for (_, item) in self.created_items.drain() {
                let _: () = msg_send![item, release];
            }
        }
    }
}

impl ToolbarRuntime {
    fn new(options: WindowToolbarOptions) -> Self {
        let mut item_definitions = HashMap::new();
        let mut custom_identifiers = Vec::with_capacity(options.items.len());

        for item in options.items {
            let identifier = item.identifier.to_string();
            custom_identifiers.push(WindowToolbarItemIdentifier::Custom(item.identifier.clone()));
            item_definitions.insert(identifier, item);
        }

        let mut default_item_identifiers = if options.default_item_identifiers.is_empty() {
            custom_identifiers.clone()
        } else {
            options.default_item_identifiers
        };

        if default_item_identifiers.is_empty() {
            default_item_identifiers = custom_identifiers.clone();
        }

        let mut allowed_item_identifiers = if options.allowed_item_identifiers.is_empty() {
            default_item_identifiers.clone()
        } else {
            options.allowed_item_identifiers
        };

        for identifier in custom_identifiers {
            push_unique_identifier(&mut allowed_item_identifiers, identifier);
        }

        for identifier in &default_item_identifiers {
            push_unique_identifier(&mut allowed_item_identifiers, identifier.clone());
        }

        let selectable_item_identifiers = options.selectable_item_identifiers;

        for identifier in &selectable_item_identifiers {
            push_unique_identifier(&mut allowed_item_identifiers, identifier.clone());
        }

        Self {
            item_definitions,
            created_items: HashMap::new(),
            owned_native_controls: Vec::new(),
            allowed_item_identifiers,
            default_item_identifiers,
            selectable_item_identifiers,
        }
    }

    unsafe fn allowed_item_identifiers_array(&self) -> id {
        unsafe { toolbar_item_identifiers_array(&self.allowed_item_identifiers) }
    }

    unsafe fn default_item_identifiers_array(&self) -> id {
        unsafe { toolbar_item_identifiers_array(&self.default_item_identifiers) }
    }

    unsafe fn selectable_item_identifiers_array(&self) -> id {
        unsafe { toolbar_item_identifiers_array(&self.selectable_item_identifiers) }
    }

    unsafe fn item_for_identifier(&mut self, identifier: id) -> id {
        unsafe {
            if is_standard_toolbar_identifier(identifier) {
                return nil;
            }

            let Some(identifier) = ns_string_to_rust(identifier) else {
                return nil;
            };

            if let Some(item) = self.created_items.get(&identifier) {
                return *item;
            }

            let Some(definition) = self.item_definitions.remove(&identifier) else {
                return self.fallback_item_for_identifier(&identifier);
            };

            let WindowToolbarItem {
                identifier: _,
                label,
                palette_label,
                tool_tip,
                kind,
            } = definition;

            let label = label.to_string();
            let palette_label = palette_label.map(|label| label.to_string());
            let tool_tip = tool_tip.map(|tip| tip.to_string());

            let item = match kind {
                WindowToolbarItemKind::Button(options) => {
                    let item = create_toolbar_item(&identifier);
                    if item.is_null() {
                        return nil;
                    }
                    apply_toolbar_item_metadata(
                        item,
                        &label,
                        palette_label.as_deref(),
                        tool_tip.as_deref(),
                    );
                    let view = self.make_button_view(options, &label);
                    self.configure_toolbar_item_view(item, view);
                    item
                }
                WindowToolbarItemKind::SegmentedControl(options) => {
                    let item = create_toolbar_item(&identifier);
                    if item.is_null() {
                        return nil;
                    }
                    apply_toolbar_item_metadata(
                        item,
                        &label,
                        palette_label.as_deref(),
                        tool_tip.as_deref(),
                    );
                    let view = self.make_segmented_view(options);
                    self.configure_toolbar_item_view(item, view);
                    item
                }
                WindowToolbarItemKind::Switch(options) => {
                    let item = create_toolbar_item(&identifier);
                    if item.is_null() {
                        return nil;
                    }
                    apply_toolbar_item_metadata(
                        item,
                        &label,
                        palette_label.as_deref(),
                        tool_tip.as_deref(),
                    );
                    let view = self.make_switch_view(options, &label);
                    self.configure_toolbar_item_view(item, view);
                    item
                }
                WindowToolbarItemKind::Group(options) => self.make_group_item(
                    &identifier,
                    &label,
                    palette_label.as_deref(),
                    tool_tip.as_deref(),
                    options,
                ),
                WindowToolbarItemKind::SearchField(options) => self.make_search_field_item(
                    &identifier,
                    &label,
                    palette_label.as_deref(),
                    tool_tip.as_deref(),
                    options,
                ),
                WindowToolbarItemKind::TrackingSeparator(options) => self
                    .make_tracking_separator_item(
                        &identifier,
                        &label,
                        palette_label.as_deref(),
                        tool_tip.as_deref(),
                        options,
                    ),
            };

            if item.is_null() {
                return nil;
            }

            self.created_items.insert(identifier, item);
            item
        }
    }

    unsafe fn fallback_item_for_identifier(&self, identifier: &str) -> id {
        unsafe {
            let item = create_toolbar_item(identifier);
            if item.is_null() {
                return nil;
            }
            apply_toolbar_item_metadata(item, identifier, None, None);
            item
        }
    }

    unsafe fn configure_toolbar_item_view(&self, item: id, view: id) {
        unsafe {
            let _: () = msg_send![view, sizeToFit];
            let frame: NSRect = msg_send![view, frame];
            let size = frame.size;
            let _: () = msg_send![item, setView: view];
            let _: () = msg_send![item, setMinSize: size];
            let _: () = msg_send![item, setMaxSize: size];
        }
    }

    unsafe fn make_button_view(
        &mut self,
        mut options: WindowToolbarButtonOptions,
        label: &str,
    ) -> id {
        unsafe {
            let title = options
                .title
                .take()
                .map(|title| title.to_string())
                .unwrap_or_else(|| label.to_string());
            let button = create_native_button(&title);

            if !options.bordered {
                set_native_button_bordered(button, false);
            }

            if let Some(symbol) = options.sf_symbol.as_ref() {
                set_native_button_sf_symbol(button, symbol.as_ref(), title.is_empty());
            }

            let mut target = ptr::null_mut();
            if let Some(callback) = options.on_click.take() {
                target = set_native_button_action(button, callback);
            }

            self.owned_native_controls.push(OwnedNativeControl::Button {
                control: button,
                target,
            });

            button
        }
    }

    unsafe fn make_segmented_view(
        &mut self,
        mut options: WindowToolbarSegmentedControlOptions,
    ) -> id {
        unsafe {
            let mut labels_owned = options
                .segments
                .iter()
                .map(|segment| segment.label.to_string())
                .collect::<Vec<_>>();

            if labels_owned.is_empty() {
                labels_owned.push(String::new());
            }

            let labels = labels_owned
                .iter()
                .map(|label| label.as_str())
                .collect::<Vec<_>>();

            let selected_index = options.selected_index.min(labels.len().saturating_sub(1));
            let segmented = create_native_segmented_control(&labels, selected_index);

            for (index, segment) in options.segments.iter().enumerate() {
                if let Some(symbol_name) = segment.sf_symbol.as_ref() {
                    set_native_segmented_image(segmented, index, symbol_name.as_ref());
                }
            }

            let mut target = ptr::null_mut();
            if let Some(callback) = options.on_change.take() {
                target = set_native_segmented_action(segmented, callback);
            }

            self.owned_native_controls
                .push(OwnedNativeControl::Segmented {
                    control: segmented,
                    target,
                });

            segmented
        }
    }

    unsafe fn make_switch_view(
        &mut self,
        mut options: WindowToolbarSwitchOptions,
        label: &str,
    ) -> id {
        unsafe {
            let switch = create_native_switch();
            set_native_switch_state(switch, options.checked);

            let title = options
                .title
                .take()
                .map(|title| title.to_string())
                .unwrap_or_else(|| label.to_string());
            if !title.is_empty() {
                let _: () = msg_send![switch, setTitle: super::super::ns_string(&title)];
            }

            let mut target = ptr::null_mut();
            if let Some(callback) = options.on_toggle.take() {
                target = set_native_switch_action(switch, callback);
            }

            self.owned_native_controls.push(OwnedNativeControl::Switch {
                control: switch,
                target,
            });

            switch
        }
    }

    unsafe fn make_group_item(
        &mut self,
        identifier: &str,
        label: &str,
        palette_label: Option<&str>,
        tool_tip: Option<&str>,
        mut options: WindowToolbarGroupOptions,
    ) -> id {
        unsafe {
            let Some(group_class) = Class::get("NSToolbarItemGroup") else {
                return nil;
            };

            let native_identifier = super::super::ns_string(identifier);
            let item: id = msg_send![group_class, alloc];
            let item: id = msg_send![item, initWithItemIdentifier: native_identifier];
            if item.is_null() {
                return nil;
            }

            apply_toolbar_item_metadata(item, label, palette_label, tool_tip);

            if !options.items.is_empty() {
                let mut subitems = Vec::with_capacity(options.items.len());
                for (index, group_item) in options.items.iter().enumerate() {
                    let sub_identifier = format!("{identifier}.item.{index}");
                    let subitem = create_toolbar_item(&sub_identifier);
                    if subitem.is_null() {
                        continue;
                    }
                    let _: () = msg_send![subitem, setLabel: super::super::ns_string(group_item.label.as_ref())];
                    if let Some(symbol_name) = group_item.sf_symbol.as_ref() {
                        set_toolbar_item_symbol(subitem, symbol_name.as_ref());
                    }
                    subitems.push(subitem);
                }

                let subitems_array: id = msg_send![
                    class!(NSArray),
                    arrayWithObjects: subitems.as_ptr()
                    count: subitems.len() as NSUInteger
                ];
                let _: () = msg_send![item, setSubitems: subitems_array];
                for subitem in subitems {
                    let _: () = msg_send![subitem, release];
                }
            }

            let _: () = msg_send![
                item,
                setSelectionMode: toolbar_group_selection_mode(options.selection_mode)
            ];
            let _: () = msg_send![
                item,
                setControlRepresentation:
                    toolbar_group_control_representation(options.control_representation)
            ];

            if let Some(selected_index) = options.selected_index {
                let _: () = msg_send![item, setSelectedIndex: selected_index as NSInteger];
            }

            if let Some(callback) = options.on_change.take() {
                let target = create_toolbar_group_target(callback);
                let _: () = msg_send![item, setTarget: target as id];
                let _: () = msg_send![item, setAction: sel!(groupAction:)];
                self.owned_native_controls
                    .push(OwnedNativeControl::GroupTarget { target });
            }

            item
        }
    }

    unsafe fn make_search_field_item(
        &mut self,
        identifier: &str,
        label: &str,
        palette_label: Option<&str>,
        tool_tip: Option<&str>,
        mut options: WindowToolbarSearchFieldOptions,
    ) -> id {
        unsafe {
            let Some(search_item_class) = Class::get("NSSearchToolbarItem") else {
                return nil;
            };

            let native_identifier = super::super::ns_string(identifier);
            let item: id = msg_send![search_item_class, alloc];
            let item: id = msg_send![item, initWithItemIdentifier: native_identifier];
            if item.is_null() {
                return nil;
            }

            apply_toolbar_item_metadata(item, label, palette_label, tool_tip);

            if let Some(width) = options.preferred_width {
                let can_set_width: i8 =
                    msg_send![item, respondsToSelector: sel!(setPreferredWidth:)];
                if can_set_width != 0 {
                    let _: () = msg_send![item, setPreferredWidth: width.0 as f64];
                }
            }

            let search_field: id = msg_send![item, searchField];
            if search_field.is_null() {
                return item;
            }

            if let Some(placeholder) = options.placeholder.as_ref() {
                let _: () = msg_send![search_field, setPlaceholderString: super::super::ns_string(placeholder.as_ref())];
            }
            if let Some(value) = options.value.as_ref() {
                let _: () = msg_send![search_field, setStringValue: super::super::ns_string(value.as_ref())];
            }

            let has_on_change = options.on_change.is_some();
            let has_on_submit = options.on_submit.is_some();

            if has_on_change {
                let _: () = msg_send![search_field, setSendsSearchStringImmediately: 1i8];
            }
            if has_on_submit {
                let _: () = msg_send![search_field, setSendsWholeSearchString: 1i8];
            }

            if has_on_change || has_on_submit {
                let target = create_toolbar_search_target(
                    options.on_change.take(),
                    options.on_submit.take(),
                );
                let _: () = msg_send![search_field, setTarget: target as id];
                let _: () = msg_send![search_field, setAction: sel!(searchAction:)];
                let _: () = msg_send![search_field, setDelegate: target as id];
                self.owned_native_controls
                    .push(OwnedNativeControl::SearchTarget { target });
            }

            item
        }
    }

    unsafe fn make_tracking_separator_item(
        &mut self,
        identifier: &str,
        label: &str,
        palette_label: Option<&str>,
        tool_tip: Option<&str>,
        options: WindowToolbarTrackingSeparatorOptions,
    ) -> id {
        unsafe {
            let Some(tracking_separator_class) = Class::get("NSTrackingSeparatorToolbarItem")
            else {
                return nil;
            };

            let split_view = options.split_view as id;
            if split_view.is_null() {
                return nil;
            }

            let native_identifier = super::super::ns_string(identifier);
            let item: id = msg_send![tracking_separator_class, alloc];
            let item: id = msg_send![
                item,
                initWithIdentifier: native_identifier
                splitView: split_view
                dividerIndex: options.divider_index as NSInteger
            ];
            if item.is_null() {
                return nil;
            }

            apply_toolbar_item_metadata(item, label, palette_label, tool_tip);
            item
        }
    }
}

pub(crate) unsafe fn install_native_window_toolbar(
    window: id,
    options: WindowToolbarOptions,
) -> NativeToolbarState {
    unsafe {
        let toolbar_identifier = options.identifier.clone();
        let display_mode = options.display_mode;
        let allows_user_customization = options.allows_user_customization;
        let autosaves_configuration = options.autosaves_configuration;
        let shows_baseline_separator = options.shows_baseline_separator;
        let centered_item_identifier = options.centered_item_identifier.clone();
        let selected_item_identifier = options.selected_item_identifier.clone();

        let toolbar: id = msg_send![class!(NSToolbar), alloc];
        let toolbar: id = msg_send![
            toolbar,
            initWithIdentifier: super::super::ns_string(toolbar_identifier.as_ref())
        ];

        let delegate: id = msg_send![TOOLBAR_DELEGATE_CLASS, alloc];
        let delegate: id = msg_send![delegate, init];

        let runtime = Box::new(ToolbarRuntime::new(options));
        (*delegate).set_ivar(TOOLBAR_RUNTIME_IVAR, Box::into_raw(runtime) as *mut c_void);

        let _: () = msg_send![toolbar, setDelegate: delegate];
        let _: () = msg_send![toolbar, setAllowsUserCustomization: allows_user_customization as i8];
        let _: () = msg_send![toolbar, setAutosavesConfiguration: autosaves_configuration as i8];
        let _: () = msg_send![toolbar, setDisplayMode: toolbar_display_mode(display_mode)];

        let can_set_baseline_separator: i8 =
            msg_send![toolbar, respondsToSelector: sel!(setShowsBaselineSeparator:)];
        if can_set_baseline_separator != 0 {
            let _: () =
                msg_send![toolbar, setShowsBaselineSeparator: shows_baseline_separator as i8];
        }

        if let Some(identifier) = centered_item_identifier.as_ref() {
            let can_set_centered: i8 =
                msg_send![toolbar, respondsToSelector: sel!(setCenteredItemIdentifier:)];
            if can_set_centered != 0 {
                let _: () = msg_send![toolbar, setCenteredItemIdentifier: toolbar_item_identifier(identifier)];
            }
        }

        if let Some(identifier) = selected_item_identifier.as_ref() {
            let _: () =
                msg_send![toolbar, setSelectedItemIdentifier: toolbar_item_identifier(identifier)];
        }

        let _: () = msg_send![window, setToolbar: toolbar];

        NativeToolbarState { toolbar, delegate }
    }
}

unsafe fn create_toolbar_item(identifier: &str) -> id {
    unsafe {
        let native_identifier = super::super::ns_string(identifier);
        let item: id = msg_send![class!(NSToolbarItem), alloc];
        msg_send![item, initWithItemIdentifier: native_identifier]
    }
}

unsafe fn apply_toolbar_item_metadata(
    item: id,
    label: &str,
    palette_label: Option<&str>,
    tool_tip: Option<&str>,
) {
    unsafe {
        let _: () = msg_send![item, setLabel: super::super::ns_string(label)];
        if let Some(label) = palette_label {
            let _: () = msg_send![item, setPaletteLabel: super::super::ns_string(label)];
        }
        if let Some(tip) = tool_tip {
            let _: () = msg_send![item, setToolTip: super::super::ns_string(tip)];
        }
    }
}

unsafe fn set_toolbar_item_symbol(item: id, symbol_name: &str) {
    unsafe {
        let image: id = msg_send![
            class!(NSImage),
            imageWithSystemSymbolName: super::super::ns_string(symbol_name)
            accessibilityDescription: nil
        ];
        if !image.is_null() {
            let _: () = msg_send![item, setImage: image];
        }
    }
}

extern "C" fn toolbar_delegate_dealloc(this: &mut Object, _: Sel) {
    unsafe {
        let runtime_ptr: *mut c_void = *this.get_ivar(TOOLBAR_RUNTIME_IVAR);
        if !runtime_ptr.is_null() {
            let _ = Box::from_raw(runtime_ptr as *mut ToolbarRuntime);
            this.set_ivar(TOOLBAR_RUNTIME_IVAR, ptr::null_mut::<c_void>());
        }

        let _: () = msg_send![super(this, class!(NSObject)), dealloc];
    }
}

extern "C" fn toolbar_allowed_item_identifiers(this: &Object, _: Sel, _toolbar: id) -> id {
    unsafe {
        if let Some(runtime) = toolbar_runtime(this) {
            runtime.allowed_item_identifiers_array()
        } else {
            msg_send![class!(NSArray), array]
        }
    }
}

extern "C" fn toolbar_default_item_identifiers(this: &Object, _: Sel, _toolbar: id) -> id {
    unsafe {
        if let Some(runtime) = toolbar_runtime(this) {
            runtime.default_item_identifiers_array()
        } else {
            msg_send![class!(NSArray), array]
        }
    }
}

extern "C" fn toolbar_selectable_item_identifiers(this: &Object, _: Sel, _toolbar: id) -> id {
    unsafe {
        if let Some(runtime) = toolbar_runtime(this) {
            runtime.selectable_item_identifiers_array()
        } else {
            msg_send![class!(NSArray), array]
        }
    }
}

extern "C" fn toolbar_item_for_identifier(
    this: &Object,
    _: Sel,
    _toolbar: id,
    identifier: id,
    _will_be_inserted: i8,
) -> id {
    let result = catch_unwind(AssertUnwindSafe(|| unsafe {
        if let Some(runtime) = toolbar_runtime_mut(this) {
            runtime.item_for_identifier(identifier)
        } else {
            nil
        }
    }));

    match result {
        Ok(item) => item,
        Err(_) => {
            log::error!("toolbar:itemForItemIdentifier: callback panicked");
            nil
        }
    }
}

unsafe fn toolbar_runtime(this: &Object) -> Option<&ToolbarRuntime> {
    unsafe {
        let runtime_ptr: *mut c_void = *this.get_ivar(TOOLBAR_RUNTIME_IVAR);
        if runtime_ptr.is_null() {
            None
        } else {
            Some(&*(runtime_ptr as *const ToolbarRuntime))
        }
    }
}

unsafe fn toolbar_runtime_mut(this: &Object) -> Option<&mut ToolbarRuntime> {
    unsafe {
        let runtime_ptr: *mut c_void = *this.get_ivar(TOOLBAR_RUNTIME_IVAR);
        if runtime_ptr.is_null() {
            None
        } else {
            Some(&mut *(runtime_ptr as *mut ToolbarRuntime))
        }
    }
}

fn push_unique_identifier(
    identifiers: &mut Vec<WindowToolbarItemIdentifier>,
    identifier: WindowToolbarItemIdentifier,
) {
    if !identifiers.contains(&identifier) {
        identifiers.push(identifier);
    }
}

unsafe fn toolbar_item_identifiers_array(identifiers: &[WindowToolbarItemIdentifier]) -> id {
    unsafe {
        if identifiers.is_empty() {
            return msg_send![class!(NSArray), array];
        }

        let native_identifiers = identifiers
            .iter()
            .map(|identifier| toolbar_item_identifier(identifier))
            .collect::<Vec<_>>();

        msg_send![
            class!(NSArray),
            arrayWithObjects: native_identifiers.as_ptr()
            count: native_identifiers.len() as NSUInteger
        ]
    }
}

fn toolbar_display_mode(display_mode: WindowToolbarDisplayMode) -> NSInteger {
    match display_mode {
        WindowToolbarDisplayMode::Default => 0,
        WindowToolbarDisplayMode::IconAndLabel => 1,
        WindowToolbarDisplayMode::IconOnly => 2,
        WindowToolbarDisplayMode::LabelOnly => 3,
    }
}

fn toolbar_group_selection_mode(selection_mode: WindowToolbarGroupSelectionMode) -> NSInteger {
    match selection_mode {
        WindowToolbarGroupSelectionMode::SelectOne => 0,
        WindowToolbarGroupSelectionMode::SelectAny => 1,
        WindowToolbarGroupSelectionMode::Momentary => 2,
    }
}

fn toolbar_group_control_representation(
    representation: WindowToolbarGroupControlRepresentation,
) -> NSInteger {
    match representation {
        WindowToolbarGroupControlRepresentation::Automatic => 0,
        WindowToolbarGroupControlRepresentation::Expanded => 1,
        WindowToolbarGroupControlRepresentation::Collapsed => 2,
    }
}

unsafe fn toolbar_item_identifier(identifier: &WindowToolbarItemIdentifier) -> id {
    unsafe {
        match identifier {
            WindowToolbarItemIdentifier::Custom(identifier) => {
                super::super::ns_string(identifier.as_ref())
            }
            WindowToolbarItemIdentifier::Space => NSToolbarSpaceItemIdentifier,
            WindowToolbarItemIdentifier::FlexibleSpace => NSToolbarFlexibleSpaceItemIdentifier,
            WindowToolbarItemIdentifier::Separator => NSToolbarSeparatorItemIdentifier,
            WindowToolbarItemIdentifier::ToggleSidebar => NSToolbarToggleSidebarItemIdentifier,
            WindowToolbarItemIdentifier::SidebarTrackingSeparator => {
                NSToolbarSidebarTrackingSeparatorItemIdentifier
            }
        }
    }
}

unsafe fn is_standard_toolbar_identifier(identifier: id) -> bool {
    unsafe {
        identifier_equals(identifier, NSToolbarSpaceItemIdentifier)
            || identifier_equals(identifier, NSToolbarFlexibleSpaceItemIdentifier)
            || identifier_equals(identifier, NSToolbarSeparatorItemIdentifier)
            || identifier_equals(identifier, NSToolbarToggleSidebarItemIdentifier)
            || identifier_equals(identifier, NSToolbarSidebarTrackingSeparatorItemIdentifier)
    }
}

unsafe fn identifier_equals(lhs: id, rhs: id) -> bool {
    unsafe {
        if lhs.is_null() || rhs.is_null() {
            return false;
        }
        let equal: i8 = msg_send![lhs, isEqual: rhs];
        equal != 0
    }
}

unsafe fn control_string_value(control: id) -> String {
    unsafe {
        if control.is_null() {
            return String::new();
        }

        let value: id = msg_send![control, stringValue];
        ns_string_to_rust(value).unwrap_or_default()
    }
}

unsafe fn ns_string_to_rust(value: id) -> Option<String> {
    unsafe {
        if value.is_null() {
            return None;
        }

        let cstr: *const c_char = msg_send![value, UTF8String];
        if cstr.is_null() {
            return None;
        }

        Some(CStr::from_ptr(cstr).to_string_lossy().into_owned())
    }
}
