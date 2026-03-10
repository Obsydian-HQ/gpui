use refineable::Refineable as _;
use std::rc::Rc;

use crate::platform::native_controls::{ButtonConfig, ButtonStyle, NativeControlState};
use crate::{
    px, AbsoluteLength, App, Bounds, ClickEvent, DefiniteLength, Element, ElementId,
    GlobalElementId, InspectorElementId, IntoElement, LayoutId, Length, Pixels, SharedString,
    Style, StyleRefinement, Styled, Window,
};

use super::native_element_helpers::schedule_native_callback_no_args;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeButtonStyle {
    #[default]
    Rounded,
    Filled,
    Inline,
    Borderless,
}

impl From<NativeButtonStyle> for ButtonStyle {
    fn from(s: NativeButtonStyle) -> Self {
        match s {
            NativeButtonStyle::Rounded => ButtonStyle::Rounded,
            NativeButtonStyle::Filled => ButtonStyle::Filled,
            NativeButtonStyle::Inline => ButtonStyle::Inline,
            NativeButtonStyle::Borderless => ButtonStyle::Borderless,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeButtonTint {
    Accent,
    Destructive,
    Warning,
    Success,
}

impl NativeButtonTint {
    pub fn rgba(self) -> (f64, f64, f64, f64) {
        match self {
            NativeButtonTint::Accent => (0.0, 0.478, 1.0, 1.0),
            NativeButtonTint::Destructive => (1.0, 0.231, 0.188, 1.0),
            NativeButtonTint::Warning => (1.0, 0.584, 0.0, 1.0),
            NativeButtonTint::Success => (0.196, 0.843, 0.294, 1.0),
        }
    }
}

pub fn native_button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> NativeButton {
    NativeButton {
        id: id.into(),
        label: label.into(),
        on_click: None,
        style: StyleRefinement::default(),
        button_style: NativeButtonStyle::default(),
        tint: None,
        disabled: false,
    }
}

pub struct NativeButton {
    id: ElementId,
    label: SharedString,
    on_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    style: StyleRefinement,
    button_style: NativeButtonStyle,
    tint: Option<NativeButtonTint>,
    disabled: bool,
}

impl NativeButton {
    pub fn on_click(
        mut self,
        listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(listener));
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

impl IntoElement for NativeButton {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeButton {
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
            let char_width = 8.0;
            let padding = 24.0;
            let width = (self.label.len() as f32 * char_width + padding).max(80.0);
            style.size.width =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(width))));
        }
        if matches!(style.size.height, Length::Auto) {
            style.size.height =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(24.0))));
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
        let label = self.label.clone();
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
                    title: &label,
                    sf_symbol: None,
                    tooltip: None,
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

impl Styled for NativeButton {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
