use std::ffi::c_void;

use cocoa::{base::id, foundation::NSRect};
use gpui::native_controls::*;
use gpui::{Bounds, Pixels, point, px, size};
use objc::{sel, sel_impl};

use crate::native_controls;

pub struct MacNativeControls;

macro_rules! define_cleanup_with_target {
    ($name:ident, $release_target:ident, $release_view:ident) => {
        unsafe fn $name(view: *mut c_void, target: *mut c_void) {
            unsafe {
                if !target.is_null() {
                    native_controls::$release_target(target);
                }
                if !view.is_null() {
                    native_controls::remove_from_parent(view as id);
                    native_controls::$release_view(view as id);
                }
            }
        }
    };
}

macro_rules! define_cleanup_view_only {
    ($name:ident, $release_view:ident) => {
        unsafe fn $name(view: *mut c_void, _target: *mut c_void) {
            unsafe {
                if !view.is_null() {
                    native_controls::remove_from_parent(view as id);
                    native_controls::$release_view(view as id);
                }
            }
        }
    };
}

fn cleanup_view_and_target_impl(view: *mut c_void, target: *mut c_void) {
    unsafe {
        if !view.is_null() {
            native_controls::remove_from_parent(view as id);
        }
        if !target.is_null() {
            let target_obj = target as id;
            let callback_ptr: *mut c_void = *(*target_obj).get_ivar(native_controls::CALLBACK_IVAR);
            if !callback_ptr.is_null() {
                // The callback could be Box<dyn Fn()>, Box<dyn Fn(bool)>, Box<dyn Fn(f64)>,
                // Box<dyn Fn(usize)>, etc. We stored it as a raw pointer to a Box<dyn Fn(...)>.
                // We use a type-erased drop: the target's dealloc should handle this.
                // For safety, we just release the target object which triggers its dealloc.
            }
            let _: () = objc::msg_send![target_obj, release];
        }
        if !view.is_null() {
            let _: () = objc::msg_send![view as id, release];
        }
    }
}

unsafe fn cleanup_view_and_target(view: *mut c_void, target: *mut c_void) {
    cleanup_view_and_target_impl(view, target);
}

define_cleanup_with_target!(cleanup_checkbox, release_native_checkbox_target, release_native_checkbox);
define_cleanup_with_target!(cleanup_switch, release_native_switch_target, release_native_switch);
define_cleanup_with_target!(cleanup_slider, release_native_slider_target, release_native_slider);
define_cleanup_with_target!(cleanup_stepper, release_native_stepper_target, release_native_stepper);
define_cleanup_with_target!(
    cleanup_segmented_control,
    release_native_segmented_target,
    release_native_segmented_control
);
define_cleanup_with_target!(cleanup_popup_button, release_native_popup_target, release_native_popup_button);
define_cleanup_with_target!(cleanup_search_field, release_native_text_field_delegate, release_native_search_field);
define_cleanup_with_target!(cleanup_text_field, release_native_text_field_delegate, release_native_text_field);
define_cleanup_with_target!(cleanup_combo_box, release_native_combo_box_delegate, release_native_combo_box);
define_cleanup_with_target!(cleanup_tab_view, release_native_tab_view_target, release_native_tab_view);
define_cleanup_with_target!(cleanup_table_view, release_native_table_target, release_native_table_view);
define_cleanup_with_target!(cleanup_outline_view, release_native_outline_target, release_native_outline_view);
define_cleanup_with_target!(
    cleanup_collection_view,
    release_native_collection_target,
    release_native_collection_view
);
define_cleanup_with_target!(
    cleanup_menu_button,
    release_native_menu_button_target,
    release_native_menu_button
);
define_cleanup_with_target!(cleanup_tracking_view, release_native_tracking_view_target, release_native_tracking_view);
define_cleanup_with_target!(cleanup_sidebar, release_sidebar_target, release_sidebar_view);
define_cleanup_view_only!(cleanup_progress, release_native_progress_indicator);
define_cleanup_view_only!(cleanup_image_view, release_native_image_view);
define_cleanup_view_only!(cleanup_visual_effect_view, release_native_visual_effect_view);
define_cleanup_view_only!(cleanup_glass_effect_view, release_native_glass_effect_view);
define_cleanup_view_only!(cleanup_stack_view, release_native_stack_view);

fn ensure_attached(
    state: &NativeControlState,
    parent: *mut c_void,
    bounds: Bounds<Pixels>,
    scale: f32,
) {
    native_controls::attach_and_position(parent, state.view() as id, bounds, scale);
}

fn has_text_field_callbacks(callbacks: &TextFieldCallbacks) -> bool {
    callbacks.on_change.is_some()
        || callbacks.on_begin_editing.is_some()
        || callbacks.on_end_editing.is_some()
        || callbacks.on_submit.is_some()
        || callbacks.on_move_up.is_some()
        || callbacks.on_move_down.is_some()
        || callbacks.on_cancel.is_some()
}

fn into_native_text_field_callbacks(
    callbacks: TextFieldCallbacks,
) -> crate::native_controls::TextFieldCallbacks {
    crate::native_controls::TextFieldCallbacks {
        on_change: callbacks.on_change,
        on_begin_editing: callbacks.on_begin_editing,
        on_end_editing: callbacks.on_end_editing,
        on_submit: callbacks.on_submit,
        on_move_up: callbacks.on_move_up,
        on_move_down: callbacks.on_move_down,
        on_cancel: callbacks.on_cancel,
    }
}

fn has_combo_box_callbacks(callbacks: &ComboBoxCallbacks) -> bool {
    callbacks.on_select.is_some() || callbacks.on_change.is_some() || callbacks.on_submit.is_some()
}

fn into_native_combo_box_callbacks(
    callbacks: ComboBoxCallbacks,
) -> crate::native_controls::ComboBoxCallbacks {
    crate::native_controls::ComboBoxCallbacks {
        on_select: callbacks.on_select,
        on_change: callbacks.on_change,
        on_submit: callbacks.on_submit,
    }
}

fn has_tracking_view_callbacks(callbacks: &TrackingViewCallbacks) -> bool {
    callbacks.on_enter.is_some() || callbacks.on_exit.is_some() || callbacks.on_move.is_some()
}

fn into_native_tracking_view_callbacks(
    callbacks: TrackingViewCallbacks,
) -> crate::native_controls::TrackingViewCallbacks {
    crate::native_controls::TrackingViewCallbacks {
        on_enter: callbacks.on_enter,
        on_exit: callbacks.on_exit,
        on_move: callbacks.on_move,
    }
}

