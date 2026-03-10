use refineable::Refineable as _;
use std::cell::RefCell;
use std::rc::Rc;

use crate::platform::native_controls::{NativeControlState, TextFieldCallbacks, TextFieldConfig};
use crate::{
    px, AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, SharedString, Style,
    StyleRefinement, Styled, Window,
};

use super::native_element_helpers::{
    FrameCallback, schedule_native_callback, schedule_native_focus_callback,
};

#[derive(Clone, Debug)]
pub struct TextChangeEvent {
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct TextSubmitEvent {
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeTextFieldStyle {
    #[default]
    Square,
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
    pub fn value(mut self, value: impl Into<SharedString>) -> Self {
        self.value = value.into();
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn field_style(mut self, style: NativeTextFieldStyle) -> Self {
        self.field_style = style;
        self
    }

    pub fn on_change(
        mut self,
        listener: impl Fn(&TextChangeEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Box::new(listener));
        self
    }

    pub fn on_submit(
        mut self,
        listener: impl Fn(&TextSubmitEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_submit = Some(Box::new(listener));
        self
    }

    pub fn on_focus(mut self, listener: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_focus = Some(Box::new(listener));
        self
    }

    pub fn on_blur(
        mut self,
        listener: impl Fn(&TextSubmitEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_blur = Some(Box::new(listener));
        self
    }
}

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
            style.size.width =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(200.0))));
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
        let parent = window.raw_native_view_ptr();
        if parent.is_null() {
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

        let nfc = window.next_frame_callbacks.clone();
        let inv = window.invalidator.clone();

        window.with_optional_element_state::<NativeControlState, _>(id, |prev_state, window| {
            let mut state = prev_state.flatten().unwrap_or_default();

            let callbacks =
                build_text_field_callbacks(on_change, on_submit, on_focus, on_blur, nfc, inv);

            let scale = window.scale_factor();
            let nc = window.native_controls();
            nc.update_text_field(
                &mut state,
                parent,
                bounds,
                scale,
                TextFieldConfig {
                    placeholder: &placeholder,
                    value: &value,
                    secure,
                    font_size: None,
                    alignment: None,
                    bezel_style: Some(field_style.to_ns_style()),
                    enabled: !disabled,
                    callbacks,
                },
            );

            ((), Some(state))
        });
    }
}

fn build_text_field_callbacks(
    on_change: Option<Box<dyn Fn(&TextChangeEvent, &mut Window, &mut App) + 'static>>,
    on_submit: Option<Box<dyn Fn(&TextSubmitEvent, &mut Window, &mut App) + 'static>>,
    on_focus: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
    on_blur: Option<Box<dyn Fn(&TextSubmitEvent, &mut Window, &mut App) + 'static>>,
    next_frame_callbacks: Rc<RefCell<Vec<FrameCallback>>>,
    invalidator: crate::WindowInvalidator,
) -> TextFieldCallbacks {
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
        on_move_up: None,
        on_move_down: None,
        on_cancel: None,
    }
}

impl Styled for NativeTextField {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
