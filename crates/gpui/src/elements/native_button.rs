use refineable::Refineable as _;
use std::ffi::c_void;
use std::rc::Rc;

use crate::{
    AbsoluteLength, App, Bounds, ClickEvent, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, SharedString, Style,
    StyleRefinement, Styled, Window, px,
};

use super::native_element_helpers::schedule_native_callback_no_args;

// =============================================================================
// Style & tint enums
// =============================================================================

/// Visual style for a native platform button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeButtonStyle {
    /// Standard macOS rounded push button (NSBezelStyleRounded).
    #[default]
    Rounded,
    /// Filled / prominent appearance. Uses bezel color for emphasis.
    Filled,
    /// Inline / accessory bar style. Compact, minimal chrome.
    Inline,
    /// Borderless — no bezel, just text or icon. Reacts on hover.
    Borderless,
}

/// Semantic tint color applied to a native button's bezel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeButtonTint {
    /// System accent color (typically blue).
    Accent,
    /// Destructive / error (red).
    Destructive,
    /// Warning (orange).
    Warning,
    /// Success (green).
    Success,
}

impl NativeButtonTint {
    /// Returns the RGBA color components for this tint (sRGB, 0.0–1.0).
    pub fn rgba(self) -> (f64, f64, f64, f64) {
        match self {
            NativeButtonTint::Accent => (0.0, 0.478, 1.0, 1.0),
            NativeButtonTint::Destructive => (1.0, 0.231, 0.188, 1.0),
            NativeButtonTint::Warning => (1.0, 0.584, 0.0, 1.0),
            NativeButtonTint::Success => (0.196, 0.843, 0.294, 1.0),
        }
    }
}

// =============================================================================
// Public constructor
// =============================================================================

/// Creates a native platform button element (NSButton on macOS).
///
/// The button participates in GPUI's Taffy layout system and renders as a real
/// platform button, not a custom-drawn element.
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

// =============================================================================
// Element struct
// =============================================================================

/// A native platform button element that creates a real OS button (NSButton on macOS)
/// as a subview of the window's native view, positioned by GPUI's Taffy layout engine.
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
    /// Register a callback to be invoked when the button is clicked.
    pub fn on_click(
        mut self,
        listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(listener));
        self
    }

    /// Set the visual style of the button.
    pub fn button_style(mut self, style: NativeButtonStyle) -> Self {
        self.button_style = style;
        self
    }

    /// Set a semantic tint color on the button bezel.
    pub fn tint(mut self, tint: NativeButtonTint) -> Self {
        self.tint = Some(tint);
        self
    }

    /// Set whether the button is disabled (grayed out, not clickable).
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

// =============================================================================
// Persisted element state
// =============================================================================

struct NativeButtonElementState {
    native_button_ptr: *mut c_void,
    native_target_ptr: *mut c_void,
    current_label: SharedString,
    current_style: NativeButtonStyle,
    current_tint: Option<NativeButtonTint>,
    attached: bool,
}

impl Drop for NativeButtonElementState {
    fn drop(&mut self) {
        if self.attached {
            #[cfg(target_os = "macos")]
            unsafe {
                use crate::platform::native_controls;
                super::native_element_helpers::cleanup_native_control(
                    self.native_button_ptr,
                    self.native_target_ptr,
                    native_controls::release_native_button_target,
                    native_controls::release_native_button,
                );
            }
        }
    }
}

unsafe impl Send for NativeButtonElementState {}

// =============================================================================
// Platform helpers
// =============================================================================

