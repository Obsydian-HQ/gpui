use refineable::Refineable as _;
use std::rc::Rc;

use crate::platform::native_controls::{CheckboxConfig, NativeControlState};
use crate::{
    px, AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, SharedString, Style,
    StyleRefinement, Styled, Window,
};

use super::native_element_helpers::schedule_native_callback;

#[derive(Clone, Debug)]
pub struct CheckboxChangeEvent {
    pub checked: bool,
}

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

pub struct NativeCheckbox {
    id: ElementId,
    label: SharedString,
    checked: bool,
    on_change: Option<Box<dyn Fn(&CheckboxChangeEvent, &mut Window, &mut App) + 'static>>,
    disabled: bool,
    style: StyleRefinement,
}

impl NativeCheckbox {
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn on_change(
        mut self,
        listener: impl Fn(&CheckboxChangeEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Box::new(listener));
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

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
        let parent = window.raw_native_view_ptr();
        if parent.is_null() {
            return;
        }

        let on_change = self.on_change.take();
        let label = self.label.clone();
        let checked = self.checked;
        let disabled = self.disabled;

        let next_frame_callbacks = window.next_frame_callbacks.clone();
        let invalidator = window.invalidator.clone();

        window.with_optional_element_state::<NativeControlState, _>(id, |prev_state, window| {
            let mut state = prev_state.flatten().unwrap_or_default();

            let on_change_fn = on_change.map(|handler| {
                let handler = Rc::new(handler);
                schedule_native_callback(
                    handler,
                    |checked| CheckboxChangeEvent { checked },
                    next_frame_callbacks.clone(),
                    invalidator.clone(),
                )
            });

            let scale = window.scale_factor();
            let nc = window.native_controls();
            nc.update_checkbox(
                &mut state,
                parent,
                bounds,
                scale,
                CheckboxConfig {
                    title: &label,
                    checked,
                    enabled: !disabled,
                    on_change: on_change_fn,
                },
            );

            ((), Some(state))
        });
    }
}

impl Styled for NativeCheckbox {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
