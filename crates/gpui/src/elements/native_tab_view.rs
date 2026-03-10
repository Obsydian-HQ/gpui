use refineable::Refineable as _;
use std::rc::Rc;

use crate::platform::native_controls::{NativeControlState, TabViewConfig};
use crate::{
    px, AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, SharedString, Style,
    StyleRefinement, Styled, Window,
};

use super::native_element_helpers::schedule_native_callback;

#[derive(Clone, Debug)]
pub struct TabSelectEvent {
    pub index: usize,
}

pub fn native_tab_view(id: impl Into<ElementId>, labels: &[impl AsRef<str>]) -> NativeTabView {
    NativeTabView {
        id: id.into(),
        labels: labels
            .iter()
            .map(|label| SharedString::from(label.as_ref().to_string()))
            .collect(),
        selected_index: 0,
        on_select: None,
        style: StyleRefinement::default(),
    }
}

pub struct NativeTabView {
    id: ElementId,
    labels: Vec<SharedString>,
    selected_index: usize,
    on_select: Option<Box<dyn Fn(&TabSelectEvent, &mut Window, &mut App) + 'static>>,
    style: StyleRefinement,
}

impl NativeTabView {
    pub fn selected_index(mut self, selected_index: usize) -> Self {
        self.selected_index = selected_index;
        self
    }

    pub fn on_select(
        mut self,
        listener: impl Fn(&TabSelectEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Box::new(listener));
        self
    }
}

impl IntoElement for NativeTabView {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeTabView {
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
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(420.0))));
        }
        if matches!(style.size.height, Length::Auto) {
            style.size.height =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(280.0))));
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

        let on_select = self.on_select.take();
        let labels = self.labels.clone();
        let selected_index = self.selected_index;

        let next_frame_callbacks = window.next_frame_callbacks.clone();
        let invalidator = window.invalidator.clone();

        window.with_optional_element_state::<NativeControlState, _>(id, |prev_state, window| {
            let mut state = prev_state.flatten().unwrap_or_default();

            let on_select_fn = on_select.map(|handler| {
                let handler = Rc::new(handler);
                schedule_native_callback(
                    handler,
                    |index| TabSelectEvent { index },
                    next_frame_callbacks.clone(),
                    invalidator.clone(),
                )
            });

            let label_strs: Vec<&str> = labels.iter().map(|s| s.as_ref()).collect();

            let scale = window.scale_factor();
            let nc = window.native_controls();
            nc.update_tab_view(
                &mut state,
                parent,
                bounds,
                scale,
                TabViewConfig {
                    labels: &label_strs,
                    selected_index,
                    enabled: true,
                    on_select: on_select_fn,
                },
            );

            ((), Some(state))
        });
    }
}

impl Styled for NativeTabView {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
