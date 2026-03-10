use refineable::Refineable as _;

use crate::platform::native_controls::{NativeControlState, VisualEffectViewConfig};
use crate::{
    AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, Style, StyleRefinement, Styled,
    Window, px,
};

/// NSVisualEffectMaterial values for configuring vibrancy appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeVisualEffectMaterial {
    /// The material for a window's titlebar area.
    Titlebar,
    /// The material for selected content.
    Selection,
    /// The material for menus.
    Menu,
    /// The material for popovers.
    Popover,
    /// The material for a source list sidebar area.
    #[default]
    Sidebar,
    /// The material for a header view area.
    HeaderView,
    /// The material for a sheet.
    Sheet,
    /// The material for a window background.
    WindowBackground,
    /// The material for a HUD window.
    HudWindow,
    /// The material for full-screen UI.
    FullScreenUI,
    /// The material for a tooltip.
    ToolTip,
    /// The material for content background areas.
    ContentBackground,
    /// The material that appears under the window.
    UnderWindow,
    /// The material that appears under the page.
    UnderPage,
}

impl NativeVisualEffectMaterial {
    fn to_raw(self) -> i64 {
        match self {
            Self::Titlebar => 3,
            Self::Selection => 4,
            Self::Menu => 5,
            Self::Popover => 6,
            Self::Sidebar => 7,
            Self::HeaderView => 10,
            Self::Sheet => 11,
            Self::WindowBackground => 12,
            Self::HudWindow => 13,
            Self::FullScreenUI => 15,
            Self::ToolTip => 17,
            Self::ContentBackground => 18,
            Self::UnderWindow => 21,
            Self::UnderPage => 22,
        }
    }
}

/// NSVisualEffectBlendingMode values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeVisualEffectBlendingMode {
    /// Blends with content behind the window.
    #[default]
    BehindWindow,
    /// Blends with content within the window.
    WithinWindow,
}

impl NativeVisualEffectBlendingMode {
    fn to_raw(self) -> i64 {
        match self {
            Self::BehindWindow => 0,
            Self::WithinWindow => 1,
        }
    }
}

/// NSVisualEffectState values controlling when the effect is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeVisualEffectState {
    /// The effect follows the window's active state.
    #[default]
    FollowsWindowActiveState,
    /// The effect is always active.
    Active,
    /// The effect is always inactive.
    Inactive,
}

impl NativeVisualEffectState {
    fn to_raw(self) -> i64 {
        match self {
            Self::FollowsWindowActiveState => 0,
            Self::Active => 1,
            Self::Inactive => 2,
        }
    }
}

/// Creates a native NSVisualEffectView element with the specified material.
pub fn native_visual_effect_view(
    id: impl Into<ElementId>,
    material: NativeVisualEffectMaterial,
) -> NativeVisualEffectView {
    NativeVisualEffectView {
        id: id.into(),
        material,
        blending_mode: NativeVisualEffectBlendingMode::default(),
        state: NativeVisualEffectState::default(),
        emphasized: false,
        corner_radius: None,
        style: StyleRefinement::default(),
    }
}

/// A GPUI element wrapping NSVisualEffectView for native macOS vibrancy.
pub struct NativeVisualEffectView {
    id: ElementId,
    material: NativeVisualEffectMaterial,
    blending_mode: NativeVisualEffectBlendingMode,
    state: NativeVisualEffectState,
    emphasized: bool,
    corner_radius: Option<f64>,
    style: StyleRefinement,
}

impl NativeVisualEffectView {
    /// Sets the blending mode (BehindWindow or WithinWindow).
    pub fn blending_mode(mut self, mode: NativeVisualEffectBlendingMode) -> Self {
        self.blending_mode = mode;
        self
    }

    /// Sets the visual effect state.
    pub fn state(mut self, state: NativeVisualEffectState) -> Self {
        self.state = state;
        self
    }

    /// Sets whether the view uses an emphasized appearance.
    pub fn emphasized(mut self, emphasized: bool) -> Self {
        self.emphasized = emphasized;
        self
    }

    /// Sets the corner radius via the backing layer.
    pub fn corner_radius(mut self, radius: f64) -> Self {
        self.corner_radius = Some(radius);
        self
    }
}

impl IntoElement for NativeVisualEffectView {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeVisualEffectView {
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
        let parent = window.raw_native_view_ptr();
        if parent.is_null() {
            return;
        }

        let material = self.material;
        let blending_mode = self.blending_mode;
        let state = self.state;
        let emphasized = self.emphasized;
        let corner_radius = self.corner_radius;

        window.with_optional_element_state::<NativeControlState, _>(id, |prev_state, window| {
            let mut control_state = prev_state.flatten().unwrap_or_default();

            let scale = window.scale_factor();
            let nc = window.native_controls();
            nc.update_visual_effect_view(
                &mut control_state,
                parent,
                bounds,
                scale,
                VisualEffectViewConfig {
                    material: material.to_raw(),
                    blending_mode: blending_mode.to_raw(),
                    state: state.to_raw(),
                    emphasized,
                    corner_radius: corner_radius.unwrap_or(0.0),
                },
            );

            ((), Some(control_state))
        });
    }
}

impl Styled for NativeVisualEffectView {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
