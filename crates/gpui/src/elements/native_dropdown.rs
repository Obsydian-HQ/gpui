use refineable::Refineable as _;
use std::ffi::c_void;
use std::rc::Rc;

use crate::{
    AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, SharedString, Style,
    StyleRefinement, Styled, Window, px,
};

use super::native_element_helpers::schedule_native_callback;

// =============================================================================
// Event type
// =============================================================================

/// Event emitted when a new item is selected in a NativeDropdown.
#[derive(Clone, Debug)]
pub struct DropdownSelectEvent {
    /// The selected item index.
    pub index: usize,
}

// =============================================================================
// Public constructor
// =============================================================================

/// Creates a native dropdown (NSPopUpButton on macOS).
pub fn native_dropdown(id: impl Into<ElementId>, items: &[impl AsRef<str>]) -> NativeDropdown {
    NativeDropdown {
        id: id.into(),
        items: items
            .iter()
            .map(|i| SharedString::from(i.as_ref().to_string()))
            .collect(),
        selected_index: 0,
        on_select: None,
        disabled: false,
        style: StyleRefinement::default(),
    }
}

// =============================================================================
// Element struct
// =============================================================================

/// A native dropdown element positioned by GPUI's Taffy layout.
pub struct NativeDropdown {
    id: ElementId,
    items: Vec<SharedString>,
    selected_index: usize,
    on_select: Option<Box<dyn Fn(&DropdownSelectEvent, &mut Window, &mut App) + 'static>>,
    disabled: bool,
    style: StyleRefinement,
}

impl NativeDropdown {
    /// Sets the selected item index.
    pub fn selected_index(mut self, index: usize) -> Self {
        self.selected_index = index;
        self
    }

    /// Registers a callback invoked when selection changes.
    pub fn on_select(
        mut self,
        listener: impl Fn(&DropdownSelectEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Box::new(listener));
        self
    }

    /// Sets whether the dropdown is disabled.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

// =============================================================================
// Persisted element state
// =============================================================================

struct NativeDropdownElementState {
    control_ptr: *mut c_void,
    target_ptr: *mut c_void,
    current_items: Vec<SharedString>,
    current_selected: usize,
    attached: bool,
}

impl Drop for NativeDropdownElementState {
    fn drop(&mut self) {
        if self.attached {
            #[cfg(target_os = "macos")]
            unsafe {
                use crate::platform::native_controls;
                super::native_element_helpers::cleanup_native_control(
                    self.control_ptr,
                    self.target_ptr,
                    native_controls::release_native_popup_target,
                    native_controls::release_native_popup_button,
                );
            }
        }
    }
}

unsafe impl Send for NativeDropdownElementState {}

// =============================================================================
// Helpers
// =============================================================================

fn clamp_selected_index(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { index.min(len - 1) }
}

// =============================================================================
// Element trait impl
// =============================================================================

impl IntoElement for NativeDropdown {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeDropdown {
    type RequestLayoutState = ();
    type PrepaintState = Bounds<Pixels>;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.refine(&self.style);

        if matches!(style.size.width, Length::Auto) {
            style.size.width =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(180.0))));
        }
        if matches!(style.size.height, Length::Auto) {
            style.size.height =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(24.0))));
        }

        let layout_id = window.request_layout(style, [], cx);
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Bounds<Pixels> {
        bounds
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        _cx: &mut App,
    ) {
        #[cfg(target_os = "macos")]
        {
            use crate::platform::native_controls;

            let native_view = window.raw_native_view_ptr();
            if native_view.is_null() {
                return;
            }

            let on_select = self.on_select.take();
            let items = self.items.clone();
            let selected_index = self.selected_index;
            let disabled = self.disabled;

            let next_frame_callbacks = window.next_frame_callbacks.clone();
            let invalidator = window.invalidator.clone();

            window.with_optional_element_state::<NativeDropdownElementState, _>(
                id,
                |prev_state, window| {
                    let clamped_selected = clamp_selected_index(selected_index, items.len());

                    let state = if let Some(Some(mut state)) = prev_state {
                        unsafe {
                            native_controls::set_native_view_frame(
                                state.control_ptr as cocoa::base::id,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );
                        }

                        let items_changed = state.current_items != items;
                        if items_changed {
                            let item_strs: Vec<&str> = items.iter().map(|s| s.as_ref()).collect();
                            unsafe {
                                native_controls::set_native_popup_items(
                                    state.control_ptr as cocoa::base::id,
                                    &item_strs,
                                );
                            }
                            state.current_items = items.clone();
                        }

                        if items_changed || state.current_selected != clamped_selected {
                            if !items.is_empty() {
                                unsafe {
                                    native_controls::set_native_popup_selected(
                                        state.control_ptr as cocoa::base::id,
                                        clamped_selected,
                                    );
                                }
                            }
                            state.current_selected = clamped_selected;
                        }

                        unsafe {
                            native_controls::set_native_control_enabled(
                                state.control_ptr as cocoa::base::id,
                                !disabled,
                            );
                        }

                        if let Some(on_select) = on_select {
                            unsafe {
                                native_controls::release_native_popup_target(state.target_ptr);
                            }
                            let nfc = next_frame_callbacks.clone();
                            let inv = invalidator.clone();
                            let on_select = Rc::new(on_select);
                            let callback = schedule_native_callback(
                                on_select,
                                |index| DropdownSelectEvent { index },
                                nfc,
                                inv,
                            );
                            unsafe {
                                state.target_ptr = native_controls::set_native_popup_action(
                                    state.control_ptr as cocoa::base::id,
                                    callback,
                                );
                            }
                        }

                        state
                    } else {
                        let (control_ptr, target_ptr) = unsafe {
                            let item_strs: Vec<&str> = items.iter().map(|s| s.as_ref()).collect();
                            let control = native_controls::create_native_popup_button(
                                &item_strs,
                                clamped_selected,
                            );
                            native_controls::attach_native_view_to_parent(
                                control,
                                native_view as cocoa::base::id,
                            );
                            native_controls::set_native_view_frame(
                                control,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );
                            native_controls::set_native_control_enabled(control, !disabled);

                            let target = if let Some(on_select) = on_select {
                                let nfc = next_frame_callbacks.clone();
                                let inv = invalidator.clone();
                                let on_select = Rc::new(on_select);
                                let callback = schedule_native_callback(
                                    on_select,
                                    |index| DropdownSelectEvent { index },
                                    nfc,
                                    inv,
                                );
                                native_controls::set_native_popup_action(control, callback)
                            } else {
                                std::ptr::null_mut()
                            };

                            (control as *mut c_void, target)
                        };

                        NativeDropdownElementState {
                            control_ptr,
                            target_ptr,
                            current_items: items,
                            current_selected: clamped_selected,
                            attached: true,
                        }
                    };

                    ((), Some(state))
                },
            );
        }
    }
}

impl Styled for NativeDropdown {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
