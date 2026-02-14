use refineable::Refineable as _;
use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;

use crate::{
    AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, SharedString, Style,
    StyleRefinement, Styled, Window, px,
};

use super::native_element_helpers::{
    FrameCallback, schedule_native_callback, schedule_native_focus_callback,
};

/// Event emitted when search text changes.
#[derive(Clone, Debug)]
pub struct SearchChangeEvent {
    /// The current search text.
    pub text: String,
}

/// Event emitted when search is submitted.
#[derive(Clone, Debug)]
pub struct SearchSubmitEvent {
    /// The submitted search text.
    pub text: String,
}

/// Creates a native search field (NSSearchField on macOS).
pub fn native_search_field(id: impl Into<ElementId>) -> NativeSearchField {
    NativeSearchField {
        id: id.into(),
        value: SharedString::default(),
        placeholder: SharedString::default(),
        sends_search_string_immediately: true,
        sends_whole_search_string: false,
        disabled: false,
        on_change: None,
        on_submit: None,
        on_focus: None,
        on_blur: None,
        style: StyleRefinement::default(),
    }
}

/// A native search field element positioned by GPUI's Taffy layout.
pub struct NativeSearchField {
    id: ElementId,
    value: SharedString,
    placeholder: SharedString,
    sends_search_string_immediately: bool,
    sends_whole_search_string: bool,
    disabled: bool,
    on_change: Option<Box<dyn Fn(&SearchChangeEvent, &mut Window, &mut App) + 'static>>,
    on_submit: Option<Box<dyn Fn(&SearchSubmitEvent, &mut Window, &mut App) + 'static>>,
    on_focus: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
    on_blur: Option<Box<dyn Fn(&SearchSubmitEvent, &mut Window, &mut App) + 'static>>,
    style: StyleRefinement,
}

impl NativeSearchField {
    /// Sets current search text value.
    pub fn value(mut self, value: impl Into<SharedString>) -> Self {
        self.value = value.into();
        self
    }

    /// Sets placeholder text.
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Controls whether every typed character emits a search update.
    pub fn sends_search_string_immediately(mut self, value: bool) -> Self {
        self.sends_search_string_immediately = value;
        self
    }

    /// Controls whether only complete search strings are sent.
    pub fn sends_whole_search_string(mut self, value: bool) -> Self {
        self.sends_whole_search_string = value;
        self
    }

    /// Sets whether this search field is disabled.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Registers a callback invoked when text changes.
    pub fn on_change(
        mut self,
        listener: impl Fn(&SearchChangeEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Box::new(listener));
        self
    }

    /// Registers a callback invoked when Enter is pressed.
    pub fn on_submit(
        mut self,
        listener: impl Fn(&SearchSubmitEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_submit = Some(Box::new(listener));
        self
    }

    /// Registers a callback invoked when the field receives focus.
    pub fn on_focus(mut self, listener: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_focus = Some(Box::new(listener));
        self
    }

    /// Registers a callback invoked when editing ends.
    pub fn on_blur(
        mut self,
        listener: impl Fn(&SearchSubmitEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_blur = Some(Box::new(listener));
        self
    }
}

struct NativeSearchFieldElementState {
    search_field_ptr: *mut c_void,
    delegate_ptr: *mut c_void,
    current_placeholder: SharedString,
    current_value: SharedString,
    current_sends_immediately: bool,
    current_sends_whole: bool,
    attached: bool,
}

impl Drop for NativeSearchFieldElementState {
    fn drop(&mut self) {
        if self.attached {
            #[cfg(target_os = "macos")]
            unsafe {
                use crate::platform::native_controls;
                super::native_element_helpers::cleanup_native_control(
                    self.search_field_ptr,
                    self.delegate_ptr,
                    native_controls::release_native_text_field_delegate,
                    native_controls::release_native_search_field,
                );
            }
        }
    }
}

unsafe impl Send for NativeSearchFieldElementState {}

impl IntoElement for NativeSearchField {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeSearchField {
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
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(220.0))));
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

            let on_change = self.on_change.take();
            let on_submit = self.on_submit.take();
            let on_focus = self.on_focus.take();
            let on_blur = self.on_blur.take();
            let value = self.value.clone();
            let placeholder = self.placeholder.clone();
            let sends_immediately = self.sends_search_string_immediately;
            let sends_whole = self.sends_whole_search_string;
            let disabled = self.disabled;

