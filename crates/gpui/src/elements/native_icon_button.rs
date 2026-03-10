use refineable::Refineable as _;
use std::rc::Rc;

use crate::platform::native_controls::{ButtonConfig, NativeControlState};
use crate::{
    px, AbsoluteLength, App, Bounds, ClickEvent, DefiniteLength, Element, ElementId,
    GlobalElementId, InspectorElementId, IntoElement, LayoutId, Length, Pixels, SharedString,
    Style, StyleRefinement, Styled, Window,
};

use super::native_button::{NativeButtonStyle, NativeButtonTint};
use super::native_element_helpers::schedule_native_callback_no_args;

pub fn native_icon_button(
    id: impl Into<ElementId>,
    sf_symbol: impl Into<SharedString>,
) -> NativeIconButton {
    NativeIconButton {
        id: id.into(),
        sf_symbol: sf_symbol.into(),
        tooltip_label: None,
        on_click: None,
        style: StyleRefinement::default(),
        button_style: NativeButtonStyle::Borderless,
        tint: None,
        disabled: false,
    }
}

pub struct NativeIconButton {
    id: ElementId,
    sf_symbol: SharedString,
    tooltip_label: Option<SharedString>,
    on_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    style: StyleRefinement,
    button_style: NativeButtonStyle,
    tint: Option<NativeButtonTint>,
    disabled: bool,
}

impl NativeIconButton {
    pub fn on_click(
        mut self,
        listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(listener));
        self
    }

    pub fn tooltip(mut self, label: impl Into<SharedString>) -> Self {
        self.tooltip_label = Some(label.into());
        self
    }

    pub fn button_style(mut self, style: NativeButtonStyle) -> Self {
        self.button_style = style;
        self
    }

    pub fn tint(mut self, tint: NativeButtonTint) -> Self {
        self.tint = Some(tint);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl IntoElement for NativeIconButton {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeIconButton {
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
        _cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.refine(&self.style);

        if matches!(style.size.width, Length::Auto) {
            style.size.width =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(28.0))));
        }
        if matches!(style.size.height, Length::Auto) {
            style.size.height =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(28.0))));
        }

        let layout_id = window.request_layout(style, [], _cx);
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

        let on_click = self.on_click.take();
        let sf_symbol = self.sf_symbol.clone();
        let tooltip = self.tooltip_label.clone();
        let button_style = self.button_style;
        let tint = self.tint;
        let disabled = self.disabled;

        let next_frame_callbacks = window.next_frame_callbacks.clone();
        let invalidator = window.invalidator.clone();

        window.with_optional_element_state::<NativeControlState, _>(id, |prev_state, window| {
            let mut state = prev_state.flatten().unwrap_or_default();

            let on_click_fn = on_click.map(|handler| {
                let handler = Rc::new(handler);
                schedule_native_callback_no_args(
                    handler,
                    || ClickEvent::default(),
                    next_frame_callbacks.clone(),
                    invalidator.clone(),
                )
            });

            let scale = window.scale_factor();
            let nc = window.native_controls();
            nc.update_button(
                &mut state,
                parent,
                bounds,
                scale,
                ButtonConfig {
                    title: "",
                    sf_symbol: Some(&sf_symbol),
                    tooltip: tooltip.as_ref().map(|v| &**v as &str),
                    style: button_style.into(),
                    tint: tint.map(|t| t.rgba()),
                    enabled: !disabled,
                    on_click: on_click_fn,
                },
            );

            ((), Some(state))
        });
    }
}

impl Styled for NativeIconButton {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
