use refineable::Refineable as _;
use std::cell::RefCell;
use std::rc::Rc;

use crate::platform::native_controls::{NativeControlState, SearchFieldConfig, TextFieldCallbacks};
use crate::{
    px, AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, SharedString, Style,
    StyleRefinement, Styled, Window,
};

use super::native_element_helpers::{
    FrameCallback, schedule_native_callback, schedule_native_focus_callback,
};

#[derive(Clone, Debug)]
pub struct SearchChangeEvent {
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct SearchSubmitEvent {
    pub text: String,
}

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
        on_move_up: None,
        on_move_down: None,
        on_cancel: None,
        style: StyleRefinement::default(),
    }
}

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
    on_move_up: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
    on_move_down: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
    on_cancel: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
    style: StyleRefinement,
}

impl NativeSearchField {
    pub fn value(mut self, value: impl Into<SharedString>) -> Self {
        self.value = value.into();
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn sends_search_string_immediately(mut self, value: bool) -> Self {
        self.sends_search_string_immediately = value;
        self
    }

    pub fn sends_whole_search_string(mut self, value: bool) -> Self {
        self.sends_whole_search_string = value;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_change(
        mut self,
        listener: impl Fn(&SearchChangeEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Box::new(listener));
        self
    }

    pub fn on_submit(
        mut self,
        listener: impl Fn(&SearchSubmitEvent, &mut Window, &mut App) + 'static,
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
        listener: impl Fn(&SearchSubmitEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_blur = Some(Box::new(listener));
        self
    }

    pub fn on_move_up(
        mut self,
        listener: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_move_up = Some(Box::new(listener));
        self
    }

    pub fn on_move_down(
        mut self,
        listener: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_move_down = Some(Box::new(listener));
        self
    }

    pub fn on_cancel(
        mut self,
        listener: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_cancel = Some(Box::new(listener));
        self
    }
}

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
        let parent = window.raw_native_view_ptr();
        if parent.is_null() {
            return;
        }

        let on_change = self.on_change.take();
        let on_submit = self.on_submit.take();
        let on_focus = self.on_focus.take();
        let on_blur = self.on_blur.take();
        let on_move_up = self.on_move_up.take();
        let on_move_down = self.on_move_down.take();
        let on_cancel = self.on_cancel.take();
        let identifier: SharedString = self.id.to_string().into();
        let value = self.value.clone();
        let placeholder = self.placeholder.clone();
        let sends_immediately = self.sends_search_string_immediately;
        let sends_whole = self.sends_whole_search_string;
        let disabled = self.disabled;

        let nfc = window.next_frame_callbacks.clone();
        let inv = window.invalidator.clone();

        window.with_optional_element_state::<NativeControlState, _>(id, |prev_state, window| {
            let mut state = prev_state.flatten().unwrap_or_default();

            let callbacks = build_search_field_callbacks(
                on_change, on_submit, on_focus, on_blur, on_move_up, on_move_down, on_cancel,
                nfc, inv,
            );

            let scale = window.scale_factor();
            let nc = window.native_controls();
            nc.update_search_field(
                &mut state,
                parent,
                bounds,
                scale,
                SearchFieldConfig {
                    placeholder: &placeholder,
                    value: &value,
                    identifier: Some(identifier.as_ref()),
                    sends_immediately,
                    sends_whole_string: sends_whole,
                    enabled: !disabled,
                    callbacks,
                },
            );

            ((), Some(state))
        });
    }
}

fn build_search_field_callbacks(
    on_change: Option<Box<dyn Fn(&SearchChangeEvent, &mut Window, &mut App) + 'static>>,
    on_submit: Option<Box<dyn Fn(&SearchSubmitEvent, &mut Window, &mut App) + 'static>>,
    on_focus: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
    on_blur: Option<Box<dyn Fn(&SearchSubmitEvent, &mut Window, &mut App) + 'static>>,
    on_move_up: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
    on_move_down: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
    on_cancel: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
    next_frame_callbacks: Rc<RefCell<Vec<FrameCallback>>>,
    invalidator: crate::WindowInvalidator,
) -> TextFieldCallbacks {
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

    let move_up_cb = on_move_up.map(|h| {
        schedule_native_focus_callback(
            Rc::new(h),
            next_frame_callbacks.clone(),
            invalidator.clone(),
        )
    });

    let move_down_cb = on_move_down.map(|h| {
        schedule_native_focus_callback(
            Rc::new(h),
            next_frame_callbacks.clone(),
            invalidator.clone(),
        )
    });

    let cancel_cb = on_cancel.map(|h| {
        schedule_native_focus_callback(Rc::new(h), next_frame_callbacks, invalidator)
    });

    TextFieldCallbacks {
        on_change: change_cb,
        on_begin_editing: begin_cb,
        on_end_editing: end_cb,
        on_submit: submit_cb,
        on_move_up: move_up_cb,
        on_move_down: move_down_cb,
        on_cancel: cancel_cb,
    }
}

impl Styled for NativeSearchField {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