fn convert_outline_nodes(
    nodes: &[NativeOutlineNodeData],
) -> Vec<crate::native_controls::NativeOutlineNodeData> {
    nodes
        .iter()
        .map(|node| crate::native_controls::NativeOutlineNodeData {
            title: node.title.clone(),
            children: convert_outline_nodes(&node.children),
        })
        .collect()
}

fn convert_collection_item_style(
    style: CollectionItemStyle,
) -> crate::native_controls::NativeCollectionItemStyleData {
    match style {
        CollectionItemStyle::Label => crate::native_controls::NativeCollectionItemStyleData::Label,
        CollectionItemStyle::Card => crate::native_controls::NativeCollectionItemStyleData::Card,
    }
}

fn convert_menu_items(
    items: &[NativeMenuItemData],
) -> Vec<crate::native_controls::NativeMenuItemData> {
    items
        .iter()
        .map(|item| match item {
            NativeMenuItemData::Action {
                title,
                enabled,
                icon,
            } => crate::native_controls::NativeMenuItemData::Action {
                title: title.clone(),
                enabled: *enabled,
                icon: icon.clone(),
            },
            NativeMenuItemData::Submenu {
                title,
                enabled,
                icon,
                items,
            } => crate::native_controls::NativeMenuItemData::Submenu {
                title: title.clone(),
                enabled: *enabled,
                icon: icon.clone(),
                items: convert_menu_items(items),
            },
            NativeMenuItemData::Separator => crate::native_controls::NativeMenuItemData::Separator,
        })
        .collect()
}

fn convert_alert_style(style: AlertStyle) -> crate::native_controls::NativeAlertStyleRaw {
    match style {
        AlertStyle::Warning => crate::native_controls::NativeAlertStyleRaw::Warning,
        AlertStyle::Informational => crate::native_controls::NativeAlertStyleRaw::Informational,
        AlertStyle::Critical => crate::native_controls::NativeAlertStyleRaw::Critical,
    }
}

fn convert_panel_style(style: PanelStyle) -> crate::native_controls::NativePanelStyle {
    match style {
        PanelStyle::Titled => crate::native_controls::NativePanelStyle::Titled,
        PanelStyle::Borderless => crate::native_controls::NativePanelStyle::Borderless,
        PanelStyle::Hud => crate::native_controls::NativePanelStyle::Hud,
        PanelStyle::Utility => crate::native_controls::NativePanelStyle::Utility,
    }
}

fn convert_panel_level(level: PanelLevel) -> crate::native_controls::NativePanelLevel {
    match level {
        PanelLevel::Normal => crate::native_controls::NativePanelLevel::Normal,
        PanelLevel::Floating => crate::native_controls::NativePanelLevel::Floating,
        PanelLevel::ModalPanel => crate::native_controls::NativePanelLevel::ModalPanel,
        PanelLevel::PopUpMenu => crate::native_controls::NativePanelLevel::PopUpMenu,
        PanelLevel::Custom(value) => crate::native_controls::NativePanelLevel::Custom(value),
    }
}

fn convert_panel_material(
    material: Option<PanelMaterial>,
) -> Option<crate::native_controls::NativePanelMaterial> {
    material.map(|material| match material {
        PanelMaterial::HudWindow => crate::native_controls::NativePanelMaterial::HudWindow,
        PanelMaterial::Popover => crate::native_controls::NativePanelMaterial::Popover,
        PanelMaterial::Sidebar => crate::native_controls::NativePanelMaterial::Sidebar,
        PanelMaterial::UnderWindow => crate::native_controls::NativePanelMaterial::UnderWindow,
    })
}

fn bounds_from_ns_rect(rect: NSRect) -> Bounds<Pixels> {
    Bounds::new(
        point(px(rect.origin.x as f32), px(rect.origin.y as f32)),
        size(px(rect.size.width as f32), px(rect.size.height as f32)),
    )
}

impl PlatformNativeControls for MacNativeControls {
    unsafe fn attach_and_position(
        &self,
        state: &NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
    ) {
        native_controls::attach_and_position(parent, state.view() as id, bounds, scale);
    }

    unsafe fn remove_from_parent(&self, state: &NativeControlState) {
        native_controls::remove_from_parent(state.view() as id);
    }

    fn update_button(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: ButtonConfig,
    ) {
        unsafe {
            if state.is_initialized() {
                let view = state.view() as id;
                native_controls::set_native_button_title(view, config.title);
                apply_button_style(view, config.style);
                apply_button_tint(view, config.tint);
                native_controls::set_native_control_enabled(view, config.enabled);
                if let Some(sf) = config.sf_symbol {
                    native_controls::set_native_button_sf_symbol(view, sf, config.title.is_empty());
                }
                if let Some(tooltip) = config.tooltip {
                    native_controls::set_native_view_tooltip(view, tooltip);
                }
                // Replace callback target
                if let Some(callback) = config.on_click {
                    native_controls::release_native_button_target(state.target());
                    let target = native_controls::set_native_button_action(view, callback);
                    state.set_target(target);
                }
                ensure_attached(state, parent, bounds, scale);
            } else {
                let view = native_controls::create_native_button(config.title);
                apply_button_style(view, config.style);
                apply_button_tint(view, config.tint);
                native_controls::set_native_control_enabled(view, config.enabled);
                if let Some(sf) = config.sf_symbol {
                    native_controls::set_native_button_sf_symbol(view, sf, config.title.is_empty());
                }
                if let Some(tooltip) = config.tooltip {
                    native_controls::set_native_view_tooltip(view, tooltip);
                }
                let target = if let Some(callback) = config.on_click {
                    native_controls::set_native_button_action(view, callback)
                } else {
                    std::ptr::null_mut()
                };
                *state =
                    NativeControlState::new(view as *mut c_void, target, cleanup_view_and_target);
                ensure_attached(state, parent, bounds, scale);
            }
        }
    }

    fn update_checkbox(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: CheckboxConfig,
    ) {
        unsafe {
            if state.is_initialized() {
                let view = state.view() as id;
                native_controls::set_native_checkbox_title(view, config.title);
                native_controls::set_native_checkbox_state(view, config.checked);
                native_controls::set_native_control_enabled(view, config.enabled);
                if let Some(callback) = config.on_change {
                    native_controls::release_native_checkbox_target(state.target());
                    let target = native_controls::set_native_checkbox_action(view, callback);
                    state.set_target(target);
                }
                ensure_attached(state, parent, bounds, scale);
            } else {
                let view = native_controls::create_native_checkbox(config.title);
                native_controls::set_native_checkbox_state(view, config.checked);
                native_controls::set_native_control_enabled(view, config.enabled);
                let target = if let Some(callback) = config.on_change {
                    native_controls::set_native_checkbox_action(view, callback)
                } else {
                    std::ptr::null_mut()
                };
                *state = NativeControlState::new(view as *mut c_void, target, cleanup_checkbox);
                ensure_attached(state, parent, bounds, scale);
            }
        }
    }

