use refineable::Refineable as _;
use std::ffi::c_void;

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

struct NativeGlassEffectViewState {
    view_ptr: *mut c_void,
    current_style: NativeGlassEffectStyle,
    current_corner_radius: Option<f64>,
    current_tint_color: Option<Hsla>,
    attached: bool,
}

impl Drop for NativeGlassEffectViewState {
    fn drop(&mut self) {
        if self.attached {
            #[cfg(target_os = "macos")]
            unsafe {
                use crate::platform::native_controls;
                native_controls::release_native_glass_effect_view(
                    self.view_ptr as cocoa::base::id,
                );
            }
        }
    }
}

unsafe impl Send for NativeGlassEffectViewState {}

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
        #[cfg(target_os = "macos")]
        {
            use crate::platform::native_controls;

            if !native_controls::is_glass_effect_available() {
                return;
            }

            let native_view = window.raw_native_view_ptr();
            if native_view.is_null() {
                return;
            }

            let glass_style = self.glass_style;
            let corner_radius = self.corner_radius;
            let tint_color = self.tint_color;

            window.with_optional_element_state::<NativeGlassEffectViewState, _>(
                id,
                |prev_state, window| {
                    let state_val = if let Some(Some(mut element_state)) = prev_state {
                        unsafe {
                            native_controls::set_native_view_frame(
                                element_state.view_ptr as cocoa::base::id,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );

                            if element_state.current_style != glass_style {
                                native_controls::set_native_glass_effect_style(
                                    element_state.view_ptr as cocoa::base::id,
                                    glass_style.to_raw(),
                                );
                                element_state.current_style = glass_style;
                            }

                            if element_state.current_corner_radius != corner_radius {
                                if let Some(radius) = corner_radius {
                                    native_controls::set_native_glass_effect_corner_radius(
                                        element_state.view_ptr as cocoa::base::id,
                                        radius,
                                    );
                                }
                                element_state.current_corner_radius = corner_radius;
                            }

                            if element_state.current_tint_color != tint_color {
                                if let Some(color) = tint_color {
                                    let rgba = color.to_rgb();
                                    native_controls::set_native_glass_effect_tint_color(
                                        element_state.view_ptr as cocoa::base::id,
                                        rgba.r as f64,
                                        rgba.g as f64,
                                        rgba.b as f64,
                                        rgba.a as f64,
                                    );
                                } else {
                                    native_controls::clear_native_glass_effect_tint_color(
                                        element_state.view_ptr as cocoa::base::id,
                                    );
                                }
                                element_state.current_tint_color = tint_color;
                            }
                        }

                        element_state
                    } else {
                        unsafe {
                            let view = native_controls::create_native_glass_effect_view();
                            if view == cocoa::base::nil {
                                return ((), None);
                            }

                            native_controls::set_native_glass_effect_style(
                                view,
                                glass_style.to_raw(),
                            );

                            if let Some(radius) = corner_radius {
                                native_controls::set_native_glass_effect_corner_radius(
                                    view, radius,
                                );
                            }

                            if let Some(color) = tint_color {
                                let rgba = color.to_rgb();
                                native_controls::set_native_glass_effect_tint_color(
                                    view,
                                    rgba.r as f64,
                                    rgba.g as f64,
                                    rgba.b as f64,
                                    rgba.a as f64,
                                );
                            }

                            native_controls::attach_native_view_to_parent(
                                view,
                                native_view as cocoa::base::id,
                            );
                            native_controls::set_native_view_frame(
                                view,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );

                            NativeGlassEffectViewState {
                                view_ptr: view as *mut c_void,
                                current_style: glass_style,
                                current_corner_radius: corner_radius,
                                current_tint_color: tint_color,
                                attached: true,
                            }
                        }
                    };

                    ((), Some(state_val))
                },
            );
        }
    }
}

impl Styled for NativeGlassEffectView {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