#[cfg(target_os = "macos")]
fn apply_button_style(button: cocoa::base::id, style: NativeButtonStyle) {
    unsafe {
        use crate::platform::native_controls;
        match style {
            NativeButtonStyle::Rounded => {
                native_controls::set_native_button_bezel_style(button, 1); // NSBezelStyleRounded
                native_controls::set_native_button_bordered(button, true);
                native_controls::set_native_button_shows_border_on_hover(button, false);
            }
            NativeButtonStyle::Filled => {
                // NSBezelStyleAccessoryBarAction (12) = flat rounded-rect, visually
                // distinct from the capsule-shaped Push (1).
                native_controls::set_native_button_bezel_style(button, 12);
                native_controls::set_native_button_bordered(button, true);
                native_controls::set_native_button_shows_border_on_hover(button, false);
                native_controls::set_native_button_bezel_color_accent(button);
                native_controls::set_native_button_content_tint_color(button, 1.0, 1.0, 1.0, 1.0);
            }
            NativeButtonStyle::Inline => {
                native_controls::set_native_button_bezel_style(button, 15); // NSBezelStyleInline
                native_controls::set_native_button_bordered(button, true);
                native_controls::set_native_button_shows_border_on_hover(button, false);
            }
            NativeButtonStyle::Borderless => {
                // Bordered + showsBorderOnlyWhileMouseInside gives borderless look
                // that highlights on hover
                native_controls::set_native_button_bezel_style(button, 1); // NSBezelStyleRounded
                native_controls::set_native_button_bordered(button, true);
                native_controls::set_native_button_shows_border_on_hover(button, true);
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn apply_button_tint(button: cocoa::base::id, tint: Option<NativeButtonTint>) {
    if let Some(tint) = tint {
        let (r, g, b, a) = tint.rgba();
        unsafe {
            use crate::platform::native_controls;
            native_controls::set_native_button_bezel_color(button, r, g, b, a);
            // For borderless/inline buttons, also tint the text
            native_controls::set_native_button_content_tint_color(button, 1.0, 1.0, 1.0, 1.0);
        }
    }
}

// =============================================================================
// Element trait impl
// =============================================================================

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
        cx: &mut App,
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

            let native_view = window.raw_native_view_ptr();
            if native_view.is_null() {
                return;
            }

            let on_click = self.on_click.take();
            let label = self.label.clone();
            let button_style = self.button_style;
            let tint = self.tint;
            let disabled = self.disabled;

            let next_frame_callbacks = window.next_frame_callbacks.clone();
            let invalidator = window.invalidator.clone();

            window.with_optional_element_state::<NativeButtonElementState, _>(
                id,
                |prev_state, window| {
                    let state = if let Some(Some(mut state)) = prev_state {
                        unsafe {
                            native_controls::set_native_button_frame(
                                state.native_button_ptr as cocoa::base::id,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );
                            if state.current_label != label {
                                native_controls::set_native_button_title(
                                    state.native_button_ptr as cocoa::base::id,
                                    &label,
                                );
                                state.current_label = label;
                            }
                            if state.current_style != button_style {
                                apply_button_style(
                                    state.native_button_ptr as cocoa::base::id,
                                    button_style,
                                );
                                state.current_style = button_style;
                            }
                            if state.current_tint != tint {
                                apply_button_tint(state.native_button_ptr as cocoa::base::id, tint);
                                state.current_tint = tint;
                            }
                            native_controls::set_native_control_enabled(
                                state.native_button_ptr as cocoa::base::id,
                                !disabled,
                            );
                        }

                        if let Some(on_click) = on_click {
                            unsafe {
                                native_controls::release_native_button_target(
                                    state.native_target_ptr,
                                );
                            }
                            let nfc = next_frame_callbacks.clone();
                            let inv = invalidator.clone();
                            let on_click = Rc::new(on_click);
                            let callback = schedule_native_callback_no_args(
                                on_click,
                                || ClickEvent::default(),
                                nfc,
                                inv,
                            );
                            unsafe {
                                state.native_target_ptr = native_controls::set_native_button_action(
                                    state.native_button_ptr as cocoa::base::id,
                                    callback,
                                );
                            }
                        }

                        state
                    } else {
                        let (button_ptr, target_ptr) = unsafe {
                            let button = native_controls::create_native_button(&label);
                            native_controls::attach_native_button_to_view(
                                button,
                                native_view as cocoa::base::id,
                            );
                            native_controls::set_native_button_frame(
                                button,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );

                            apply_button_style(button, button_style);
                            apply_button_tint(button, tint);
                            native_controls::set_native_control_enabled(button, !disabled);

                            let target = if let Some(on_click) = on_click {
                                let nfc = next_frame_callbacks.clone();
                                let inv = invalidator.clone();
                                let on_click = Rc::new(on_click);
                                let callback = schedule_native_callback_no_args(
                                    on_click,
                                    || ClickEvent::default(),
                                    nfc,
                                    inv,
                                );
                                native_controls::set_native_button_action(button, callback)
                            } else {
                                std::ptr::null_mut()
                            };

                            (button as *mut c_void, target)
                        };

                        NativeButtonElementState {
                            native_button_ptr: button_ptr,
                            native_target_ptr: target_ptr,
                            current_label: label,
                            current_style: button_style,
                            current_tint: tint,
                            attached: true,
                        }
                    };

                    ((), Some(state))
                },
            );
        }
    }
}

impl Styled for NativeButton {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
