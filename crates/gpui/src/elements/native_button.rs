use refineable::Refineable as _;
use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;

use crate::{
    AbsoluteLength, App, Bounds, ClickEvent, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, SharedString, Style, StyleRefinement,
    Styled, Window, px,
};

type FrameCallback = Box<dyn FnOnce(&mut Window, &mut App)>;

/// Creates a native platform button element (NSButton on macOS).
///
/// The button participates in GPUI's Taffy layout system and renders as a real
/// platform button, not a custom-drawn element.
pub fn native_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
) -> NativeButton {
    NativeButton {
        id: id.into(),
        label: label.into(),
        on_click: None,
        style: StyleRefinement::default(),
    }
}

/// A native platform button element that creates a real OS button (NSButton on macOS)
/// as a subview of the window's native view, positioned by GPUI's Taffy layout engine.
pub struct NativeButton {
    id: ElementId,
    label: SharedString,
    on_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    style: StyleRefinement,
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
}

/// State persisted across frames via GlobalElementId.
struct NativeButtonElementState {
    native_button_ptr: *mut c_void,
    native_target_ptr: *mut c_void,
    current_label: SharedString,
    attached: bool,
}

impl Drop for NativeButtonElementState {
    fn drop(&mut self) {
        if self.attached {
            #[cfg(target_os = "macos")]
            unsafe {
                use crate::platform::native_controls;
                native_controls::remove_native_button_from_view(
                    self.native_button_ptr as cocoa::base::id,
                );
                native_controls::release_native_button_target(self.native_target_ptr);
                native_controls::release_native_button(
                    self.native_button_ptr as cocoa::base::id,
                );
            }
        }
    }
}

// Safety: NativeButtonElementState holds ObjC pointers that are only accessed on the main thread.
unsafe impl Send for NativeButtonElementState {}

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

        // Set reasonable default size for a button if not specified
        if matches!(style.size.width, Length::Auto) {
            let char_width = 8.0;
            let padding = 24.0;
            let width = (self.label.len() as f32 * char_width + padding).max(80.0);
            style.size.width = Length::Definite(DefiniteLength::Absolute(
                AbsoluteLength::Pixels(px(width)),
            ));
        }
        if matches!(style.size.height, Length::Auto) {
            style.size.height = Length::Definite(DefiniteLength::Absolute(
                AbsoluteLength::Pixels(px(24.0)),
            ));
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

            // Clone the next_frame_callbacks and invalidator so the AppKit action
            // can schedule a GPUI callback on the next frame.
            let next_frame_callbacks = window.next_frame_callbacks.clone();
            let invalidator = window.invalidator.clone();

            window.with_optional_element_state::<NativeButtonElementState, _>(
                id,
                |prev_state, window| {
                    let state = if let Some(Some(mut state)) = prev_state {
                        // Reuse existing NSButton - update frame and label if needed
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
                        }

                        // Update click callback
                        if let Some(on_click) = on_click {
                            unsafe {
                                native_controls::release_native_button_target(
                                    state.native_target_ptr,
                                );
                            }
                            let nfc = next_frame_callbacks.clone();
                            let inv = invalidator.clone();
                            let on_click = Rc::new(on_click);
                            let callback = make_native_callback(on_click, nfc, inv);
                            unsafe {
                                state.native_target_ptr =
                                    native_controls::set_native_button_action(
                                        state.native_button_ptr as cocoa::base::id,
                                        callback,
                                    );
                            }
                        }

                        state
                    } else {
                        // First frame - create NSButton
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

                            let target = if let Some(on_click) = on_click {
                                let nfc = next_frame_callbacks.clone();
                                let inv = invalidator.clone();
                                let on_click = Rc::new(on_click);
                                let callback = make_native_callback(on_click, nfc, inv);
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
                            attached: true,
                        }
                    };

                    ((), Some(state))
                },
            );
        }
    }
}

/// Creates a callback for the native button action that bridges into GPUI's frame callback system.
#[cfg(target_os = "macos")]
fn make_native_callback(
    on_click: Rc<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    next_frame_callbacks: Rc<RefCell<Vec<FrameCallback>>>,
    invalidator: crate::WindowInvalidator,
) -> Box<dyn Fn()> {
    Box::new(move || {
        let on_click = on_click.clone();
        let callback: FrameCallback = Box::new(move |window, cx| {
            let event = ClickEvent::default();
            on_click(&event, window, cx);
        });
        RefCell::borrow_mut(&next_frame_callbacks).push(callback);
        invalidator.set_dirty(true);
    })
}

impl Styled for NativeButton {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