    fn update_switch(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: SwitchConfig,
    ) {
        unsafe {
            if state.is_initialized() {
                let view = state.view() as id;
                native_controls::set_native_switch_state(view, config.checked);
                native_controls::set_native_control_enabled(view, config.enabled);
                if let Some(callback) = config.on_change {
                    native_controls::release_native_switch_target(state.target());
                    let target = native_controls::set_native_switch_action(view, callback);
                    state.set_target(target);
                }
                ensure_attached(state, parent, bounds, scale);
            } else {
                let view = native_controls::create_native_switch();
                native_controls::set_native_switch_state(view, config.checked);
                native_controls::set_native_control_enabled(view, config.enabled);
                let target = if let Some(callback) = config.on_change {
                    native_controls::set_native_switch_action(view, callback)
                } else {
                    std::ptr::null_mut()
                };
                *state = NativeControlState::new(view as *mut c_void, target, cleanup_switch);
                ensure_attached(state, parent, bounds, scale);
            }
        }
    }

    fn update_slider(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: SliderConfig,
    ) {
        unsafe {
            if state.is_initialized() {
                let view = state.view() as id;
                native_controls::set_native_slider_min(view, config.min);
                native_controls::set_native_slider_max(view, config.max);
                native_controls::set_native_slider_value(view, config.value);
                native_controls::set_native_slider_continuous(view, config.continuous);
                native_controls::set_native_slider_tick_marks(
                    view,
                    config.tick_mark_count,
                    config.snap_to_ticks,
                );
                native_controls::set_native_control_enabled(view, config.enabled);
                if let Some(callback) = config.on_change {
                    native_controls::release_native_slider_target(state.target());
                    let target = native_controls::set_native_slider_action(view, callback);
                    state.set_target(target);
                }
                ensure_attached(state, parent, bounds, scale);
            } else {
                let view =
                    native_controls::create_native_slider(config.min, config.max, config.value);
                native_controls::set_native_slider_continuous(view, config.continuous);
                native_controls::set_native_slider_tick_marks(
                    view,
                    config.tick_mark_count,
                    config.snap_to_ticks,
                );
                native_controls::set_native_control_enabled(view, config.enabled);
                let target = if let Some(callback) = config.on_change {
                    native_controls::set_native_slider_action(view, callback)
                } else {
                    std::ptr::null_mut()
                };
                *state = NativeControlState::new(view as *mut c_void, target, cleanup_slider);
                ensure_attached(state, parent, bounds, scale);
            }
        }
    }

    fn update_stepper(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: StepperConfig,
    ) {
        unsafe {
            if state.is_initialized() {
                let view = state.view() as id;
                native_controls::set_native_stepper_min(view, config.min);
                native_controls::set_native_stepper_max(view, config.max);
                native_controls::set_native_stepper_value(view, config.value);
                native_controls::set_native_stepper_increment(view, config.increment);
                native_controls::set_native_stepper_wraps(view, config.wraps);
                native_controls::set_native_stepper_autorepeat(view, config.autorepeat);
                native_controls::set_native_control_enabled(view, config.enabled);
                if let Some(callback) = config.on_change {
                    native_controls::release_native_stepper_target(state.target());
                    let target = native_controls::set_native_stepper_action(view, callback);
                    state.set_target(target);
                }
                ensure_attached(state, parent, bounds, scale);
            } else {
                let view = native_controls::create_native_stepper(
                    config.min,
                    config.max,
                    config.value,
                    config.increment,
                );
                native_controls::set_native_stepper_wraps(view, config.wraps);
                native_controls::set_native_stepper_autorepeat(view, config.autorepeat);
                native_controls::set_native_control_enabled(view, config.enabled);
                let target = if let Some(callback) = config.on_change {
                    native_controls::set_native_stepper_action(view, callback)
                } else {
                    std::ptr::null_mut()
                };
                *state = NativeControlState::new(view as *mut c_void, target, cleanup_stepper);
                ensure_attached(state, parent, bounds, scale);
            }
        }
    }

    fn update_segmented_control(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: SegmentedControlConfig,
    ) {
        unsafe {
            if state.is_initialized() {
                let view = state.view() as id;
                native_controls::set_native_segmented_selected(view, config.selected_index);
                native_controls::set_native_segmented_border_shape(view, config.border_shape);
                native_controls::set_native_segmented_control_size(view, config.control_size);
                for &(segment, symbol) in config.images {
                    native_controls::set_native_segmented_image(view, segment, symbol);
                }
                native_controls::set_native_control_enabled(view, config.enabled);
                if let Some(callback) = config.on_select {
                    native_controls::release_native_segmented_target(state.target());
                    let target = native_controls::set_native_segmented_action(view, callback);
                    state.set_target(target);
                }
                ensure_attached(state, parent, bounds, scale);
            } else {
                let view = native_controls::create_native_segmented_control(
                    config.labels,
                    config.selected_index,
                );
                native_controls::set_native_segmented_selected(view, config.selected_index);
                native_controls::set_native_segmented_border_shape(view, config.border_shape);
                native_controls::set_native_segmented_control_size(view, config.control_size);
                for &(segment, symbol) in config.images {
                    native_controls::set_native_segmented_image(view, segment, symbol);
                }
                native_controls::set_native_control_enabled(view, config.enabled);
                let target = if let Some(callback) = config.on_select {
                    native_controls::set_native_segmented_action(view, callback)
                } else {
                    std::ptr::null_mut()
                };
                *state = NativeControlState::new(
                    view as *mut c_void,
                    target,
                    cleanup_segmented_control,
                );
                ensure_attached(state, parent, bounds, scale);
            }
        }
    }

    fn update_popup_button(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: PopupButtonConfig,
    ) {
        unsafe {
            if state.is_initialized() {
                let view = state.view() as id;
                native_controls::set_native_popup_items(view, config.items);
                native_controls::set_native_popup_selected(view, config.selected_index);
                native_controls::set_native_control_enabled(view, config.enabled);
                if let Some(callback) = config.on_select {
                    native_controls::release_native_popup_target(state.target());
                    let target = native_controls::set_native_popup_action(view, callback);
                    state.set_target(target);
                }
                ensure_attached(state, parent, bounds, scale);
            } else {
                let view = native_controls::create_native_popup_button(
                    config.items,
                    config.selected_index,
                );
                native_controls::set_native_control_enabled(view, config.enabled);
                let target = if let Some(callback) = config.on_select {
                    native_controls::set_native_popup_action(view, callback)
                } else {
                    std::ptr::null_mut()
                };
                *state =
                    NativeControlState::new(view as *mut c_void, target, cleanup_popup_button);
                ensure_attached(state, parent, bounds, scale);
            }
        }
    }

