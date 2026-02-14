use refineable::Refineable as _;
use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;

use crate::{
    AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, SharedString, Style, StyleRefinement,
    Styled, Window, px,
};

use super::native_element_helpers::{
    FrameCallback, schedule_native_callback, schedule_native_focus_callback,
};

// =============================================================================
// Event types
// =============================================================================

/// Event emitted when the text changes in a NativeTextField.
#[derive(Clone, Debug)]
pub struct TextChangeEvent {
    /// The current text value.
    pub text: String,
}

/// Event emitted when the user presses Enter in a NativeTextField.
#[derive(Clone, Debug)]
pub struct TextSubmitEvent {
    /// The text value at the time of submission.
    pub text: String,
}

// =============================================================================
// Style enum
// =============================================================================

/// Bezel style for the native text field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeTextFieldStyle {
    /// Standard square bezel (NSTextFieldSquareBezel = 0).
    #[default]
    Square,
    /// Rounded bezel, search-bar style (NSTextFieldRoundedBezel = 1).
    Rounded,
}

impl NativeTextFieldStyle {
    fn to_ns_style(self) -> i64 {
        match self {
            NativeTextFieldStyle::Square => 0,
            NativeTextFieldStyle::Rounded => 1,
        }
    }
}

// =============================================================================
// Public constructor
// =============================================================================

/// Creates a native platform text field element (NSTextField on macOS).
///
/// The text field participates in GPUI's Taffy layout system and renders as a real
/// platform text field, not a custom-drawn element.
pub fn native_text_field(id: impl Into<ElementId>) -> NativeTextField {
    NativeTextField {
        id: id.into(),
        value: SharedString::default(),
        placeholder: SharedString::default(),
        secure: false,
        disabled: false,
        field_style: NativeTextFieldStyle::default(),
        on_change: None,
        on_submit: None,
        on_focus: None,
        on_blur: None,
        style: StyleRefinement::default(),
    }
}

// =============================================================================
// Element struct
// =============================================================================

/// A native platform text field element that creates a real OS text field
/// (NSTextField on macOS) as a subview of the window's native view,
/// positioned by GPUI's Taffy layout engine.
pub struct NativeTextField {
    id: ElementId,
    value: SharedString,
    placeholder: SharedString,
    secure: bool,
    disabled: bool,
    field_style: NativeTextFieldStyle,
    on_change: Option<Box<dyn Fn(&TextChangeEvent, &mut Window, &mut App) + 'static>>,
    on_submit: Option<Box<dyn Fn(&TextSubmitEvent, &mut Window, &mut App) + 'static>>,
    on_focus: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
    on_blur: Option<Box<dyn Fn(&TextSubmitEvent, &mut Window, &mut App) + 'static>>,
    style: StyleRefinement,
}

impl NativeTextField {
    /// Set the current text value of the text field.
    pub fn value(mut self, value: impl Into<SharedString>) -> Self {
        self.value = value.into();
        self
    }

    /// Set the placeholder text.
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Set whether this is a secure (password) text field.
    /// Uses NSSecureTextField which shows bullet characters.
    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    /// Set whether the text field is disabled (not editable, grayed out).
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Set the visual bezel style of the text field.
    pub fn field_style(mut self, style: NativeTextFieldStyle) -> Self {
        self.field_style = style;
        self
    }

    /// Register a callback invoked when the text changes.
    pub fn on_change(
        mut self,
        listener: impl Fn(&TextChangeEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Box::new(listener));
        self
    }

