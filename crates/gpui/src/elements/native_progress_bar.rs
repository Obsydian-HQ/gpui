use refineable::Refineable as _;

use crate::platform::native_controls::{NativeControlState, ProgressConfig};
use crate::{
    px, AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, Style, StyleRefinement, Styled,
    Window,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeProgressStyle {
    #[default]
    Bar,
    Spinner,
}

impl NativeProgressStyle {
    fn to_raw(self) -> i64 {
        match self {
            NativeProgressStyle::Bar => 0,
            NativeProgressStyle::Spinner => 1,
        }
    }
}

pub fn native_progress_bar(id: impl Into<ElementId>) -> NativeProgressBar {
    NativeProgressBar {
        id: id.into(),
        value: Some(0.0),
        min: 0.0,
        max: 1.0,
        progress_style: NativeProgressStyle::Bar,
        displayed_when_stopped: true,
        style: StyleRefinement::default(),
    }
}

pub struct NativeProgressBar {
    id: ElementId,
    value: Option<f64>,
    min: f64,
    max: f64,
    progress_style: NativeProgressStyle,
    displayed_when_stopped: bool,
    style: StyleRefinement,
}

impl NativeProgressBar {
    pub fn maybe_value(mut self, value: Option<f64>) -> Self {
        self.value = value;
        self
    }

    pub fn value(mut self, value: f64) -> Self {
        self.value = Some(value);
        self
    }

    pub fn indeterminate(mut self, indeterminate: bool) -> Self {
        self.value = if indeterminate { None } else { Some(self.min) };
        self
    }

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

    pub fn progress_style(mut self, style: NativeProgressStyle) -> Self {
        self.progress_style = style;
        self
    }

    pub fn displayed_when_stopped(mut self, displayed: bool) -> Self {
        self.displayed_when_stopped = displayed;
        self
    }
}

impl IntoElement for NativeProgressBar {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeProgressBar {
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
            let width = if matches!(self.progress_style, NativeProgressStyle::Spinner) {
                32.0
            } else {
                200.0
            };
            style.size.width =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(width))));
        }
        if matches!(style.size.height, Length::Auto) {
            let height = if matches!(self.progress_style, NativeProgressStyle::Spinner) {
                32.0
            } else {
                20.0
            };
            style.size.height =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(height))));
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

        let progress_style = self.progress_style;
        let displayed_when_stopped = self.displayed_when_stopped;
        let (min, max) = if self.min <= self.max {
            (self.min, self.max)
        } else {
            (self.max, self.min)
        };
        let indeterminate =
            self.value.is_none() || matches!(progress_style, NativeProgressStyle::Spinner);
        let value = self.value.map(|v| v.max(min).min(max)).unwrap_or(min);
        let animating = indeterminate || matches!(progress_style, NativeProgressStyle::Spinner);

        window.with_optional_element_state::<NativeControlState, _>(id, |prev_state, window| {
            let mut state = prev_state.flatten().unwrap_or_default();

            let scale = window.scale_factor();
            let nc = window.native_controls();
            nc.update_progress(
                &mut state,
                parent,
                bounds,
                scale,
                ProgressConfig {
                    style: progress_style.to_raw(),
                    indeterminate,
                    value,
                    min,
                    max,
                    animating,
                    display_when_stopped: displayed_when_stopped,
                },
            );

            ((), Some(state))
        });
    }
}

impl Styled for NativeProgressBar {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