            let next_frame_callbacks = window.next_frame_callbacks.clone();
            let invalidator = window.invalidator.clone();

            window.with_optional_element_state::<NativeSearchFieldElementState, _>(
                id,
                |prev_state, window| {
                    let state = if let Some(Some(mut state)) = prev_state {
                        unsafe {
                            native_controls::set_native_view_frame(
                                state.search_field_ptr as cocoa::base::id,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );
                            if state.current_placeholder != placeholder {
                                native_controls::set_native_search_field_placeholder(
                                    state.search_field_ptr as cocoa::base::id,
                                    &placeholder,
                                );
                                state.current_placeholder = placeholder.clone();
                            }
                            if state.current_value != value {
                                native_controls::set_native_search_field_string_value(
                                    state.search_field_ptr as cocoa::base::id,
                                    &value,
                                );
                                state.current_value = value.clone();
                            }
                            if state.current_sends_immediately != sends_immediately {
                                native_controls::set_native_search_field_sends_immediately(
                                    state.search_field_ptr as cocoa::base::id,
                                    sends_immediately,
                                );
                                state.current_sends_immediately = sends_immediately;
                            }
                            if state.current_sends_whole != sends_whole {
                                native_controls::set_native_search_field_sends_whole_string(
                                    state.search_field_ptr as cocoa::base::id,
                                    sends_whole,
                                );
                                state.current_sends_whole = sends_whole;
                            }
                            native_controls::set_native_control_enabled(
                                state.search_field_ptr as cocoa::base::id,
                                !disabled,
                            );
                        }

                        unsafe {
                            native_controls::release_native_text_field_delegate(state.delegate_ptr);
                        }
                        let callbacks = build_search_field_callbacks(
                            on_change,
                            on_submit,
                            on_focus,
                            on_blur,
                            next_frame_callbacks,
                            invalidator,
                        );
                        unsafe {
                            state.delegate_ptr = native_controls::set_native_text_field_delegate(
                                state.search_field_ptr as cocoa::base::id,
                                callbacks,
                            );
                        }

                        state
                    } else {
                        let (search_field_ptr, delegate_ptr) = unsafe {
                            let field = native_controls::create_native_search_field(&placeholder);
                            if !value.is_empty() {
                                native_controls::set_native_search_field_string_value(
                                    field, &value,
                                );
                            }
                            native_controls::set_native_search_field_sends_immediately(
                                field,
                                sends_immediately,
                            );
                            native_controls::set_native_search_field_sends_whole_string(
                                field,
                                sends_whole,
                            );
                            native_controls::set_native_control_enabled(field, !disabled);

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

                            let callbacks = build_search_field_callbacks(
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

                        NativeSearchFieldElementState {
                            search_field_ptr,
                            delegate_ptr,
                            current_placeholder: placeholder,
                            current_value: value,
                            current_sends_immediately: sends_immediately,
                            current_sends_whole: sends_whole,
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
fn build_search_field_callbacks(
    on_change: Option<Box<dyn Fn(&SearchChangeEvent, &mut Window, &mut App) + 'static>>,
    on_submit: Option<Box<dyn Fn(&SearchSubmitEvent, &mut Window, &mut App) + 'static>>,
    on_focus: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
    on_blur: Option<Box<dyn Fn(&SearchSubmitEvent, &mut Window, &mut App) + 'static>>,
    next_frame_callbacks: Rc<RefCell<Vec<FrameCallback>>>,
    invalidator: crate::WindowInvalidator,
) -> crate::platform::native_controls::TextFieldCallbacks {
    use crate::platform::native_controls::TextFieldCallbacks;

    let change_cb = on_change.map(|h| {
        schedule_native_callback(
            Rc::new(h),
            |text| SearchChangeEvent { text },
            next_frame_callbacks.clone(),
            invalidator.clone(),
        )
    });

    let submit_cb = on_submit.map(|h| {
        schedule_native_callback(
            Rc::new(h),
            |text| SearchSubmitEvent { text },
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
            |text| SearchSubmitEvent { text },
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

impl Styled for NativeSearchField {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
