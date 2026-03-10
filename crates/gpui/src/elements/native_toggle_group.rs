use refineable::Refineable as _;
use std::rc::Rc;

use crate::platform::native_controls::{NativeControlState, SegmentedControlConfig};
use crate::{
    px, AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, SharedString, Style,
    StyleRefinement, Styled, Window,
};

use super::native_element_helpers::schedule_native_callback;

#[derive(Clone, Debug)]
pub struct SegmentSelectEvent {
    pub index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeSegmentedShape {
    #[default]
    Automatic,
    Capsule,
    RoundedRectangle,
    Circle,
}

impl NativeSegmentedShape {
    fn to_raw(self) -> i64 {
        match self {
            NativeSegmentedShape::Automatic => 0,
            NativeSegmentedShape::Capsule => 1,
            NativeSegmentedShape::RoundedRectangle => 2,
            NativeSegmentedShape::Circle => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeSegmentedStyle {
    #[default]
    Automatic,
    Capsule,
    RoundRect,
    Circle,
}

impl From<NativeSegmentedStyle> for NativeSegmentedShape {
    fn from(style: NativeSegmentedStyle) -> Self {
        match style {
            NativeSegmentedStyle::Automatic => NativeSegmentedShape::Automatic,
            NativeSegmentedStyle::Capsule => NativeSegmentedShape::Capsule,
            NativeSegmentedStyle::RoundRect => NativeSegmentedShape::RoundedRectangle,
            NativeSegmentedStyle::Circle => NativeSegmentedShape::Circle,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeSegmentedSize {
    Mini,
    Small,
    #[default]
    Regular,
    Large,
    ExtraLarge,
}

impl NativeSegmentedSize {
    fn to_raw(self) -> u64 {
        match self {
            NativeSegmentedSize::Mini => 2,
            NativeSegmentedSize::Small => 1,
            NativeSegmentedSize::Regular => 0,
            NativeSegmentedSize::Large => 3,
            NativeSegmentedSize::ExtraLarge => 4,
        }
    }
}

pub fn native_toggle_group(
    id: impl Into<ElementId>,
    labels: &[impl AsRef<str>],
) -> NativeToggleGroup {
    NativeToggleGroup {
        id: id.into(),
        labels: labels
            .iter()
            .map(|l| SharedString::from(l.as_ref().to_string()))
            .collect(),
        symbols: None,
        selected_index: None,
        on_select: None,
        style: StyleRefinement::default(),
        border_shape: NativeSegmentedShape::default(),
        control_size: NativeSegmentedSize::default(),
    }
}

pub struct NativeToggleGroup {
    id: ElementId,
    labels: Vec<SharedString>,
    symbols: Option<Vec<SharedString>>,
    selected_index: Option<usize>,
    on_select: Option<Box<dyn Fn(&SegmentSelectEvent, &mut Window, &mut App) + 'static>>,
    style: StyleRefinement,
    border_shape: NativeSegmentedShape,
    control_size: NativeSegmentedSize,
}

impl NativeToggleGroup {
    pub fn selected_index(mut self, index: usize) -> Self {
        self.selected_index = Some(index);
        self
    }

    pub fn on_select(
        mut self,
        listener: impl Fn(&SegmentSelectEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Box::new(listener));
        self
    }

    pub fn border_shape(mut self, shape: NativeSegmentedShape) -> Self {
        self.border_shape = shape;
        self
    }

    pub fn segment_style(mut self, style: NativeSegmentedStyle) -> Self {
        self.border_shape = style.into();
        self
    }

    pub fn control_size(mut self, size: NativeSegmentedSize) -> Self {
        self.control_size = size;
        self
    }

    pub fn sf_symbols(mut self, symbols: &[impl AsRef<str>]) -> Self {
        self.symbols = Some(
            symbols
                .iter()
                .map(|s| SharedString::from(s.as_ref().to_string()))
                .collect(),
        );
        self
    }
}

impl IntoElement for NativeToggleGroup {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeToggleGroup {
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
            let per_segment = if self.symbols.is_some() { 36.0 } else { 70.0 };
            let width = (self.labels.len() as f32 * per_segment).max(72.0);
            style.size.width =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(width))));
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

        let on_select = self.on_select.take();
        let labels = self.labels.clone();
        let symbols = self.symbols.clone();
        let selected_index = self.selected_index;
        let border_shape = self.border_shape;
        let control_size = self.control_size;

        let next_frame_callbacks = window.next_frame_callbacks.clone();
        let invalidator = window.invalidator.clone();

        window.with_optional_element_state::<NativeControlState, _>(id, |prev_state, window| {
            let mut state = prev_state.flatten().unwrap_or_default();

            let on_select_fn = on_select.map(|handler| {
                let handler = Rc::new(handler);
                schedule_native_callback(
                    handler,
                    |index| SegmentSelectEvent { index },
                    next_frame_callbacks.clone(),
                    invalidator.clone(),
                )
            });

            let label_strs: Vec<&str> = labels.iter().map(|s| s.as_ref()).collect();
            let image_pairs: Vec<(usize, &str)> = symbols
                .as_ref()
                .map(|syms| {
                    syms.iter()
                        .enumerate()
                        .filter(|(_, s)| !s.is_empty())
                        .map(|(i, s)| (i, s.as_ref()))
                        .collect()
                })
                .unwrap_or_default();

            let scale = window.scale_factor();
            let nc = window.native_controls();
            nc.update_segmented_control(
                &mut state,
                parent,
                bounds,
                scale,
                SegmentedControlConfig {
                    labels: &label_strs,
                    selected_index,
                    border_shape: border_shape.to_raw(),
                    control_size: control_size.to_raw(),
                    images: &image_pairs,
                    enabled: true,
                    on_select: on_select_fn,
                },
            );

            ((), Some(state))
        });
    }
}

impl Styled for NativeToggleGroup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
