use refineable::Refineable as _;

use crate::platform::native_controls::{GlassEffectViewConfig, NativeControlState};
use crate::{
    AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, Style, StyleRefinement, Styled,
    Window, px,
};

/// Style for NSGlassEffectView (macOS 26+).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeGlassEffectStyle {
    /// Standard glass with full visual effect.
    #[default]
    Regular,
    /// High transparency, minimal effect.
    Clear,
}

impl NativeGlassEffectStyle {
    fn to_raw(self) -> i64 {
        match self {
            Self::Regular => 0,
            Self::Clear => 1,
        }
    }
}

/// Creates a native NSGlassEffectView element (macOS 26+ Liquid Glass).
///
/// Returns a no-op transparent element on older macOS versions.
pub fn native_glass_effect_view(
    id: impl Into<ElementId>,
    glass_style: NativeGlassEffectStyle,
) -> NativeGlassEffectView {
    NativeGlassEffectView {
        id: id.into(),
        glass_style,
        corner_radius: None,
        tint_color: None,
        style: StyleRefinement::default(),
    }
}

/// A GPUI element wrapping NSGlassEffectView for macOS 26+ Liquid Glass.
///
/// On older macOS versions, renders as an empty transparent element.
pub struct NativeGlassEffectView {
    id: ElementId,
    glass_style: NativeGlassEffectStyle,
    corner_radius: Option<f64>,
    tint_color: Option<Hsla>,
    style: StyleRefinement,
}

impl NativeGlassEffectView {
    /// Sets the corner radius.
    pub fn corner_radius(mut self, radius: f64) -> Self {
        self.corner_radius = Some(radius);
        self
    }

    /// Sets a tint color for the glass effect.
    pub fn tint_color(mut self, color: Hsla) -> Self {
        self.tint_color = Some(color);
        self
    }
}

impl IntoElement for NativeGlassEffectView {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeGlassEffectView {
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
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(200.0))));
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
        if !window.native_controls().is_glass_effect_available() {
            return;
        }

        let parent = window.raw_native_view_ptr();
        if parent.is_null() {
            return;
        }

        let glass_style = self.glass_style;
        let corner_radius = self.corner_radius;
        let tint_color = self.tint_color;

        window.with_optional_element_state::<NativeControlState, _>(id, |prev_state, window| {
            let mut control_state = prev_state.flatten().unwrap_or_default();

            let tint = tint_color.map(|color| {
                let rgba = color.to_rgb();
                (rgba.r as f64, rgba.g as f64, rgba.b as f64, rgba.a as f64)
            });

            let scale = window.scale_factor();
            let nc = window.native_controls();
            nc.update_glass_effect_view(
                &mut control_state,
                parent,
                bounds,
                scale,
                GlassEffectViewConfig {
                    style: glass_style.to_raw(),
                    corner_radius: corner_radius.unwrap_or(0.0),
                    tint_color: tint,
                },
            );

            ((), Some(control_state))
        });
    }
}

impl Styled for NativeGlassEffectView {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