    fn update_progress(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: ProgressConfig,
    ) {
        unsafe {
            if state.is_initialized() {
                let view = state.view() as id;
                native_controls::set_native_progress_style(view, config.style);
                native_controls::set_native_progress_indeterminate(view, config.indeterminate);
                native_controls::set_native_progress_value(view, config.value);
                native_controls::set_native_progress_min_max(view, config.min, config.max);
                if config.animating {
                    native_controls::start_native_progress_animation(view);
                } else {
                    native_controls::stop_native_progress_animation(view);
                }
                native_controls::set_native_progress_displayed_when_stopped(
                    view,
                    config.display_when_stopped,
                );
                ensure_attached(state, parent, bounds, scale);
            } else {
                let view = native_controls::create_native_progress_indicator();
                native_controls::set_native_progress_style(view, config.style);
                native_controls::set_native_progress_indeterminate(view, config.indeterminate);
                native_controls::set_native_progress_value(view, config.value);
                native_controls::set_native_progress_min_max(view, config.min, config.max);
                if config.animating {
                    native_controls::start_native_progress_animation(view);
                } else {
                    native_controls::stop_native_progress_animation(view);
                }
                native_controls::set_native_progress_displayed_when_stopped(
                    view,
                    config.display_when_stopped,
                );
                *state = NativeControlState::new(
                    view as *mut c_void,
                    std::ptr::null_mut(),
                    cleanup_progress,
                );
                ensure_attached(state, parent, bounds, scale);
            }
        }
    }

    fn update_search_field(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: SearchFieldConfig,
    ) {
        unsafe {
            if state.is_initialized() {
                let view = state.view() as id;
                native_controls::set_native_search_field_placeholder(view, config.placeholder);
                native_controls::set_native_search_field_string_value(view, config.value);
                if let Some(identifier) = config.identifier {
                    native_controls::set_native_search_field_identifier(view, identifier);
                }
                native_controls::set_native_search_field_sends_immediately(
                    view,
                    config.sends_immediately,
                );
                native_controls::set_native_search_field_sends_whole_string(
                    view,
                    config.sends_whole_string,
                );
                native_controls::set_native_control_enabled(view, config.enabled);
                // Delegate handles callbacks — replace if provided
                if has_text_field_callbacks(&config.callbacks) {
                    native_controls::release_native_text_field_delegate(state.target());
                    let delegate = native_controls::set_native_text_field_delegate(
                        view,
                        into_native_text_field_callbacks(config.callbacks),
                    );
                    state.set_target(delegate);
                }
                ensure_attached(state, parent, bounds, scale);
            } else {
                let view = native_controls::create_native_search_field(config.placeholder);
                native_controls::set_native_search_field_string_value(view, config.value);
                if let Some(identifier) = config.identifier {
                    native_controls::set_native_search_field_identifier(view, identifier);
                }
                native_controls::set_native_search_field_sends_immediately(
                    view,
                    config.sends_immediately,
                );
                native_controls::set_native_search_field_sends_whole_string(
                    view,
                    config.sends_whole_string,
                );
                native_controls::set_native_control_enabled(view, config.enabled);
                let delegate = native_controls::set_native_text_field_delegate(
                    view,
                    into_native_text_field_callbacks(config.callbacks),
                );
                *state =
                    NativeControlState::new(view as *mut c_void, delegate, cleanup_search_field);
                ensure_attached(state, parent, bounds, scale);
            }
        }
    }

    fn update_text_field(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: TextFieldConfig,
    ) {
        unsafe {
            if state.is_initialized() {
                let view = state.view() as id;
                native_controls::set_native_text_field_placeholder(view, config.placeholder);
                native_controls::set_native_text_field_string_value(view, config.value);
                if let Some(font_size) = config.font_size {
                    native_controls::set_native_text_field_font_size(view, font_size);
                }
                if let Some(alignment) = config.alignment {
                    native_controls::set_native_text_field_alignment(view, alignment);
                }
                if let Some(bezel_style) = config.bezel_style {
                    native_controls::set_native_text_field_bezel_style(view, bezel_style);
                }
                native_controls::set_native_control_enabled(view, config.enabled);
                if has_text_field_callbacks(&config.callbacks) {
                    native_controls::release_native_text_field_delegate(state.target());
                    let delegate = native_controls::set_native_text_field_delegate(
                        view,
                        into_native_text_field_callbacks(config.callbacks),
                    );
                    state.set_target(delegate);
                }
                ensure_attached(state, parent, bounds, scale);
            } else {
                let view = if config.secure {
                    native_controls::create_native_secure_text_field(config.placeholder)
                } else {
                    native_controls::create_native_text_field(config.placeholder)
                };
                native_controls::set_native_text_field_string_value(view, config.value);
                if let Some(font_size) = config.font_size {
                    native_controls::set_native_text_field_font_size(view, font_size);
                }
                if let Some(alignment) = config.alignment {
                    native_controls::set_native_text_field_alignment(view, alignment);
                }
                if let Some(bezel_style) = config.bezel_style {
                    native_controls::set_native_text_field_bezel_style(view, bezel_style);
                }
                native_controls::set_native_control_enabled(view, config.enabled);
                let delegate = native_controls::set_native_text_field_delegate(
                    view,
                    into_native_text_field_callbacks(config.callbacks),
                );
                *state =
                    NativeControlState::new(view as *mut c_void, delegate, cleanup_text_field);
                ensure_attached(state, parent, bounds, scale);
            }
        }
    }

