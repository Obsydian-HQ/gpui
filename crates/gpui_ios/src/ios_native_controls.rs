use std::ffi::c_void;

use gpui::native_controls::{
    AlertConfig, ButtonConfig, ButtonStyle,
    CheckboxConfig, CollectionViewConfig, ComboBoxCallbacks,
    ComboBoxConfig, GlassEffectViewConfig, ImageViewConfig, MenuButtonConfig, NativeControlState,
    NativeMenuItemData, NativeOutlineNodeData, OutlineViewConfig, PanelConfig, PlatformNativeControls, PopupButtonConfig, PopoverConfig,
    ProgressConfig, SearchFieldConfig, SegmentedControlConfig, SidebarViewConfig, SliderConfig,
    StackViewConfig, StepperConfig, SwitchConfig, TabViewConfig, TableViewConfig,
    TextFieldCallbacks, TextFieldConfig, TrackingViewConfig,
    VisualEffectViewConfig, ALERT_FIRST_BUTTON_RETURN,
};
use gpui::{Bounds, Pixels};
use objc::{class, msg_send, sel, sel_impl};

use crate::native_controls::{self, id};

pub struct IosNativeControls;

pub static IOS_NATIVE_CONTROLS: IosNativeControls = IosNativeControls;

unsafe fn cleanup_button(view: *mut c_void, target: *mut c_void) { unsafe {
    if !target.is_null() {
        native_controls::release_native_button_target(target);
    }
    if !view.is_null() {
        native_controls::remove_native_view_from_parent(view as id);
        native_controls::release_native_button(view as id);
    }
}}

unsafe fn cleanup_checkbox(view: *mut c_void, target: *mut c_void) { unsafe {
    if !target.is_null() {
        native_controls::release_native_checkbox_target(target);
    }
    if !view.is_null() {
        native_controls::remove_native_view_from_parent(view as id);
        native_controls::release_native_checkbox(view as id);
    }
}}

unsafe fn cleanup_switch(view: *mut c_void, target: *mut c_void) { unsafe {
    if !target.is_null() {
        native_controls::release_native_switch_target(target);
    }
    if !view.is_null() {
        native_controls::remove_native_view_from_parent(view as id);
        native_controls::release_native_switch(view as id);
    }
}}

unsafe fn cleanup_slider(view: *mut c_void, target: *mut c_void) { unsafe {
    if !target.is_null() {
        native_controls::release_native_slider_target(target);
    }
    if !view.is_null() {
        native_controls::remove_native_view_from_parent(view as id);
        native_controls::release_native_slider(view as id);
    }
}}

unsafe fn cleanup_stepper(view: *mut c_void, target: *mut c_void) { unsafe {
    if !target.is_null() {
        native_controls::release_native_stepper_target(target);
    }
    if !view.is_null() {
        native_controls::remove_native_view_from_parent(view as id);
        native_controls::release_native_stepper(view as id);
    }
}}

unsafe fn cleanup_segmented_control(view: *mut c_void, target: *mut c_void) { unsafe {
    if !target.is_null() {
        native_controls::release_native_segmented_target(target);
    }
    if !view.is_null() {
        native_controls::remove_native_view_from_parent(view as id);
        native_controls::release_native_segmented_control(view as id);
    }
}}

unsafe fn cleanup_popup_button(view: *mut c_void, target: *mut c_void) { unsafe {
    if !target.is_null() {
        native_controls::release_native_popup_target(target);
    }
    if !view.is_null() {
        native_controls::remove_native_view_from_parent(view as id);
        native_controls::release_native_popup_button(view as id);
    }
}}

unsafe fn cleanup_progress(view: *mut c_void, _target: *mut c_void) { unsafe {
    if !view.is_null() {
        native_controls::remove_native_view_from_parent(view as id);
        native_controls::release_native_progress_indicator(view as id);
    }
}}

unsafe fn cleanup_search_field(view: *mut c_void, _target: *mut c_void) { unsafe {
    if !view.is_null() {
        native_controls::remove_native_view_from_parent(view as id);
        native_controls::release_native_search_field(view as id);
    }
}}

unsafe fn cleanup_text_field(view: *mut c_void, target: *mut c_void) { unsafe {
    if !target.is_null() {
        native_controls::release_native_text_field_delegate(target);
    }
    if !view.is_null() {
        native_controls::remove_native_view_from_parent(view as id);
        native_controls::release_native_text_field(view as id);
    }
}}

unsafe fn cleanup_combo_box(view: *mut c_void, target: *mut c_void) { unsafe {
    if !target.is_null() {
        native_controls::release_native_combo_box_delegate(target);
    }
    if !view.is_null() {
        native_controls::remove_native_view_from_parent(view as id);
        native_controls::release_native_combo_box(view as id);
    }
}}

