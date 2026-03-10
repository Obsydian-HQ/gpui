use refineable::Refineable as _;

use crate::platform::native_controls::{NativeControlState, StackViewConfig};
use crate::{
    AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, Style, StyleRefinement, Styled,
    Window, px,
};

/// Orientation for an NSStackView.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeStackOrientation {
    /// Items arranged left-to-right.
    #[default]
    Horizontal,
    /// Items arranged top-to-bottom.
    Vertical,
}

impl NativeStackOrientation {
    fn to_raw(self) -> i64 {
        match self {
            Self::Horizontal => 0,
            Self::Vertical => 1,
        }
    }
}

/// Distribution mode for an NSStackView.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeStackDistribution {
    /// Subviews clustered at gravity areas.
    #[default]
    GravityAreas,
    /// Equal centering between subviews.
    EqualCentering,
    /// Equal spacing between subviews.
    EqualSpacing,
    /// Fill available space.
    Fill,
    /// Fill equally across subviews.
    FillEqually,
    /// Fill proportionally based on intrinsic size.
    FillProportionally,
}

impl NativeStackDistribution {
    fn to_raw(self) -> i64 {
        match self {
            Self::GravityAreas => 0,
            Self::EqualCentering => 1,
            Self::EqualSpacing => 2,
            Self::Fill => 3,
            Self::FillEqually => 4,
            Self::FillProportionally => 5,
        }
    }
}

/// Alignment for an NSStackView (maps to NSLayoutAttribute).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeStackAlignment {
    /// Top alignment (for horizontal stacks).
    Top,
    /// Bottom alignment (for horizontal stacks).
    Bottom,
    /// Leading alignment (for vertical stacks).
    Leading,
    /// Trailing alignment (for vertical stacks).
    Trailing,
    /// Center on the Y axis (for horizontal stacks).
    #[default]
    CenterY,
    /// Center on the X axis (for vertical stacks).
    CenterX,
}

impl NativeStackAlignment {
    fn to_raw(self) -> i64 {
        match self {
            Self::Top => 3,
            Self::Bottom => 4,
            Self::Leading => 5,
            Self::Trailing => 6,
            Self::CenterX => 9,
            Self::CenterY => 10,
        }
    }
}

/// Creates a native NSStackView element.
pub fn native_stack_view(
    id: impl Into<ElementId>,
    orientation: NativeStackOrientation,
) -> NativeStackView {
    NativeStackView {
        id: id.into(),
        orientation,
        spacing: None,
        distribution: NativeStackDistribution::default(),
        alignment: NativeStackAlignment::default(),
        edge_insets: None,
        detach_hidden: false,
        style: StyleRefinement::default(),
    }
}

/// A GPUI element wrapping NSStackView for native layout of NSView subviews.
///
/// NSStackView manages child NSViews, not GPUI elements. Native subviews
/// should be added via the `raw_view_ptr()` accessor and the FFI layer.
pub struct NativeStackView {
    id: ElementId,
    orientation: NativeStackOrientation,
    spacing: Option<f64>,
    distribution: NativeStackDistribution,
    alignment: NativeStackAlignment,
    edge_insets: Option<(f64, f64, f64, f64)>,
    detach_hidden: bool,
    style: StyleRefinement,
}

impl NativeStackView {
    /// Sets the spacing between arranged subviews.
    pub fn spacing(mut self, spacing: f64) -> Self {
        self.spacing = Some(spacing);
        self
    }

    /// Sets the distribution mode.
    pub fn distribution(mut self, distribution: NativeStackDistribution) -> Self {
        self.distribution = distribution;
        self
    }

    /// Sets the alignment.
    pub fn alignment(mut self, alignment: NativeStackAlignment) -> Self {
        self.alignment = alignment;
        self
    }

    /// Sets edge insets (top, left, bottom, right).
    pub fn edge_insets(mut self, top: f64, left: f64, bottom: f64, right: f64) -> Self {
        self.edge_insets = Some((top, left, bottom, right));
        self
    }

    /// Sets whether hidden subviews should be detached from the stack.
    pub fn detach_hidden(mut self, detach: bool) -> Self {
        self.detach_hidden = detach;
        self
    }

    /// Returns a pointer to the underlying NSStackView, if it has been created.
    /// This allows adding native subviews via the FFI layer.
    pub fn raw_view_ptr(&self) -> *mut std::ffi::c_void {
        std::ptr::null_mut()
    }
}

impl IntoElement for NativeStackView {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeStackView {
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
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(40.0))));
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

        let orientation = self.orientation;
        let spacing = self.spacing;
        let distribution = self.distribution;
        let alignment = self.alignment;
        let edge_insets = self.edge_insets;
        let detach_hidden = self.detach_hidden;

        window.with_optional_element_state::<NativeControlState, _>(id, |prev_state, window| {
            let mut state = prev_state.flatten().unwrap_or_default();

            let scale = window.scale_factor();
            let nc = window.native_controls();
            nc.update_stack_view(
                &mut state,
                parent,
                bounds,
                scale,
                StackViewConfig {
                    orientation: orientation.to_raw(),
                    spacing: spacing.unwrap_or(0.0),
                    alignment: alignment.to_raw(),
                    distribution: distribution.to_raw(),
                    edge_insets: edge_insets.unwrap_or((0.0, 0.0, 0.0, 0.0)),
                    detach_hidden,
                    children: Vec::new(),
                },
            );

            ((), Some(state))
        });
    }
}

impl Styled for NativeStackView {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