    fn update_combo_box(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: ComboBoxConfig,
    ) {
        unsafe {
            if state.is_initialized() {
                let view = state.view() as id;
                native_controls::set_native_combo_box_items(view, config.items);
                native_controls::set_native_combo_box_selected(view, config.selected_index);
                native_controls::set_native_combo_box_editable(view, config.editable);
                native_controls::set_native_combo_box_completes(view, config.completes);
                if let Some(value) = config.value {
                    native_controls::set_native_combo_box_string_value(view, value);
                }
                native_controls::set_native_control_enabled(view, config.enabled);
                if has_combo_box_callbacks(&config.callbacks) {
                    native_controls::release_native_combo_box_delegate(state.target());
                    let delegate = native_controls::set_native_combo_box_delegate(
                        view,
                        into_native_combo_box_callbacks(config.callbacks),
                    );
                    state.set_target(delegate);
                }
                ensure_attached(state, parent, bounds, scale);
            } else {
                let view = native_controls::create_native_combo_box(
                    config.items,
                    config.selected_index,
                    config.editable,
                );
                native_controls::set_native_combo_box_editable(view, config.editable);
                native_controls::set_native_combo_box_completes(view, config.completes);
                if let Some(value) = config.value {
                    native_controls::set_native_combo_box_string_value(view, value);
                }
                native_controls::set_native_control_enabled(view, config.enabled);
                let delegate = native_controls::set_native_combo_box_delegate(
                    view,
                    into_native_combo_box_callbacks(config.callbacks),
                );
                *state =
                    NativeControlState::new(view as *mut c_void, delegate, cleanup_combo_box);
                ensure_attached(state, parent, bounds, scale);
            }
        }
    }

    fn update_image_view(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: ImageViewConfig,
    ) {
        unsafe {
            if state.is_initialized() {
                let view = state.view() as id;
                if let Some(sf_symbol) = config.sf_symbol {
                    native_controls::set_native_image_view_sf_symbol(view, sf_symbol);
                } else if config.image_data.is_none() {
                    native_controls::clear_native_image_view_image(view);
                }
                if let Some((name, size, weight)) = config.sf_symbol_config {
                    native_controls::set_native_image_view_sf_symbol_config(
                        view, name, size, weight,
                    );
                }
                if let Some(data) = config.image_data {
                    native_controls::set_native_image_view_image_from_data(view, data);
                }
                if let Some(scaling) = config.scaling {
                    native_controls::set_native_image_view_scaling(view, scaling);
                }
                if let Some((r, g, b, a)) = config.tint_color {
                    native_controls::set_native_image_view_content_tint_color(view, r, g, b, a);
                }
                native_controls::set_native_image_view_enabled(view, config.enabled);
                ensure_attached(state, parent, bounds, scale);
            } else {
                let view = native_controls::create_native_image_view();
                if let Some(sf_symbol) = config.sf_symbol {
                    native_controls::set_native_image_view_sf_symbol(view, sf_symbol);
                }
                if let Some((name, size, weight)) = config.sf_symbol_config {
                    native_controls::set_native_image_view_sf_symbol_config(
                        view, name, size, weight,
                    );
                }
                if let Some(data) = config.image_data {
                    native_controls::set_native_image_view_image_from_data(view, data);
                }
                if let Some(scaling) = config.scaling {
                    native_controls::set_native_image_view_scaling(view, scaling);
                }
                if let Some((r, g, b, a)) = config.tint_color {
                    native_controls::set_native_image_view_content_tint_color(view, r, g, b, a);
                }
                native_controls::set_native_image_view_enabled(view, config.enabled);
                *state = NativeControlState::new(
                    view as *mut c_void,
                    std::ptr::null_mut(),
                    cleanup_image_view,
                );
                ensure_attached(state, parent, bounds, scale);
            }
        }
    }

    fn update_tab_view(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: TabViewConfig,
    ) {
        unsafe {
            if state.is_initialized() {
                let view = state.view() as id;
                native_controls::set_native_tab_view_items(
                    view,
                    config.labels,
                    config.labels.len(),
                );
                native_controls::set_native_tab_view_selected(view, config.selected_index);
                native_controls::set_native_control_enabled(view, config.enabled);
                if let Some(callback) = config.on_select {
                    native_controls::release_native_tab_view_target(state.target());
                    let target = native_controls::set_native_tab_view_action(view, Some(callback));
                    state.set_target(target);
                }
                ensure_attached(state, parent, bounds, scale);
            } else {
                let view = native_controls::create_native_tab_view();
                native_controls::set_native_tab_view_items(
                    view,
                    config.labels,
                    config.labels.len(),
                );
                native_controls::set_native_tab_view_selected(view, config.selected_index);
                native_controls::set_native_control_enabled(view, config.enabled);
                let target = if let Some(callback) = config.on_select {
                    native_controls::set_native_tab_view_action(view, Some(callback))
                } else {
                    std::ptr::null_mut()
                };
                *state = NativeControlState::new(view as *mut c_void, target, cleanup_tab_view);
                ensure_attached(state, parent, bounds, scale);
            }
        }
    }

    fn update_table_view(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: TableViewConfig,
    ) {
        unsafe {
            if state.is_initialized() {
                let view = state.view() as id;
                if let Some(title) = config.column_title {
                    native_controls::set_native_table_column_title(view, title);
                }
                if let Some(width) = config.column_width {
                    native_controls::set_native_table_column_width(view, width);
                }
                native_controls::release_native_table_target(state.target());
                let target = native_controls::set_native_table_items(
                    view,
                    config.items,
                    config.selected_index,
                    config.on_select,
                );
                state.set_target(target);
                if let Some(row_height) = config.row_height {
                    native_controls::set_native_table_row_height(view, row_height);
                }
                if let Some(row_size_style) = config.row_size_style {
                    native_controls::set_native_table_row_size_style(view, row_size_style);
                }
                if let Some(style) = config.style {
                    native_controls::set_native_table_style(view, style);
                }
                if let Some(highlight_style) = config.highlight_style {
                    native_controls::set_native_table_selection_highlight_style(
                        view,
                        highlight_style,
                    );
                }
                if let Some(grid_style) = config.grid_style {
                    native_controls::set_native_table_grid_style(view, grid_style);
                }
                native_controls::set_native_table_uses_alternating_rows(
                    view,
                    config.alternating_rows,
                );
                native_controls::set_native_table_allows_multiple_selection(
                    view,
                    config.multiple_selection,
                );
                native_controls::set_native_table_show_header(view, config.show_header);
                ensure_attached(state, parent, bounds, scale);
            } else {
                let view = native_controls::create_native_table_view();
                if let Some(title) = config.column_title {
                    native_controls::set_native_table_column_title(view, title);
                }
                if let Some(width) = config.column_width {
                    native_controls::set_native_table_column_width(view, width);
                }
                let target = native_controls::set_native_table_items(
                    view,
                    config.items,
                    config.selected_index,
                    config.on_select,
                );
                if let Some(row_height) = config.row_height {
                    native_controls::set_native_table_row_height(view, row_height);
                }
                if let Some(row_size_style) = config.row_size_style {
                    native_controls::set_native_table_row_size_style(view, row_size_style);
                }
                if let Some(style) = config.style {
                    native_controls::set_native_table_style(view, style);
                }
                if let Some(highlight_style) = config.highlight_style {
                    native_controls::set_native_table_selection_highlight_style(
                        view,
                        highlight_style,
                    );
                }
                if let Some(grid_style) = config.grid_style {
                    native_controls::set_native_table_grid_style(view, grid_style);
                }
                native_controls::set_native_table_uses_alternating_rows(
                    view,
                    config.alternating_rows,
                );
                native_controls::set_native_table_allows_multiple_selection(
                    view,
                    config.multiple_selection,
                );
                native_controls::set_native_table_show_header(view, config.show_header);
                *state = NativeControlState::new(
                    view as *mut c_void,
                    target,
                    cleanup_table_view,
                );
                ensure_attached(state, parent, bounds, scale);
            }
        }
    }