unsafe fn cleanup_image_view(view: *mut c_void, _target: *mut c_void) { unsafe {
    if !view.is_null() {
        native_controls::remove_native_view_from_parent(view as id);
        native_controls::release_native_image_view(view as id);
    }
}}

unsafe fn cleanup_tab_view(view: *mut c_void, target: *mut c_void) { unsafe {
    if !target.is_null() {
        native_controls::release_native_tab_view_target(target);
    }
    if !view.is_null() {
        native_controls::remove_native_view_from_parent(view as id);
        native_controls::release_native_tab_view(view as id);
    }
}}

unsafe fn cleanup_table_view(view: *mut c_void, target: *mut c_void) { unsafe {
    if !target.is_null() {
        native_controls::release_native_table_target(target);
    }
    if !view.is_null() {
        native_controls::remove_native_view_from_parent(view as id);
        native_controls::release_native_table_view(view as id);
    }
}}

unsafe fn cleanup_outline_view(view: *mut c_void, target: *mut c_void) { unsafe {
    if !target.is_null() {
        native_controls::release_native_outline_target(target);
    }
    if !view.is_null() {
        native_controls::remove_native_view_from_parent(view as id);
        native_controls::release_native_outline_view(view as id);
    }
}}

unsafe fn cleanup_collection_view(view: *mut c_void, target: *mut c_void) { unsafe {
    if !target.is_null() {
        native_controls::release_native_collection_target(target);
    }
    if !view.is_null() {
        native_controls::remove_native_view_from_parent(view as id);
        native_controls::release_native_collection_view(view as id);
    }
}}

unsafe fn cleanup_menu_button(view: *mut c_void, target: *mut c_void) { unsafe {
    if !target.is_null() {
        native_controls::release_native_menu_button_target(target);
    }
    if !view.is_null() {
        native_controls::remove_native_view_from_parent(view as id);
        native_controls::release_native_menu_button(view as id);
    }
}}

unsafe fn cleanup_visual_effect_view(view: *mut c_void, _target: *mut c_void) { unsafe {
    if !view.is_null() {
        native_controls::remove_native_view_from_parent(view as id);
        native_controls::release_native_visual_effect_view(view as id);
    }
}}

unsafe fn cleanup_glass_effect_view(view: *mut c_void, _target: *mut c_void) { unsafe {
    if !view.is_null() {
        native_controls::remove_native_view_from_parent(view as id);
        native_controls::release_native_glass_effect_view(view as id);
    }
}}

unsafe fn cleanup_tracking_view(view: *mut c_void, target: *mut c_void) { unsafe {
    if !target.is_null() {
        native_controls::release_native_tracking_view_target(target);
    }
    if !view.is_null() {
        native_controls::remove_native_view_from_parent(view as id);
        native_controls::release_native_tracking_view(view as id);
    }
}}

unsafe fn cleanup_stack_view(view: *mut c_void, _target: *mut c_void) { unsafe {
    if !view.is_null() {
        native_controls::remove_native_view_from_parent(view as id);
        native_controls::release_native_stack_view(view as id);
    }
}}

unsafe fn cleanup_sidebar(view: *mut c_void, target: *mut c_void) { unsafe {
    if !target.is_null() {
        native_controls::release_native_sidebar_target(target);
    }
    if !view.is_null() {
        native_controls::remove_native_view_from_parent(view as id);
        native_controls::release_native_sidebar_view(view as id);
    }
}}

unsafe fn cleanup_panel(view: *mut c_void, _target: *mut c_void) { unsafe {
    if !view.is_null() {
        native_controls::remove_native_view_from_parent(view as id);
        native_controls::release_object(view as id);
    }
}}

unsafe fn cleanup_popover(view: *mut c_void, _target: *mut c_void) { unsafe {
    if !view.is_null() {
        native_controls::remove_native_view_from_parent(view as id);
        native_controls::release_object(view as id);
    }
}}