    /// Register a callback invoked when the user presses Enter.
    pub fn on_submit(
        mut self,
        listener: impl Fn(&TextSubmitEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_submit = Some(Box::new(listener));
        self
    }

    /// Register a callback invoked when editing begins (first keystroke).
    pub fn on_focus(
        mut self,
        listener: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_focus = Some(Box::new(listener));
        self
    }

    /// Register a callback invoked when editing ends (blur).
    pub fn on_blur(
        mut self,
        listener: impl Fn(&TextSubmitEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_blur = Some(Box::new(listener));
        self
    }
}

// =============================================================================
// Persisted element state
// =============================================================================

struct NativeTextFieldElementState {
    native_field_ptr: *mut c_void,
    native_delegate_ptr: *mut c_void,
    current_placeholder: SharedString,
    current_value: SharedString,
    current_secure: bool,
    current_style: NativeTextFieldStyle,
    attached: bool,
}

impl Drop for NativeTextFieldElementState {
    fn drop(&mut self) {
        if self.attached {
            #[cfg(target_os = "macos")]
            unsafe {
                use crate::platform::native_controls;
                super::native_element_helpers::cleanup_native_control(
                    self.native_field_ptr,
                    self.native_delegate_ptr,
                    native_controls::release_native_text_field_delegate,
                    native_controls::release_native_text_field,
                );
            }
        }
    }
}

unsafe impl Send for NativeTextFieldElementState {}

// =============================================================================
// Element trait impl
// =============================================================================

impl IntoElement for NativeTextField {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeTextField {
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
            style.size.width = Length::Definite(DefiniteLength::Absolute(
                AbsoluteLength::Pixels(px(200.0)),
            ));
        }
        if matches!(style.size.height, Length::Auto) {
            style.size.height = Length::Definite(DefiniteLength::Absolute(
                AbsoluteLength::Pixels(px(22.0)),
            ));
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
            let on_submit = self.on_submit.take();
            let on_focus = self.on_focus.take();
            let on_blur = self.on_blur.take();
            let value = self.value.clone();
            let placeholder = self.placeholder.clone();
            let secure = self.secure;
            let disabled = self.disabled;
            let field_style = self.field_style;

            let next_frame_callbacks = window.next_frame_callbacks.clone();
            let invalidator = window.invalidator.clone();

            window.with_optional_element_state::<NativeTextFieldElementState, _>(
                id,
                |prev_state, window| {
                    // If secure mode or bezel style changed, destroy old control to recreate.
                    // NSSecureTextField is a different class, and bezel style can't be
                    // reliably changed at runtime.
                    let prev_state = match prev_state {
                        Some(Some(mut state))
                            if state.current_secure != secure
                                || state.current_style != field_style =>
                        {
                            unsafe {
                                super::native_element_helpers::cleanup_native_control(
                                    state.native_field_ptr,
                                    state.native_delegate_ptr,
                                    native_controls::release_native_text_field_delegate,
                                    native_controls::release_native_text_field,
                                );
                            }
                            state.attached = false; // Prevent Drop from double-freeing
                            Some(None) // Fall through to creation path
                        }
                        other => other,
                    };

                    let state = if let Some(Some(mut state)) = prev_state {
                        // Normal update path
                        unsafe {
                            native_controls::set_native_view_frame(
                                state.native_field_ptr as cocoa::base::id,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );
                            if state.current_placeholder != placeholder {
                                native_controls::set_native_text_field_placeholder(
                                    state.native_field_ptr as cocoa::base::id,
                                    &placeholder,
                                );
                                state.current_placeholder = placeholder;
                            }
                            if state.current_value != value {
                                native_controls::set_native_text_field_string_value(
                                    state.native_field_ptr as cocoa::base::id,
                                    &value,
                                );
                                state.current_value = value;
                            }
                            native_controls::set_native_control_enabled(
                                state.native_field_ptr as cocoa::base::id,
                                !disabled,
                            );
                        }

                        // Reconnect delegate callbacks
                        unsafe {
                            native_controls::release_native_text_field_delegate(
                                state.native_delegate_ptr,
                            );
                        }
                        let callbacks = build_text_field_callbacks(
                            on_change,
                            on_submit,
                            on_focus,
                            on_blur,
                            next_frame_callbacks,
                            invalidator,
                        );
                        unsafe {
                            state.native_delegate_ptr =
                                native_controls::set_native_text_field_delegate(
                                    state.native_field_ptr as cocoa::base::id,
                                    callbacks,
                                );
                        }

                        state
                    } else {
                        // Creation path: new control or secure/style changed
                        let (field_ptr, delegate_ptr) = unsafe {
                            let field = if secure {
                                native_controls::create_native_secure_text_field(&placeholder)
                            } else {
                                native_controls::create_native_text_field(&placeholder)
                            };

                            // Set bezel style
                            native_controls::set_native_text_field_bezel_style(
                                field,
                                field_style.to_ns_style(),
                            );

                            // Set initial value
                            if !value.is_empty() {
                                native_controls::set_native_text_field_string_value(
                                    field, &value,
                                );
                            }

                            // Disabled state
                            native_controls::set_native_control_enabled(field, !disabled);

                            // Attach to parent
                            native_controls::attach_native_view_to_parent(
                                field,
                                native_view as cocoa::base::id,
                            );
                            native_controls::set_native_view_frame(
                                field,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );

                            // Set delegate
                            let callbacks = build_text_field_callbacks(
                                on_change,
                                on_submit,
                                on_focus,
                                on_blur,
                                next_frame_callbacks,
                                invalidator,
                            );
                            let delegate =
                                native_controls::set_native_text_field_delegate(field, callbacks);

                            (field as *mut c_void, delegate)
                        };

                        NativeTextFieldElementState {
                            native_field_ptr: field_ptr,
                            native_delegate_ptr: delegate_ptr,
                            current_placeholder: placeholder,
                            current_value: value,
                            current_secure: secure,
                            current_style: field_style,
                            attached: true,
                        }
                    };

                    ((), Some(state))
                },
            );
        }
    }
}

#[cfg(target_os = "macos")]
fn build_text_field_callbacks(
    on_change: Option<Box<dyn Fn(&TextChangeEvent, &mut Window, &mut App) + 'static>>,
    on_submit: Option<Box<dyn Fn(&TextSubmitEvent, &mut Window, &mut App) + 'static>>,
    on_focus: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
    on_blur: Option<Box<dyn Fn(&TextSubmitEvent, &mut Window, &mut App) + 'static>>,
    next_frame_callbacks: Rc<RefCell<Vec<FrameCallback>>>,
    invalidator: crate::WindowInvalidator,
) -> crate::platform::native_controls::TextFieldCallbacks {
    use crate::platform::native_controls::TextFieldCallbacks;

    let change_cb = on_change.map(|h| {
        schedule_native_callback(
            Rc::new(h),
            |text| TextChangeEvent { text },
            next_frame_callbacks.clone(),
            invalidator.clone(),
        )
    });

    let submit_cb = on_submit.map(|h| {
        schedule_native_callback(
            Rc::new(h),
            |text| TextSubmitEvent { text },
            next_frame_callbacks.clone(),
            invalidator.clone(),
        )
    });

    let begin_cb = on_focus.map(|h| {
        schedule_native_focus_callback(
            Rc::new(h),
            next_frame_callbacks.clone(),
            invalidator.clone(),
        )
    });

    let end_cb = on_blur.map(|h| {
        schedule_native_callback(
            Rc::new(h),
            |text| TextSubmitEvent { text },
            next_frame_callbacks.clone(),
            invalidator.clone(),
        )
    });

    TextFieldCallbacks {
        on_change: change_cb,
        on_begin_editing: begin_cb,
        on_end_editing: end_cb,
        on_submit: submit_cb,
    }
}

impl Styled for NativeTextField {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