    fn update_outline_view(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: OutlineViewConfig,
    ) {
        unsafe {
            if state.is_initialized() {
                let view = state.view() as id;
                let nodes = convert_outline_nodes(config.nodes);
                native_controls::release_native_outline_target(state.target());
                let target = native_controls::set_native_outline_items(
                    view,
                    &nodes,
                    config.selected_row,
                    config.expand_all,
                    config.on_select,
                );
                state.set_target(target);
                native_controls::sync_native_outline_column_width(view);
                if let Some(highlight_style) = config.highlight_style {
                    native_controls::set_native_outline_highlight_style(view, highlight_style);
                }
                if let Some(row_height) = config.row_height {
                    native_controls::set_native_outline_row_height(view, row_height);
                }
                ensure_attached(state, parent, bounds, scale);
            } else {
                let view = native_controls::create_native_outline_view();
                let nodes = convert_outline_nodes(config.nodes);
                let target = native_controls::set_native_outline_items(
                    view,
                    &nodes,
                    config.selected_row,
                    config.expand_all,
                    config.on_select,
                );
                native_controls::sync_native_outline_column_width(view);
                if let Some(highlight_style) = config.highlight_style {
                    native_controls::set_native_outline_highlight_style(view, highlight_style);
                }
                if let Some(row_height) = config.row_height {
                    native_controls::set_native_outline_row_height(view, row_height);
                }
                *state = NativeControlState::new(
                    view as *mut c_void,
                    target,
                    cleanup_outline_view,
                );
                ensure_attached(state, parent, bounds, scale);
            }
        }
    }

    fn update_collection_view(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: CollectionViewConfig,
    ) {
        unsafe {
            if state.is_initialized() {
                let view = state.view() as id;
                native_controls::set_native_collection_layout(
                    view,
                    config.width,
                    config.columns,
                    config.item_height,
                    config.spacing,
                );
                native_controls::release_native_collection_target(state.target());
                let target = native_controls::set_native_collection_data_source(
                    view,
                    config.items,
                    config.selected,
                    convert_collection_item_style(config.item_style),
                    config.on_select,
                );
                state.set_target(target);
                ensure_attached(state, parent, bounds, scale);
            } else {
                let view = native_controls::create_native_collection_view();
                native_controls::set_native_collection_layout(
                    view,
                    config.width,
                    config.columns,
                    config.item_height,
                    config.spacing,
                );
                let target = native_controls::set_native_collection_data_source(
                    view,
                    config.items,
                    config.selected,
                    convert_collection_item_style(config.item_style),
                    config.on_select,
                );
                *state = NativeControlState::new(
                    view as *mut c_void,
                    target,
                    cleanup_collection_view,
                );
                ensure_attached(state, parent, bounds, scale);
            }
        }
    }

    fn update_menu_button(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: MenuButtonConfig,
    ) {
        unsafe {
            if state.is_initialized() {
                let view = state.view() as id;
                native_controls::set_native_menu_button_title(view, config.title);
                let native_items = convert_menu_items(config.items);
                native_controls::release_native_menu_button_target(state.target());
                let target = native_controls::set_native_menu_button_items(
                    view,
                    &native_items,
                    config.on_select,
                );
                state.set_target(target);
                native_controls::set_native_control_enabled(view, config.enabled);
                ensure_attached(state, parent, bounds, scale);
            } else {
                let view = if config.context_menu {
                    native_controls::create_native_context_menu_button(config.title)
                } else {
                    native_controls::create_native_menu_button(config.title)
                };
                let native_items = convert_menu_items(config.items);
                let target = native_controls::set_native_menu_button_items(
                    view,
                    &native_items,
                    config.on_select,
                );
                native_controls::set_native_control_enabled(view, config.enabled);
                *state =
                    NativeControlState::new(view as *mut c_void, target, cleanup_menu_button);
                ensure_attached(state, parent, bounds, scale);
            }
        }
    }

    fn update_visual_effect_view(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: VisualEffectViewConfig,
    ) {
        unsafe {
            if state.is_initialized() {
                let view = state.view() as id;
                native_controls::set_native_visual_effect_material(view, config.material);
                native_controls::set_native_visual_effect_blending_mode(view, config.blending_mode);
                native_controls::set_native_visual_effect_state(view, config.state);
                native_controls::set_native_visual_effect_emphasized(view, config.emphasized);
                native_controls::set_native_visual_effect_corner_radius(view, config.corner_radius);
                ensure_attached(state, parent, bounds, scale);
            } else {
                let view = native_controls::create_native_visual_effect_view();
                native_controls::set_native_visual_effect_material(view, config.material);
                native_controls::set_native_visual_effect_blending_mode(view, config.blending_mode);
                native_controls::set_native_visual_effect_state(view, config.state);
                native_controls::set_native_visual_effect_emphasized(view, config.emphasized);
                native_controls::set_native_visual_effect_corner_radius(view, config.corner_radius);
                *state = NativeControlState::new(
                    view as *mut c_void,
                    std::ptr::null_mut(),
                    cleanup_visual_effect_view,
                );
                ensure_attached(state, parent, bounds, scale);
            }
        }
    }

    fn update_glass_effect_view(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: GlassEffectViewConfig,
    ) {
        unsafe {
            if state.is_initialized() {
                let view = state.view() as id;
                native_controls::set_native_glass_effect_style(view, config.style);
                native_controls::set_native_glass_effect_corner_radius(view, config.corner_radius);
                if let Some((r, g, b, a)) = config.tint_color {
                    native_controls::set_native_glass_effect_tint_color(view, r, g, b, a);
                } else {
                    native_controls::clear_native_glass_effect_tint_color(view);
                }
                ensure_attached(state, parent, bounds, scale);
            } else {
                let view = native_controls::create_native_glass_effect_view();
                native_controls::set_native_glass_effect_style(view, config.style);
                native_controls::set_native_glass_effect_corner_radius(view, config.corner_radius);
                if let Some((r, g, b, a)) = config.tint_color {
                    native_controls::set_native_glass_effect_tint_color(view, r, g, b, a);
                } else {
                    native_controls::clear_native_glass_effect_tint_color(view);
                }
                *state = NativeControlState::new(
                    view as *mut c_void,
                    std::ptr::null_mut(),
                    cleanup_glass_effect_view,
                );
                ensure_attached(state, parent, bounds, scale);
            }
        }
    }

