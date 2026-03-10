use refineable::Refineable as _;
use std::rc::Rc;

use crate::platform::native_controls::{NativeControlState, StepperConfig};
use crate::{
    px, AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, Style, StyleRefinement, Styled,
    Window,
};

use super::native_element_helpers::schedule_native_callback;

#[derive(Clone, Debug)]
pub struct StepperChangeEvent {
    pub value: f64,
}

pub fn native_stepper(id: impl Into<ElementId>) -> NativeStepper {
    NativeStepper {
        id: id.into(),
        min: 0.0,
        max: 100.0,
        value: 0.0,
        increment: 1.0,
        wraps: false,
        autorepeat: true,
        on_change: None,
        disabled: false,
        style: StyleRefinement::default(),
    }
}

pub struct NativeStepper {
    id: ElementId,
    min: f64,
    max: f64,
    value: f64,
    increment: f64,
    wraps: bool,
    autorepeat: bool,
    on_change: Option<Box<dyn Fn(&StepperChangeEvent, &mut Window, &mut App) + 'static>>,
    disabled: bool,
    style: StyleRefinement,
}

impl NativeStepper {
    pub fn range(mut self, min: f64, max: f64) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    pub fn min(mut self, min: f64) -> Self {
        self.min = min;
        self
    }

    pub fn max(mut self, max: f64) -> Self {
        self.max = max;
        self
    }

    pub fn value(mut self, value: f64) -> Self {
        self.value = value;
        self
    }

    pub fn increment(mut self, increment: f64) -> Self {
        self.increment = increment;
        self
    }

    pub fn wraps(mut self, wraps: bool) -> Self {
        self.wraps = wraps;
        self
    }

    pub fn autorepeat(mut self, autorepeat: bool) -> Self {
        self.autorepeat = autorepeat;
        self
    }

    pub fn on_change(
        mut self,
        listener: impl Fn(&StepperChangeEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Box::new(listener));
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl IntoElement for NativeStepper {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeStepper {
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
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(20.0))));
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
        let (min, max) = if self.min <= self.max {
            (self.min, self.max)
        } else {
            (self.max, self.min)
        };
        let value = self.value.max(min).min(max);
        let increment = self.increment.max(f64::EPSILON);
        let wraps = self.wraps;
        let autorepeat = self.autorepeat;
        let disabled = self.disabled;

        let next_frame_callbacks = window.next_frame_callbacks.clone();
        let invalidator = window.invalidator.clone();

        window.with_optional_element_state::<NativeControlState, _>(id, |prev_state, window| {
            let mut state = prev_state.flatten().unwrap_or_default();

            let on_change_fn = on_change.map(|handler| {
                let handler = Rc::new(handler);
                schedule_native_callback(
                    handler,
                    |value| StepperChangeEvent { value },
                    next_frame_callbacks.clone(),
                    invalidator.clone(),
                )
            });

            let scale = window.scale_factor();
            let nc = window.native_controls();
            nc.update_stepper(
                &mut state,
                parent,
                bounds,
                scale,
                StepperConfig {
                    min,
                    max,
                    value,
                    increment,
                    wraps,
                    autorepeat,
                    enabled: !disabled,
                    on_change: on_change_fn,
                },
            );

            ((), Some(state))
        });
    }
}

impl Styled for NativeStepper {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
