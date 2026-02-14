use refineable::Refineable as _;
use std::ffi::c_void;
use std::rc::Rc;

use crate::{
    AbsoluteLength, App, Bounds, ClickEvent, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, SharedString, Style,
    StyleRefinement, Styled, Window, px,
};

use super::native_button::{NativeButtonStyle, NativeButtonTint};
use super::native_element_helpers::schedule_native_callback_no_args;

// =============================================================================
// Public constructor
// =============================================================================

/// Creates a native icon button using an SF Symbol name (macOS 11+).
///
/// Example SF Symbol names: "gear", "plus", "trash", "magnifyingglass",
/// "square.and.arrow.up", "folder", "bell", "person.crop.circle".
pub fn native_icon_button(
    id: impl Into<ElementId>,
    sf_symbol: impl Into<SharedString>,
) -> NativeIconButton {
    NativeIconButton {
        id: id.into(),
        sf_symbol: sf_symbol.into(),
        tooltip_label: None,
        on_click: None,
        style: StyleRefinement::default(),
        button_style: NativeButtonStyle::Borderless,
        tint: None,
        disabled: false,
    }
}

// =============================================================================
// Element struct
// =============================================================================

/// A native icon-only button that uses SF Symbols on macOS.
///
/// Renders as a real NSButton with an image, positioned by GPUI's Taffy layout.
pub struct NativeIconButton {
    id: ElementId,
    sf_symbol: SharedString,
    tooltip_label: Option<SharedString>,
    on_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    style: StyleRefinement,
    button_style: NativeButtonStyle,
    tint: Option<NativeButtonTint>,
    disabled: bool,
}

impl NativeIconButton {
    /// Register a callback to be invoked when the button is clicked.
    pub fn on_click(
        mut self,
        listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(listener));
        self
    }

    /// Set an accessibility tooltip for the button.
    pub fn tooltip(mut self, label: impl Into<SharedString>) -> Self {
        self.tooltip_label = Some(label.into());
        self
    }

    /// Set the visual style of the button.
    pub fn button_style(mut self, style: NativeButtonStyle) -> Self {
        self.button_style = style;
        self
    }

    /// Set a semantic tint color on the button.
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

struct NativeIconButtonState {
    native_button_ptr: *mut c_void,
    native_target_ptr: *mut c_void,
    current_symbol: SharedString,
    attached: bool,
}

impl Drop for NativeIconButtonState {
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

unsafe impl Send for NativeIconButtonState {}

// =============================================================================
// Element trait impl
// =============================================================================

impl IntoElement for NativeIconButton {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeIconButton {
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

        // Square default size for icon buttons
        if matches!(style.size.width, Length::Auto) {
            style.size.width =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(28.0))));
        }
        if matches!(style.size.height, Length::Auto) {
            style.size.height =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(28.0))));
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
            let sf_symbol = self.sf_symbol.clone();
            let button_style = self.button_style;
            let tint = self.tint;
            let tooltip = self.tooltip_label.clone();
            let disabled = self.disabled;

            let next_frame_callbacks = window.next_frame_callbacks.clone();
            let invalidator = window.invalidator.clone();

            window.with_optional_element_state::<NativeIconButtonState, _>(
                id,
                |prev_state, window| {
                    let state = if let Some(Some(mut state)) = prev_state {
                        unsafe {
                            native_controls::set_native_view_frame(
                                state.native_button_ptr as cocoa::base::id,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );
                            if state.current_symbol != sf_symbol {
                                native_controls::set_native_button_sf_symbol(
                                    state.native_button_ptr as cocoa::base::id,
                                    &sf_symbol,
                                    true,
                                );
                                state.current_symbol = sf_symbol;
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
                            // Create button with empty title
                            let button = native_controls::create_native_button("");
                            native_controls::set_native_button_sf_symbol(button, &sf_symbol, true);

                            // Apply style
                            match button_style {
                                NativeButtonStyle::Rounded => {
                                    native_controls::set_native_button_bezel_style(button, 1);
                                    native_controls::set_native_button_bordered(button, true);
                                }
                                NativeButtonStyle::Filled => {
                                    native_controls::set_native_button_bezel_style(button, 1);
                                    native_controls::set_native_button_bordered(button, true);
                                }
                                NativeButtonStyle::Inline => {
                                    native_controls::set_native_button_bezel_style(button, 15);
                                    native_controls::set_native_button_bordered(button, true);
                                }
                                NativeButtonStyle::Borderless => {
                                    native_controls::set_native_button_bordered(button, false);
                                }
                            }

                            // Apply tint
                            if let Some(tint) = tint {
                                let (r, g, b, a) = tint.rgba();
                                native_controls::set_native_button_content_tint_color(
                                    button, r, g, b, a,
                                );
                            }

                            // Tooltip
                            if let Some(ref tip) = tooltip {
                                native_controls::set_native_view_tooltip(button, tip);
                            }

                            // Disabled state
                            native_controls::set_native_control_enabled(button, !disabled);

                            native_controls::attach_native_view_to_parent(
                                button,
                                native_view as cocoa::base::id,
                            );
                            native_controls::set_native_view_frame(
                                button,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );

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

                        NativeIconButtonState {
                            native_button_ptr: button_ptr,
                            native_target_ptr: target_ptr,
                            current_symbol: sf_symbol,
                            attached: true,
                        }
                    };

                    ((), Some(state))
                },
            );
        }
    }
}

impl Styled for NativeIconButton {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