    fn update_tracking_view(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: TrackingViewConfig,
    ) {
        unsafe {
            if state.is_initialized() {
                let view = state.view() as id;
                if has_tracking_view_callbacks(&config.callbacks) {
                    // set_native_tracking_view_callbacks already frees the old
                    // callbacks via the ObjC ivar, so do NOT also call
                    // release_native_tracking_view_target — that would double-free.
                    let target = native_controls::set_native_tracking_view_callbacks(
                        view,
                        into_native_tracking_view_callbacks(config.callbacks),
                    );
                    state.set_target(target);
                }
                ensure_attached(state, parent, bounds, scale);
            } else {
                let view = native_controls::create_native_tracking_view();
                let target = native_controls::set_native_tracking_view_callbacks(
                    view,
                    into_native_tracking_view_callbacks(config.callbacks),
                );
                *state =
                    NativeControlState::new(view as *mut c_void, target, cleanup_tracking_view);
                ensure_attached(state, parent, bounds, scale);
            }
        }
    }

    fn update_stack_view(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: StackViewConfig,
    ) {
        unsafe {
            if state.is_initialized() {
                let view = state.view() as id;
                native_controls::set_native_stack_view_spacing(view, config.spacing);
                native_controls::set_native_stack_view_alignment(view, config.alignment);
                native_controls::set_native_stack_view_distribution(view, config.distribution);
                native_controls::set_native_stack_view_edge_insets(
                    view,
                    config.edge_insets.0,
                    config.edge_insets.1,
                    config.edge_insets.2,
                    config.edge_insets.3,
                );
                native_controls::set_native_stack_view_detach_hidden(view, config.detach_hidden);
                // Re-add children
                native_controls::remove_all_native_stack_view_arranged_subviews(view);
                for child in &config.children {
                    native_controls::add_native_stack_view_arranged_subview(view, *child as id);
                }
                ensure_attached(state, parent, bounds, scale);
            } else {
                let view = native_controls::create_native_stack_view(config.orientation);
                native_controls::set_native_stack_view_spacing(view, config.spacing);
                native_controls::set_native_stack_view_alignment(view, config.alignment);
                native_controls::set_native_stack_view_distribution(view, config.distribution);
                native_controls::set_native_stack_view_edge_insets(
                    view,
                    config.edge_insets.0,
                    config.edge_insets.1,
                    config.edge_insets.2,
                    config.edge_insets.3,
                );
                native_controls::set_native_stack_view_detach_hidden(view, config.detach_hidden);
                for child in &config.children {
                    native_controls::add_native_stack_view_arranged_subview(view, *child as id);
                }
                *state = NativeControlState::new(
                    view as *mut c_void,
                    std::ptr::null_mut(),
                    cleanup_stack_view,
                );
                ensure_attached(state, parent, bounds, scale);
            }
        }
    }

    fn update_sidebar(
        &self,
        state: &mut NativeControlState,
        _parent: *mut c_void,
        _bounds: Bounds<Pixels>,
        _scale: f32,
        config: SidebarViewConfig,
    ) {
        unsafe {
            if state.is_initialized() {
                let view = state.view() as id;
                native_controls::set_sidebar_width(
                    view,
                    config.sidebar_width,
                    config.min_width,
                    config.max_width,
                );
                native_controls::set_sidebar_collapsed(
                    view,
                    config.collapsed,
                    config.expanded_width,
                    config.min_width,
                    config.max_width,
                );
                native_controls::release_sidebar_target(state.target());
                let target = native_controls::set_sidebar_items(
                    view,
                    config.items,
                    config.selected_index,
                    config.min_width,
                    config.max_width,
                    config.on_select,
                );
                state.set_target(target);
                if let Some(header_title) = config.header_title {
                    native_controls::set_sidebar_header(
                        view,
                        Some(header_title),
                        config.header_button_symbols,
                    );
                }
                if let Some(on_header_button) = config.on_header_button {
                    native_controls::update_sidebar_header_callback(
                        view,
                        Some(on_header_button),
                    );
                }
                if let Some((r, g, b, a)) = config.background_color {
                    native_controls::set_sidebar_background_color(view, r, g, b, a);
                } else {
                    native_controls::clear_sidebar_background_color(view);
                }
                // The sidebar's view attachment is handled by
                // configure_native_sidebar_window (via configure_hosted_content),
                // NOT by the generic ensure_attached/attach_and_position path.
                // Calling addSubview: here during paint overflows the stack.
            } else {
                let view = native_controls::create_sidebar(
                    config.sidebar_width,
                    config.min_width,
                    config.max_width,
                    config.embed_in_host,
                );
                native_controls::set_sidebar_collapsed(
                    view,
                    config.collapsed,
                    config.expanded_width,
                    config.min_width,
                    config.max_width,
                );
                let target = native_controls::set_sidebar_items(
                    view,
                    config.items,
                    config.selected_index,
                    config.min_width,
                    config.max_width,
                    config.on_select,
                );
                if let Some(header_title) = config.header_title {
                    native_controls::set_sidebar_header(
                        view,
                        Some(header_title),
                        config.header_button_symbols,
                    );
                }
                if let Some(on_header_button) = config.on_header_button {
                    native_controls::update_sidebar_header_callback(
                        view,
                        Some(on_header_button),
                    );
                }
                if let Some((r, g, b, a)) = config.background_color {
                    native_controls::set_sidebar_background_color(view, r, g, b, a);
                } else {
                    native_controls::clear_sidebar_background_color(view);
                }
                *state = NativeControlState::new(
                    view as *mut c_void,
                    target,
                    cleanup_sidebar,
                );
            }
        }
    }

    fn is_glass_effect_available(&self) -> bool {
        native_controls::is_glass_effect_available()
    }

    fn get_text_field_value(&self, state: &NativeControlState) -> String {
        unsafe { native_controls::get_native_text_field_string_value(state.view() as id) }
    }

    fn get_combo_box_value(&self, state: &NativeControlState) -> String {
        unsafe { native_controls::get_native_combo_box_string_value(state.view() as id) }
    }