fn ensure_attached(
    state: &NativeControlState,
    parent: *mut c_void,
    bounds: Bounds<Pixels>,
    scale: f32,
) {
    unsafe {
        IOS_NATIVE_CONTROLS.attach_and_position(state, parent, bounds, scale);
    }
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

fn into_combo_box_callbacks(
    callbacks: ComboBoxCallbacks,
) -> crate::native_controls::TextFieldCallbacks {
    crate::native_controls::TextFieldCallbacks {
        on_change: callbacks.on_change,
        on_begin_editing: None,
        on_end_editing: None,
        on_submit: callbacks.on_submit,
        on_move_up: None,
        on_move_down: None,
        on_cancel: None,
    }
}

fn flatten_menu_items(items: &[NativeMenuItemData]) -> Vec<&str> {
    items.iter()
        .filter_map(|item| match item {
            NativeMenuItemData::Action { title, .. } => Some(title.as_str()),
            NativeMenuItemData::Submenu { title, .. } => Some(title.as_str()),
            NativeMenuItemData::Separator => None,
        })
        .collect()
}

fn flatten_outline_nodes(nodes: &[NativeOutlineNodeData], titles: &mut Vec<String>) {
    for node in nodes {
        titles.push(node.title.clone());
        flatten_outline_nodes(&node.children, titles);
    }
}

fn apply_button_style(button: id, style: ButtonStyle) {
    unsafe {
        match style {
            ButtonStyle::Rounded => {
                native_controls::set_native_button_bezel_style(button, 1);
                native_controls::set_native_button_bordered(button, true);
            }
            ButtonStyle::Filled => {
                native_controls::set_native_button_bezel_style(button, 12);
                native_controls::set_native_button_bordered(button, true);
                native_controls::set_native_button_bezel_color_accent(button);
            }
            ButtonStyle::Inline => {
                native_controls::set_native_button_bezel_style(button, 15);
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

fn normalized_progress(value: f64, min: f64, max: f64) -> f64 {
    if max <= min {
        0.0
    } else {
        ((value - min) / (max - min)).clamp(0.0, 1.0)
    }
}

unsafe fn set_view_frame(
    view: id,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) {
    let frame: ((f64, f64), (f64, f64)) = ((x, y), (width, height));
    let _: () = msg_send![view, setFrame: frame];
}

impl PlatformNativeControls for IosNativeControls {
    unsafe fn attach_and_position(
        &self,
        state: &NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
    ) {
        let view = state.view() as id;
        let parent = parent as id;
        let superview: id = msg_send![view, superview];
        if superview != parent {
            if superview != std::ptr::null_mut() {
                unsafe {
                    native_controls::remove_native_view_from_parent(view);
                }
            }
            unsafe {
                native_controls::attach_native_view_to_parent(view, parent);
            }
        }
        unsafe {
            native_controls::set_native_view_frame(view, bounds, parent, scale);
        }
    }

    unsafe fn remove_from_parent(&self, state: &NativeControlState) { unsafe {
        if !state.view().is_null() {
            native_controls::remove_native_view_from_parent(state.view() as id);
        }
    }}

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
                if let Some(symbol) = config.sf_symbol {
                    native_controls::set_native_button_sf_symbol(
                        view,
                        symbol,
                        config.title.is_empty(),
                    );
                }
                if let Some(callback) = config.on_click {
                    native_controls::release_native_button_target(state.target());
                    let target = native_controls::set_native_button_action(view, callback);
                    state.set_target(target);
                }
            } else {
                let view = native_controls::create_native_button(config.title);
                apply_button_style(view, config.style);
                apply_button_tint(view, config.tint);
                native_controls::set_native_control_enabled(view, config.enabled);
                if let Some(symbol) = config.sf_symbol {
                    native_controls::set_native_button_sf_symbol(
                        view,
                        symbol,
                        config.title.is_empty(),
                    );
                }
                let target = config
                    .on_click
                    .map(|callback| native_controls::set_native_button_action(view, callback))
                    .unwrap_or(std::ptr::null_mut());
                *state = NativeControlState::new(view as *mut c_void, target, cleanup_button);
            }
            ensure_attached(state, parent, bounds, scale);
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
            } else {
                let view = native_controls::create_native_checkbox(config.title);
                native_controls::set_native_checkbox_state(view, config.checked);
                native_controls::set_native_control_enabled(view, config.enabled);
                let target = config
                    .on_change
                    .map(|callback| native_controls::set_native_checkbox_action(view, callback))
                    .unwrap_or(std::ptr::null_mut());
                *state = NativeControlState::new(view as *mut c_void, target, cleanup_checkbox);
            }
            ensure_attached(state, parent, bounds, scale);
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
            } else {
                let view = native_controls::create_native_switch();
                native_controls::set_native_switch_state(view, config.checked);
                native_controls::set_native_control_enabled(view, config.enabled);
                let target = config
                    .on_change
                    .map(|callback| native_controls::set_native_switch_action(view, callback))
                    .unwrap_or(std::ptr::null_mut());
                *state = NativeControlState::new(view as *mut c_void, target, cleanup_switch);
            }
            ensure_attached(state, parent, bounds, scale);
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
                let target = config
                    .on_change
                    .map(|callback| native_controls::set_native_slider_action(view, callback))
                    .unwrap_or(std::ptr::null_mut());
                *state = NativeControlState::new(view as *mut c_void, target, cleanup_slider);
            }
            ensure_attached(state, parent, bounds, scale);
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
                let target = config
                    .on_change
                    .map(|callback| native_controls::set_native_stepper_action(view, callback))
                    .unwrap_or(std::ptr::null_mut());
                *state = NativeControlState::new(view as *mut c_void, target, cleanup_stepper);
            }
            ensure_attached(state, parent, bounds, scale);
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
                let target = config
                    .on_select
                    .map(|callback| native_controls::set_native_segmented_action(view, callback))
                    .unwrap_or(std::ptr::null_mut());
                *state =
                    NativeControlState::new(view as *mut c_void, target, cleanup_segmented_control);
            }
            ensure_attached(state, parent, bounds, scale);
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
                if !state.target().is_null() {
                    native_controls::release_native_popup_target(state.target());
                    state.set_target(std::ptr::null_mut());
                }
                let target = config
                    .on_select
                    .map(|callback| native_controls::set_native_popup_action(callback))
                    .unwrap_or(std::ptr::null_mut());
                native_controls::set_native_popup_items(view, config.items, target);
                native_controls::set_native_popup_selected(view, config.selected_index);
                native_controls::set_native_control_enabled(view, config.enabled);
                state.set_target(target);
            } else {
                let view =
                    native_controls::create_native_popup_button(config.items, config.selected_index);
                let target = config
                    .on_select
                    .map(|callback| native_controls::set_native_popup_action(callback))
                    .unwrap_or(std::ptr::null_mut());
                native_controls::set_native_popup_items(view, config.items, target);
                native_controls::set_native_popup_selected(view, config.selected_index);
                native_controls::set_native_control_enabled(view, config.enabled);
                *state =
                    NativeControlState::new(view as *mut c_void, target, cleanup_popup_button);
            }
            ensure_attached(state, parent, bounds, scale);
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
        let value = normalized_progress(config.value, config.min, config.max);
        unsafe {
            if state.is_initialized() {
                let view = state.view() as id;
                native_controls::set_native_progress_style(view, config.style);
                native_controls::set_native_progress_indeterminate(view, config.indeterminate);
                native_controls::set_native_progress_min_max(view, config.min, config.max);
                native_controls::set_native_progress_value(view, value);
                if config.animating {
                    native_controls::start_native_progress_animation(view);
                } else {
                    native_controls::stop_native_progress_animation(view);
                }
                native_controls::set_native_progress_displayed_when_stopped(
                    view,
                    config.display_when_stopped,
                );
            } else {
                let view = native_controls::create_native_progress_indicator();
                native_controls::set_native_progress_style(view, config.style);
                native_controls::set_native_progress_indeterminate(view, config.indeterminate);
                native_controls::set_native_progress_min_max(view, config.min, config.max);
                native_controls::set_native_progress_value(view, value);
                if config.animating {
                    native_controls::start_native_progress_animation(view);
                } else {
                    native_controls::stop_native_progress_animation(view);
                }
                native_controls::set_native_progress_displayed_when_stopped(
                    view,
                    config.display_when_stopped,
                );
                *state = NativeControlState::new(view as *mut c_void, std::ptr::null_mut(), cleanup_progress);
            }
            ensure_attached(state, parent, bounds, scale);
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
                let _: () = msg_send![view, setUserInteractionEnabled: config.enabled as i8];
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
                let _: () = msg_send![view, setUserInteractionEnabled: config.enabled as i8];
                *state =
                    NativeControlState::new(view as *mut c_void, std::ptr::null_mut(), cleanup_search_field);
            }
            let _ = config.callbacks;
            ensure_attached(state, parent, bounds, scale);
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
                let target = if has_text_field_callbacks(&config.callbacks) {
                    native_controls::set_native_text_field_delegate(
                        view,
                        into_native_text_field_callbacks(config.callbacks),
                    )
                } else {
                    std::ptr::null_mut()
                };
                *state = NativeControlState::new(view as *mut c_void, target, cleanup_text_field);
            }
            ensure_attached(state, parent, bounds, scale);
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
                if let Some(value) = config.value {
                    native_controls::set_native_combo_box_string_value(view, value);
                }
                native_controls::set_native_combo_box_editable(view, config.editable);
                native_controls::set_native_combo_box_completes(view, config.completes);
                let _: () = msg_send![view, setUserInteractionEnabled: config.enabled as i8];
                if has_combo_box_callbacks(&config.callbacks) {
                    native_controls::release_native_combo_box_delegate(state.target());
                    let delegate = native_controls::set_native_combo_box_delegate(
                        view,
                        into_combo_box_callbacks(config.callbacks),
                    );
                    state.set_target(delegate);
                }
            } else {
                let view = native_controls::create_native_combo_box(
                    config.items,
                    Some(config.selected_index),
                    config.editable,
                );
                if let Some(value) = config.value {
                    native_controls::set_native_combo_box_string_value(view, value);
                }
                native_controls::set_native_combo_box_editable(view, config.editable);
                native_controls::set_native_combo_box_completes(view, config.completes);
                let _: () = msg_send![view, setUserInteractionEnabled: config.enabled as i8];
                let target = if has_combo_box_callbacks(&config.callbacks) {
                    native_controls::set_native_combo_box_delegate(
                        view,
                        into_combo_box_callbacks(config.callbacks),
                    )
                } else {
                    std::ptr::null_mut()
                };
                *state = NativeControlState::new(view as *mut c_void, target, cleanup_combo_box);
            }
            ensure_attached(state, parent, bounds, scale);
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
            let view = if state.is_initialized() {
                state.view() as id
            } else {
                let view = native_controls::create_native_image_view();
                *state =
                    NativeControlState::new(view as *mut c_void, std::ptr::null_mut(), cleanup_image_view);
                view
            };

            if let Some(symbol) = config.sf_symbol {
                if let Some((_, point_size, weight)) = config.sf_symbol_config {
                    native_controls::set_native_image_view_sf_symbol_config(
                        view,
                        symbol,
                        point_size,
                        weight,
                    );
                } else {
                    native_controls::set_native_image_view_sf_symbol(view, symbol);
                }
            } else if let Some(data) = config.image_data {
                native_controls::set_native_image_view_image_from_data(view, data);
            } else {
                native_controls::clear_native_image_view_image(view);
            }

            if let Some(scaling) = config.scaling {
                native_controls::set_native_image_view_scaling(view, scaling);
            }
            if let Some((r, g, b, a)) = config.tint_color {
                native_controls::set_native_image_view_content_tint_color(view, r, g, b, a);
            }
            native_controls::set_native_image_view_enabled(view, config.enabled);
            ensure_attached(state, parent, bounds, scale);
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
                native_controls::set_native_tab_view_items(view, config.labels);
                native_controls::set_native_tab_view_selected(view, config.selected_index);
                native_controls::set_native_control_enabled(view, config.enabled);
                if let Some(callback) = config.on_select {
                    native_controls::release_native_tab_view_target(state.target());
                    let target = native_controls::set_native_tab_view_action(view, callback);
                    state.set_target(target);
                }
            } else {
                let view = native_controls::create_native_tab_view();
                native_controls::set_native_tab_view_items(view, config.labels);
                native_controls::set_native_tab_view_selected(view, config.selected_index);
                native_controls::set_native_control_enabled(view, config.enabled);
                let target = config
                    .on_select
                    .map(|callback| native_controls::set_native_tab_view_action(view, callback))
                    .unwrap_or(std::ptr::null_mut());
                *state = NativeControlState::new(view as *mut c_void, target, cleanup_tab_view);
            }
            ensure_attached(state, parent, bounds, scale);
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
            let rows = config
                .items
                .iter()
                .map(|item| crate::native_controls::IosTableRow {
                    text: (*item).to_string(),
                })
                .collect::<Vec<_>>();

            if state.is_initialized() {
                let view = state.view() as id;
                native_controls::release_native_table_target(state.target());
                let target =
                    native_controls::set_native_table_items(view, rows, config.on_select, config.selected_index);
                state.set_target(target);
                if let Some(title) = config.column_title {
                    native_controls::set_native_table_column_title(view, title);
                }
                if let Some(width) = config.column_width {
                    native_controls::set_native_table_column_width(view, width);
                }
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
                    native_controls::set_native_table_selection_highlight_style(view, highlight_style);
                }
                if let Some(grid_style) = config.grid_style {
                    native_controls::set_native_table_grid_style(view, grid_style);
                }
                native_controls::set_native_table_uses_alternating_rows(view, config.alternating_rows);
                native_controls::set_native_table_allows_multiple_selection(
                    view,
                    config.multiple_selection,
                );
                native_controls::set_native_table_show_header(view, config.show_header);
            } else {
                let view = native_controls::create_native_table_view();
                let target =
                    native_controls::set_native_table_items(view, rows, config.on_select, config.selected_index);
                if let Some(title) = config.column_title {
                    native_controls::set_native_table_column_title(view, title);
                }
                if let Some(width) = config.column_width {
                    native_controls::set_native_table_column_width(view, width);
                }
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
                    native_controls::set_native_table_selection_highlight_style(view, highlight_style);
                }
                if let Some(grid_style) = config.grid_style {
                    native_controls::set_native_table_grid_style(view, grid_style);
                }
                native_controls::set_native_table_uses_alternating_rows(view, config.alternating_rows);
                native_controls::set_native_table_allows_multiple_selection(
                    view,
                    config.multiple_selection,
                );
                native_controls::set_native_table_show_header(view, config.show_header);
                *state = NativeControlState::new(view as *mut c_void, target, cleanup_table_view);
            }
            ensure_attached(state, parent, bounds, scale);
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
            let mut titles = Vec::new();
            flatten_outline_nodes(config.nodes, &mut titles);
            if state.is_initialized() {
                let view = state.view() as id;
                native_controls::release_native_outline_target(state.target());
                let target = native_controls::set_native_outline_items(
                    view,
                    titles,
                    config.on_select,
                    config.selected_row,
                );
                state.set_target(target);
                if let Some(highlight_style) = config.highlight_style {
                    native_controls::set_native_outline_highlight_style(view, highlight_style);
                }
                if let Some(row_height) = config.row_height {
                    native_controls::set_native_outline_row_height(view, row_height);
                }
                native_controls::sync_native_outline_column_width(view);
            } else {
                let view = native_controls::create_native_outline_view();
                let target = native_controls::set_native_outline_items(
                    view,
                    titles,
                    config.on_select,
                    config.selected_row,
                );
                if let Some(highlight_style) = config.highlight_style {
                    native_controls::set_native_outline_highlight_style(view, highlight_style);
                }
                if let Some(row_height) = config.row_height {
                    native_controls::set_native_outline_row_height(view, row_height);
                }
                native_controls::sync_native_outline_column_width(view);
                *state = NativeControlState::new(view as *mut c_void, target, cleanup_outline_view);
            }
            let _ = config.expand_all;
            ensure_attached(state, parent, bounds, scale);
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
            let items = config
                .items
                .iter()
                .map(|item| crate::native_controls::IosCollectionItem {
                    text: (*item).to_string(),
                })
                .collect::<Vec<_>>();

            if state.is_initialized() {
                let view = state.view() as id;
                native_controls::release_native_collection_target(state.target());
                let target = native_controls::set_native_collection_data_source(
                    view,
                    items,
                    config.on_select,
                    config.selected,
                );
                state.set_target(target);
                let item_width = (config.width - ((config.columns.saturating_sub(1)) as f64 * config.spacing))
                    / config.columns.max(1) as f64;
                native_controls::set_native_collection_layout(
                    view,
                    item_width.max(1.0),
                    config.item_height,
                    config.spacing,
                );
            } else {
                let view = native_controls::create_native_collection_view();
                let target = native_controls::set_native_collection_data_source(
                    view,
                    items,
                    config.on_select,
                    config.selected,
                );
                let item_width = (config.width - ((config.columns.saturating_sub(1)) as f64 * config.spacing))
                    / config.columns.max(1) as f64;
                native_controls::set_native_collection_layout(
                    view,
                    item_width.max(1.0),
                    config.item_height,
                    config.spacing,
                );
                *state =
                    NativeControlState::new(view as *mut c_void, target, cleanup_collection_view);
            }
            let _ = config.item_style;
            ensure_attached(state, parent, bounds, scale);
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
            let flat_items = flatten_menu_items(config.items);
            if state.is_initialized() {
                let view = state.view() as id;
                native_controls::set_native_menu_button_title(view, config.title);
                if !state.target().is_null() {
                    native_controls::release_native_menu_button_target(state.target());
                    state.set_target(std::ptr::null_mut());
                }
                let target = config
                    .on_select
                    .map(|callback| native_controls::create_native_menu_target(callback))
                    .unwrap_or(std::ptr::null_mut());
                native_controls::set_native_menu_button_items(view, &flat_items, target);
                native_controls::set_native_control_enabled(view, config.enabled);
                state.set_target(target);
            } else {
                let view = if config.context_menu {
                    native_controls::create_native_context_menu_button(config.title)
                } else {
                    native_controls::create_native_menu_button(config.title)
                };
                let target = if let Some(callback) = config.on_select {
                    native_controls::create_native_menu_target(callback)
                } else {
                    std::ptr::null_mut()
                };
                native_controls::set_native_menu_button_items(view, &flat_items, target);
                native_controls::set_native_control_enabled(view, config.enabled);
                *state = NativeControlState::new(view as *mut c_void, target, cleanup_menu_button);
            }
            ensure_attached(state, parent, bounds, scale);
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
            let view = if state.is_initialized() {
                state.view() as id
            } else {
                let view = native_controls::create_native_visual_effect_view();
                *state = NativeControlState::new(
                    view as *mut c_void,
                    std::ptr::null_mut(),
                    cleanup_visual_effect_view,
                );
                view
            };
            native_controls::set_native_visual_effect_material(view, config.material);
            native_controls::set_native_visual_effect_blending_mode(view, config.blending_mode);
            native_controls::set_native_visual_effect_state(view, config.state);
            native_controls::set_native_visual_effect_emphasized(view, config.emphasized);
            native_controls::set_native_visual_effect_corner_radius(view, config.corner_radius);
            ensure_attached(state, parent, bounds, scale);
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
            let view = if state.is_initialized() {
                state.view() as id
            } else {
                let view = native_controls::create_native_glass_effect_view();
                *state = NativeControlState::new(
                    view as *mut c_void,
                    std::ptr::null_mut(),
                    cleanup_glass_effect_view,
                );
                view
            };
            native_controls::set_native_glass_effect_style(view, config.style);
            native_controls::set_native_glass_effect_corner_radius(view, config.corner_radius);
            if let Some((r, g, b, a)) = config.tint_color {
                native_controls::set_native_glass_effect_tint_color(view, r, g, b, a);
            }
            ensure_attached(state, parent, bounds, scale);
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
                if !state.target().is_null() {
                    native_controls::release_native_tracking_view_target(state.target());
                    state.set_target(std::ptr::null_mut());
                }
            } else {
                let view = native_controls::create_native_tracking_view();
                *state = NativeControlState::new(view as *mut c_void, std::ptr::null_mut(), cleanup_tracking_view);
            }
            if let (Some(on_enter), Some(on_exit)) = (config.callbacks.on_enter, config.callbacks.on_exit) {
                let target = native_controls::set_native_tracking_view_callbacks(
                    state.view() as id,
                    on_enter,
                    on_exit,
                );
                state.set_target(target);
            } else {
                let _ = config.callbacks.on_move;
            }
            ensure_attached(state, parent, bounds, scale);
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
            let view = if state.is_initialized() {
                state.view() as id
            } else {
                let view = native_controls::create_native_stack_view(config.orientation);
                *state =
                    NativeControlState::new(view as *mut c_void, std::ptr::null_mut(), cleanup_stack_view);
                view
            };
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
            native_controls::remove_all_native_stack_view_arranged_subviews(view);
            for child in config.children {
                if !child.is_null() {
                    native_controls::add_native_stack_view_arranged_subview(view, child as id);
                }
            }
            ensure_attached(state, parent, bounds, scale);
        }
    }

    fn update_sidebar(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: SidebarViewConfig,
    ) {
        unsafe {
            let view = if state.is_initialized() {
                state.view() as id
            } else {
                let view = native_controls::create_native_sidebar_view(
                    config.sidebar_width,
                    config.min_width,
                    config.max_width,
                );
                *state = NativeControlState::new(view as *mut c_void, std::ptr::null_mut(), cleanup_sidebar);
                view
            };
            native_controls::set_native_sidebar_width(view, config.sidebar_width);
            native_controls::set_native_sidebar_collapsed(view, config.collapsed);
            if let Some((r, g, b, a)) = config.background_color {
                native_controls::set_native_sidebar_background_color(view, r, g, b, a);
            } else {
                native_controls::clear_native_sidebar_background_color(view);
            }
            let _ = native_controls::set_native_sidebar_items(view, std::ptr::null_mut());
            if let Some(title) = config.header_title {
                native_controls::set_native_sidebar_header(view, title);
            }
            let _ = config.embed_in_host;
            let _ = config.expanded_width;
            let _ = config.items;
            let _ = config.selected_index;
            let _ = config.header_button_symbols;
            let _ = config.on_select;
            let _ = config.on_header_button;
            ensure_attached(state, parent, bounds, scale);
        }
    }

    fn is_glass_effect_available(&self) -> bool {
        true
    }

    fn get_text_field_value(&self, state: &NativeControlState) -> String {
        unsafe { native_controls::get_native_text_field_string_value(state.view() as id) }
    }

    fn get_combo_box_value(&self, state: &NativeControlState) -> String {
        unsafe { native_controls::get_native_combo_box_string_value(state.view() as id) }
    }

    fn show_context_menu(
        &self,
        _items: &[NativeMenuItemData],
        _view: *mut c_void,
        _x: f64,
        _y: f64,
        on_result: Box<dyn FnOnce(Option<usize>)>,
    ) {
        on_result(None);
    }

    fn show_alert_modal(&self, _config: AlertConfig) -> i64 {
        ALERT_FIRST_BUTTON_RETURN
    }

    fn show_alert_sheet(
        &self,
        _config: AlertConfig,
        _parent_window: *mut c_void,
        callback: Option<Box<dyn FnOnce(i64)>>,
    ) {
        if let Some(callback) = callback {
            callback(ALERT_FIRST_BUTTON_RETURN);
        }
    }

    fn create_panel(&self, config: PanelConfig) -> NativeControlState {
        unsafe {
            let panel: id = msg_send![class!(UIView), alloc];
            let panel: id = msg_send![panel, init];
            set_view_frame(panel, 0.0, 0.0, config.width, config.height);
            NativeControlState::new(panel as *mut c_void, std::ptr::null_mut(), cleanup_panel)
        }
    }

    fn get_panel_content_view(&self, state: &NativeControlState) -> *mut c_void {
        state.view()
    }

    fn show_panel(&self, state: &NativeControlState) {
        unsafe {
            let _: () = msg_send![state.view() as id, setHidden: false as i8];
        }
    }

    fn show_panel_centered(&self, state: &NativeControlState) {
        self.show_panel(state);
    }

    fn set_panel_origin(&self, state: &NativeControlState, x: f64, y: f64) {
        unsafe {
            let frame: ((f64, f64), (f64, f64)) = msg_send![state.view() as id, frame];
            set_view_frame(state.view() as id, x, y, frame.1.0, frame.1.1);
        }
    }

    fn set_panel_top_left(&self, state: &NativeControlState, x: f64, y: f64) {
        self.set_panel_origin(state, x, y);
    }

    fn set_panel_size(&self, state: &NativeControlState, width: f64, height: f64) {
        unsafe {
            let frame: ((f64, f64), (f64, f64)) = msg_send![state.view() as id, frame];
            set_view_frame(state.view() as id, frame.0.0, frame.0.1, width, height);
        }
    }

    fn set_panel_frame(
        &self,
        state: &NativeControlState,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        _animate: bool,
    ) {
        unsafe {
            set_view_frame(state.view() as id, x, y, width, height);
        }
    }

    fn close_panel(&self, state: &NativeControlState) {
        unsafe {
            let _: () = msg_send![state.view() as id, removeFromSuperview];
        }
    }

    fn hide_panel(&self, state: &NativeControlState) {
        unsafe {
            let _: () = msg_send![state.view() as id, setHidden: true as i8];
        }
    }

    fn is_panel_visible(&self, state: &NativeControlState) -> bool {
        unsafe {
            let hidden: bool = msg_send![state.view() as id, isHidden];
            !hidden
        }
    }

    fn get_toolbar_item_frame(&self, _window: *mut c_void, _item_id: &str) -> Option<Bounds<Pixels>> {
        None
    }

    fn create_popover(&self, config: PopoverConfig) -> NativeControlState {
        unsafe {
            let view: id = msg_send![class!(UIView), alloc];
            let view: id = msg_send![view, init];
            set_view_frame(view, 0.0, 0.0, config.width, config.height);
            let _: () = msg_send![view, setHidden: true as i8];
            NativeControlState::new(view as *mut c_void, std::ptr::null_mut(), cleanup_popover)
        }
    }

    fn get_popover_content_view(&self, state: &NativeControlState) -> *mut c_void {
        state.view()
    }

    fn show_popover_at_toolbar_item(&self, state: &NativeControlState, _toolbar_item: *mut c_void) {
        unsafe {
            let _: () = msg_send![state.view() as id, setHidden: false as i8];
        }
    }

    fn dismiss_popover(&self, state: &NativeControlState) {
        unsafe {
            let _: () = msg_send![state.view() as id, setHidden: true as i8];
        }
    }
}

// Private popover content helpers — used internally for rendering popover
// content items, not part of the public trait surface.
impl IosNativeControls {
    fn add_popover_label(
        &self,
        content_view: *mut c_void,
        text: &str,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        font_size: f64,
        bold: bool,
    ) -> *mut c_void {
        unsafe {
            let label: id = msg_send![class!(UILabel), alloc];
            let label: id = msg_send![label, init];
            let _: () = msg_send![label, setText: native_controls::ns_string(text)];
            let font: id = if bold {
                msg_send![class!(UIFont), boldSystemFontOfSize: font_size]
            } else {
                msg_send![class!(UIFont), systemFontOfSize: font_size]
            };
            let _: () = msg_send![label, setFont: font];
            set_view_frame(label, x, y, width, height);
            let _: () = msg_send![content_view as id, addSubview: label];
            label as *mut c_void
        }
    }
}
