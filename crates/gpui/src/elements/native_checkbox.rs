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

/// Event emitted when the checked state changes in a NativeCheckbox.
#[derive(Clone, Debug)]
pub struct CheckboxChangeEvent {
    /// The new checked state.
    pub checked: bool,
}

// =============================================================================
// Public constructor
// =============================================================================

/// Creates a native checkbox (NSButton in checkbox mode on macOS).
pub fn native_checkbox(id: impl Into<ElementId>, label: impl Into<SharedString>) -> NativeCheckbox {
    NativeCheckbox {
        id: id.into(),
        label: label.into(),
        checked: false,
        on_change: None,
        disabled: false,
        style: StyleRefinement::default(),
    }
}

// =============================================================================
// Element struct
// =============================================================================

/// A native checkbox element positioned by GPUI's Taffy layout.
pub struct NativeCheckbox {
    id: ElementId,
    label: SharedString,
    checked: bool,
    on_change: Option<Box<dyn Fn(&CheckboxChangeEvent, &mut Window, &mut App) + 'static>>,
    disabled: bool,
    style: StyleRefinement,
}

impl NativeCheckbox {
    /// Sets whether the checkbox is checked.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Registers a callback invoked when the checked state changes.
    pub fn on_change(
        mut self,
        listener: impl Fn(&CheckboxChangeEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Box::new(listener));
        self
    }

    /// Sets whether this checkbox is disabled.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

// =============================================================================
// Persisted element state
// =============================================================================

struct NativeCheckboxElementState {
    native_checkbox_ptr: *mut c_void,
    native_target_ptr: *mut c_void,
    current_label: SharedString,
    current_checked: bool,
    attached: bool,
}

impl Drop for NativeCheckboxElementState {
    fn drop(&mut self) {
        if self.attached {
            #[cfg(target_os = "macos")]
            unsafe {
                use crate::platform::native_controls;
                super::native_element_helpers::cleanup_native_control(
                    self.native_checkbox_ptr,
                    self.native_target_ptr,
                    native_controls::release_native_checkbox_target,
                    native_controls::release_native_checkbox,
                );
            }
        }
    }
}

unsafe impl Send for NativeCheckboxElementState {}

// =============================================================================
// Element trait impl
// =============================================================================

impl IntoElement for NativeCheckbox {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeCheckbox {
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
            let width = (self.label.len() as f32 * 8.0 + 40.0).max(90.0);
            style.size.width =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(width))));
        }
        if matches!(style.size.height, Length::Auto) {
            style.size.height =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(18.0))));
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

            let on_change = self.on_change.take();
            let label = self.label.clone();
            let checked = self.checked;
            let disabled = self.disabled;

            let next_frame_callbacks = window.next_frame_callbacks.clone();
            let invalidator = window.invalidator.clone();

            window.with_optional_element_state::<NativeCheckboxElementState, _>(
                id,
                |prev_state, window| {
                    let state = if let Some(Some(mut state)) = prev_state {
                        unsafe {
                            native_controls::set_native_view_frame(
                                state.native_checkbox_ptr as cocoa::base::id,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );
                            if state.current_label != label {
                                native_controls::set_native_checkbox_title(
                                    state.native_checkbox_ptr as cocoa::base::id,
                                    &label,
                                );
                                state.current_label = label.clone();
                            }
                            if state.current_checked != checked {
                                native_controls::set_native_checkbox_state(
                                    state.native_checkbox_ptr as cocoa::base::id,
                                    checked,
                                );
                                state.current_checked = checked;
                            }
                            native_controls::set_native_control_enabled(
                                state.native_checkbox_ptr as cocoa::base::id,
                                !disabled,
                            );
                        }

                        if let Some(on_change) = on_change {
                            unsafe {
                                native_controls::release_native_checkbox_target(
                                    state.native_target_ptr,
                                );
                            }
                            let nfc = next_frame_callbacks.clone();
                            let inv = invalidator.clone();
                            let on_change = Rc::new(on_change);
                            let callback = schedule_native_callback(
                                on_change,
                                |checked| CheckboxChangeEvent { checked },
                                nfc,
                                inv,
                            );
                            unsafe {
                                state.native_target_ptr =
                                    native_controls::set_native_checkbox_action(
                                        state.native_checkbox_ptr as cocoa::base::id,
                                        callback,
                                    );
                            }
                        }

                        state
                    } else {
                        let (checkbox_ptr, target_ptr) = unsafe {
                            let checkbox = native_controls::create_native_checkbox(&label);
                            native_controls::set_native_checkbox_state(checkbox, checked);
                            native_controls::set_native_control_enabled(checkbox, !disabled);
                            native_controls::attach_native_view_to_parent(
                                checkbox,
                                native_view as cocoa::base::id,
                            );
                            native_controls::set_native_view_frame(
                                checkbox,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );

                            let target = if let Some(on_change) = on_change {
                                let nfc = next_frame_callbacks.clone();
                                let inv = invalidator.clone();
                                let on_change = Rc::new(on_change);
                                let callback = schedule_native_callback(
                                    on_change,
                                    |checked| CheckboxChangeEvent { checked },
                                    nfc,
                                    inv,
                                );
                                native_controls::set_native_checkbox_action(checkbox, callback)
                            } else {
                                std::ptr::null_mut()
                            };

                            (checkbox as *mut c_void, target)
                        };

                        NativeCheckboxElementState {
                            native_checkbox_ptr: checkbox_ptr,
                            native_target_ptr: target_ptr,
                            current_label: label,
                            current_checked: checked,
                            attached: true,
                        }
                    };

                    ((), Some(state))
                },
            );
        }
    }
}

impl Styled for NativeCheckbox {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