    fn show_context_menu(
        &self,
        items: &[NativeMenuItemData],
        view: *mut c_void,
        x: f64,
        y: f64,
        on_result: Box<dyn FnOnce(Option<usize>)>,
    ) {
        unsafe {
            let native_items = convert_menu_items(items);
            native_controls::show_popup_menu_deferred(&native_items, view as id, x, y, on_result);
        }
    }

    fn show_alert_modal(&self, config: AlertConfig) -> i64 {
        unsafe {
            let alert = native_controls::create_native_alert(
                convert_alert_style(config.style),
                config.message,
                config.informative_text,
                config.button_titles,
                config.shows_suppression_button,
            );
            let result = native_controls::run_native_alert_modal(alert);
            native_controls::release_native_alert(alert);
            result
        }
    }

    fn show_alert_sheet(
        &self,
        config: AlertConfig,
        parent_window: *mut c_void,
        callback: Option<Box<dyn FnOnce(i64)>>,
    ) {
        unsafe {
            let alert = native_controls::create_native_alert(
                convert_alert_style(config.style),
                config.message,
                config.informative_text,
                config.button_titles,
                config.shows_suppression_button,
            );
            native_controls::run_native_alert_as_sheet(alert, parent_window as id, callback);
        }
    }

    fn create_panel(&self, config: PanelConfig) -> NativeControlState {
        unsafe {
            let (panel, delegate) = native_controls::create_native_panel(
                config.width,
                config.height,
                convert_panel_style(config.style),
                convert_panel_level(config.level),
                config.non_activating,
                config.has_shadow,
                config.corner_radius,
                convert_panel_material(config.material),
                config.on_close,
            );
            NativeControlState::new(panel as *mut c_void, delegate, cleanup_panel)
        }
    }

    fn get_panel_content_view(&self, state: &NativeControlState) -> *mut c_void {
        unsafe { native_controls::get_native_panel_content_view(state.view() as id) as *mut c_void }
    }

    fn show_panel(&self, state: &NativeControlState) {
        unsafe { native_controls::show_native_panel(state.view() as id) }
    }

    fn show_panel_centered(&self, state: &NativeControlState) {
        unsafe { native_controls::show_native_panel_centered(state.view() as id) }
    }

    fn set_panel_origin(&self, state: &NativeControlState, x: f64, y: f64) {
        unsafe { native_controls::set_native_panel_frame_origin(state.view() as id, x, y) }
    }

    fn set_panel_top_left(&self, state: &NativeControlState, x: f64, y: f64) {
        unsafe { native_controls::set_native_panel_frame_top_left(state.view() as id, x, y) }
    }

    fn set_panel_size(&self, state: &NativeControlState, width: f64, height: f64) {
        unsafe { native_controls::set_native_panel_size(state.view() as id, width, height) }
    }

    fn set_panel_frame(
        &self,
        state: &NativeControlState,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        animate: bool,
    ) {
        unsafe {
            native_controls::set_native_panel_frame(
                state.view() as id,
                x,
                y,
                width,
                height,
                animate,
            )
        }
    }

    fn close_panel(&self, state: &NativeControlState) {
        unsafe { native_controls::close_native_panel(state.view() as id) }
    }

    fn hide_panel(&self, state: &NativeControlState) {
        unsafe { native_controls::hide_native_panel(state.view() as id) }
    }

    fn is_panel_visible(&self, state: &NativeControlState) -> bool {
        unsafe { native_controls::is_native_panel_visible(state.view() as id) }
    }

    fn get_toolbar_item_frame(&self, window: *mut c_void, item_id: &str) -> Option<Bounds<Pixels>> {
        unsafe {
            native_controls::get_toolbar_item_screen_frame(window as id, item_id)
                .map(bounds_from_ns_rect)
        }
    }

    fn create_popover(&self, config: PopoverConfig) -> NativeControlState {
        unsafe {
            let (popover, delegate) = native_controls::create_native_popover(
                config.width,
                config.height,
                config.behavior,
                config.on_close,
                config.on_show,
            );
            NativeControlState::new(popover as *mut c_void, delegate, cleanup_popover)
        }
    }

    fn get_popover_content_view(&self, state: &NativeControlState) -> *mut c_void {
        unsafe {
            native_controls::get_native_popover_content_view(state.view() as id) as *mut c_void
        }
    }

    fn show_popover_at_toolbar_item(&self, state: &NativeControlState, toolbar_item: *mut c_void) {
        unsafe {
            native_controls::show_native_popover_relative_to_toolbar_item(
                state.view() as id,
                toolbar_item as id,
            );
        }
    }

    fn dismiss_popover(&self, state: &NativeControlState) {
        unsafe { native_controls::dismiss_native_popover(state.view() as id) }
    }

}

fn cleanup_panel_impl(view: *mut c_void, delegate: *mut c_void) {
    unsafe {
        if !view.is_null() {
            native_controls::release_native_panel(view as id, delegate);
        }
    }
}

unsafe fn cleanup_panel(view: *mut c_void, delegate: *mut c_void) {
    cleanup_panel_impl(view, delegate);
}

fn cleanup_popover_impl(view: *mut c_void, delegate: *mut c_void) {
    unsafe {
        if !view.is_null() {
            native_controls::release_native_popover(view as id, delegate);
        }
    }
}

unsafe fn cleanup_popover(view: *mut c_void, delegate: *mut c_void) {
    cleanup_popover_impl(view, delegate);
}

fn apply_button_style(button: id, style: ButtonStyle) {
    unsafe {
        match style {
            ButtonStyle::Rounded => {
                native_controls::set_native_button_bezel_style(button, 1); // NSBezelStyleRounded
                native_controls::set_native_button_bordered(button, true);
            }
            ButtonStyle::Filled => {
                native_controls::set_native_button_bezel_style(button, 1);
                native_controls::set_native_button_bordered(button, true);
                native_controls::set_native_button_bezel_color_accent(button);
            }
            ButtonStyle::Inline => {
                native_controls::set_native_button_bezel_style(button, 15); // NSBezelStyleInline
                native_controls::set_native_button_bordered(button, true);
            }
            ButtonStyle::Borderless => {
                native_controls::set_native_button_bordered(button, false);
                native_controls::set_native_button_shows_border_on_hover(button, true);
            }
        }
    }
}

fn apply_button_tint(button: id, tint: Option<(f64, f64, f64, f64)>) {
    unsafe {
        if let Some((r, g, b, a)) = tint {
            native_controls::set_native_button_bezel_color(button, r, g, b, a);
            native_controls::set_native_button_content_tint_color(button, 1.0, 1.0, 1.0, 1.0);
        }
    }
}
