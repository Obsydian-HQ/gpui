use refineable::Refineable as _;
use std::ffi::c_void;
use std::rc::Rc;

use crate::{
    AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, Style, StyleRefinement, Styled,
    Window, px,
};

use super::native_element_helpers::schedule_native_callback;

/// Event emitted when a native switch changes state.
#[derive(Clone, Debug)]
pub struct SwitchChangeEvent {
    /// The new checked state.
    pub checked: bool,
}

/// Creates a native switch (NSSwitch on macOS).
pub fn native_switch(id: impl Into<ElementId>) -> NativeSwitch {
    NativeSwitch {
        id: id.into(),
        checked: false,
        on_change: None,
        disabled: false,
        style: StyleRefinement::default(),
    }
}

/// A native switch element positioned by GPUI's Taffy layout.
pub struct NativeSwitch {
    id: ElementId,
    checked: bool,
    on_change: Option<Box<dyn Fn(&SwitchChangeEvent, &mut Window, &mut App) + 'static>>,
    disabled: bool,
    style: StyleRefinement,
}

impl NativeSwitch {
    /// Sets whether the switch is checked.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Registers a callback invoked when the checked state changes.
    pub fn on_change(
        mut self,
        listener: impl Fn(&SwitchChangeEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Box::new(listener));
        self
    }

    /// Sets whether this switch is disabled.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

struct NativeSwitchElementState {
    switch_ptr: *mut c_void,
    target_ptr: *mut c_void,
    current_checked: bool,
    attached: bool,
}

impl Drop for NativeSwitchElementState {
    fn drop(&mut self) {
        if self.attached {
            #[cfg(target_os = "macos")]
            unsafe {
                use crate::platform::native_controls;
                super::native_element_helpers::cleanup_native_control(
                    self.switch_ptr,
                    self.target_ptr,
                    native_controls::release_native_switch_target,
                    native_controls::release_native_switch,
                );
            }
        }
    }
}

unsafe impl Send for NativeSwitchElementState {}

impl IntoElement for NativeSwitch {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeSwitch {
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
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(38.0))));
        }
        if matches!(style.size.height, Length::Auto) {
            style.size.height =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(22.0))));
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
            let checked = self.checked;
            let disabled = self.disabled;

            let next_frame_callbacks = window.next_frame_callbacks.clone();
            let invalidator = window.invalidator.clone();

            window.with_optional_element_state::<NativeSwitchElementState, _>(
                id,
                |prev_state, window| {
                    let state = if let Some(Some(mut state)) = prev_state {
                        unsafe {
                            native_controls::set_native_view_frame(
                                state.switch_ptr as cocoa::base::id,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );
                            if state.current_checked != checked {
                                native_controls::set_native_switch_state(
                                    state.switch_ptr as cocoa::base::id,
                                    checked,
                                );
                                state.current_checked = checked;
                            }
                            native_controls::set_native_control_enabled(
                                state.switch_ptr as cocoa::base::id,
                                !disabled,
                            );
                        }

                        if let Some(on_change) = on_change {
                            unsafe {
                                native_controls::release_native_switch_target(state.target_ptr);
                            }
                            let nfc = next_frame_callbacks.clone();
                            let inv = invalidator.clone();
                            let on_change = Rc::new(on_change);
                            let callback = schedule_native_callback(
                                on_change,
                                |checked| SwitchChangeEvent { checked },
                                nfc,
                                inv,
                            );
                            unsafe {
                                state.target_ptr = native_controls::set_native_switch_action(
                                    state.switch_ptr as cocoa::base::id,
                                    callback,
                                );
                            }
                        }

                        state
                    } else {
                        let (switch_ptr, target_ptr) = unsafe {
                            let switch = native_controls::create_native_switch();
                            native_controls::set_native_switch_state(switch, checked);
                            native_controls::set_native_control_enabled(switch, !disabled);
                            native_controls::attach_native_view_to_parent(
                                switch,
                                native_view as cocoa::base::id,
                            );
                            native_controls::set_native_view_frame(
                                switch,
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
                                    |checked| SwitchChangeEvent { checked },
                                    nfc,
                                    inv,
                                );
                                native_controls::set_native_switch_action(switch, callback)
                            } else {
                                std::ptr::null_mut()
                            };

                            (switch as *mut c_void, target)
                        };

                        NativeSwitchElementState {
                            switch_ptr,
                            target_ptr,
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

impl Styled for NativeSwitch {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
