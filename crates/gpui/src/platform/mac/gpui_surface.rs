use super::metal_renderer::{SharedRenderResources, SurfaceRenderer};
use super::window::MacWindowState;
use crate::{
    DevicePixels, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, PlatformInput, Pixels,
    Size, px, size,
};
use cocoa::{
    base::{id, nil},
    foundation::{NSPoint, NSRect, NSSize},
};
use ctor::ctor;
use metal::CAMetalLayer;
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Sel},
    sel, sel_impl,
};
use parking_lot::Mutex;
use std::{ffi::c_void, mem, ptr, sync::Arc};

const WINDOW_STATE_IVAR: &str = "windowStatePtr";

static mut GPUI_SURFACE_VIEW_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_gpui_surface_view_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUISurfaceView", class!(NSView)).unwrap();

        decl.add_method(
            sel!(makeBackingLayer),
            make_backing_layer as extern "C" fn(&Object, Sel) -> id,
        );
        decl.add_method(
            sel!(wantsLayer),
            wants_layer as extern "C" fn(&Object, Sel) -> i8,
        );
        decl.add_method(
            sel!(isFlipped),
            is_flipped as extern "C" fn(&Object, Sel) -> i8,
        );
        decl.add_method(
            sel!(acceptsFirstResponder),
            accepts_first_responder as extern "C" fn(&Object, Sel) -> i8,
        );
        decl.add_method(
            sel!(wantsUpdateLayer),
            wants_update_layer as extern "C" fn(&Object, Sel) -> i8,
        );

        // Mouse event handlers — forward to the window's event_callback
        decl.add_method(
            sel!(mouseDown:),
            handle_surface_view_event as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(mouseUp:),
            handle_surface_view_event as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(rightMouseDown:),
            handle_surface_view_event as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(rightMouseUp:),
            handle_surface_view_event as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(otherMouseDown:),
            handle_surface_view_event as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(otherMouseUp:),
            handle_surface_view_event as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(mouseMoved:),
            handle_surface_view_event as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(mouseDragged:),
            handle_surface_view_event as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(rightMouseDragged:),
            handle_surface_view_event as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(otherMouseDragged:),
            handle_surface_view_event as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(scrollWheel:),
            handle_surface_view_event as extern "C" fn(&Object, Sel, id),
        );

        // Store the CAMetalLayer pointer so makeBackingLayer can return it
        decl.add_ivar::<*mut c_void>("metalLayerPtr");
        // Store a raw pointer to Arc<Mutex<MacWindowState>> for event forwarding
        decl.add_ivar::<*mut c_void>(WINDOW_STATE_IVAR);

        GPUI_SURFACE_VIEW_CLASS = decl.register();
    }
}

extern "C" fn make_backing_layer(this: &Object, _sel: Sel) -> id {
    unsafe {
        let layer_ptr: *mut c_void = *this.get_ivar("metalLayerPtr");
        if layer_ptr.is_null() {
            // Fallback to a normal CALayer
            msg_send![class!(CALayer), layer]
        } else {
            layer_ptr as id
        }
    }
}

extern "C" fn wants_layer(_this: &Object, _sel: Sel) -> i8 {
    1 // YES
}

extern "C" fn is_flipped(_this: &Object, _sel: Sel) -> i8 {
    1 // YES — GPUI uses top-down coordinates
}

extern "C" fn accepts_first_responder(_this: &Object, _sel: Sel) -> i8 {
    1 // YES
}

extern "C" fn wants_update_layer(_this: &Object, _sel: Sel) -> i8 {
    1 // YES — we drive rendering ourselves, not via display_layer
}

/// Handles mouse events on the surface view by converting coordinates to
/// view-local space and forwarding through the window's event_callback.
/// Since isFlipped=YES, convertPoint:fromView:nil gives top-down coords
/// that match the surface's GPUI hitbox coordinates.
extern "C" fn handle_surface_view_event(this: &Object, _sel: Sel, native_event: id) {
    let window_state: Arc<Mutex<MacWindowState>> = unsafe {
        let raw: *mut c_void = *this.get_ivar(WINDOW_STATE_IVAR);
        if raw.is_null() {
            return;
        }
        let rc1 = Arc::from_raw(raw as *mut Mutex<MacWindowState>);
        let rc2 = rc1.clone();
        mem::forget(rc1);
        rc2
    };

    let event = unsafe {
        PlatformInput::from_native(
            native_event,
            None,
            Some(this as *const _ as id),
        )
    };

    if let Some(mut event) = event {
        // Ctrl-left-click → right-click conversion (matches main window behavior)
        match &mut event {
            PlatformInput::MouseDown(
                down @ MouseDownEvent {
                    button: MouseButton::Left,
                    modifiers: Modifiers { control: true, .. },
                    ..
                },
            ) => {
                *down = MouseDownEvent {
                    button: MouseButton::Right,
                    modifiers: Modifiers {
                        control: false,
                        ..down.modifiers
                    },
                    click_count: 1,
                    ..*down
                };
            }
            PlatformInput::MouseUp(
                up @ MouseUpEvent {
                    button: MouseButton::Left,
                    modifiers: Modifiers { control: true, .. },
                    ..
                },
            ) => {
                *up = MouseUpEvent {
                    button: MouseButton::Right,
                    modifiers: Modifiers {
                        control: false,
                        ..up.modifiers
                    },
                    ..*up
                };
            }
            _ => {}
        }

        let native_view_ptr = this as *const _ as *mut c_void;
        let mut lock = window_state.as_ref().lock();
        if let Some(mut callback) = lock.surface_event_callback.take() {
            drop(lock);
            callback(native_view_ptr, event);
            window_state.lock().surface_event_callback = Some(callback);
        }
    }
}

/// A secondary GPUI rendering surface that can be embedded in any NSView container.
/// It owns a `SurfaceRenderer` (lightweight, shares GPU resources with the main renderer)
/// and a `GPUISurfaceView` (NSView backed by the surface's CAMetalLayer).
pub(crate) struct GpuiSurface {
    renderer: SurfaceRenderer,
    native_view: id, // GPUISurfaceView
    has_window_state: bool,
}

impl GpuiSurface {
    pub fn new(shared: Arc<SharedRenderResources>, transparent: bool) -> Self {
        let renderer = SurfaceRenderer::new(shared, transparent);

        let native_view = unsafe {
            let view: id = msg_send![GPUI_SURFACE_VIEW_CLASS, alloc];
            let view: id = msg_send![view, initWithFrame: NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(100.0, 100.0),
            )];

            // Store the Metal layer pointer so makeBackingLayer returns it
            let layer_ptr = renderer.layer_ptr() as *mut c_void;
            (*(view as *mut Object)).set_ivar::<*mut c_void>("metalLayerPtr", layer_ptr);

            // Initialize window state pointer to null
            (*(view as *mut Object)).set_ivar::<*mut c_void>(WINDOW_STATE_IVAR, ptr::null_mut());

            // Force the view to create its layer now
            let _: () = msg_send![view, setWantsLayer: 1i8];

            view
        };

        Self {
            renderer,
            native_view,
            has_window_state: false,
        }
    }

    /// Returns a raw pointer to the GPUISurfaceView for placing in a container NSView.
    pub fn native_view_ptr(&self) -> *mut c_void {
        self.native_view as *mut c_void
    }

    /// Returns the CAMetalLayer pointer.
    pub fn layer_ptr(&self) -> *mut CAMetalLayer {
        self.renderer.layer_ptr()
    }

    /// Draws the given scene to the surface's Metal layer.
    pub fn draw(&mut self, scene: &crate::Scene) {
        self.renderer.draw(scene);
    }

    /// Updates the drawable size (in device pixels) of the surface.
    pub fn update_drawable_size(&mut self, size: Size<DevicePixels>) {
        self.renderer.update_drawable_size(size);
    }

    /// Updates the transparency of the surface's Metal layer.
    pub fn update_transparency(&self, transparent: bool) {
        self.renderer.update_transparency(transparent);
    }

    /// Returns the content size of the surface view in logical pixels.
    pub fn content_size(&self) -> Size<Pixels> {
        unsafe {
            let frame: NSRect = msg_send![self.native_view, frame];
            size(px(frame.size.width as f32), px(frame.size.height as f32))
        }
    }

    /// Returns the scale factor from the view's backing properties.
    pub fn scale_factor(&self) -> f32 {
        unsafe {
            let window: id = msg_send![self.native_view, window];
            if window != nil {
                let factor: f64 = msg_send![window, backingScaleFactor];
                factor as f32
            } else {
                let screen: id = msg_send![self.native_view, screen];
                if screen != nil {
                    let factor: f64 = msg_send![screen, backingScaleFactor];
                    factor as f32
                } else {
                    2.0 // Default retina
                }
            }
        }
    }

    /// Sets the contentsScale on the Metal layer to match the display's scale factor.
    pub fn set_contents_scale(&self, scale: f64) {
        unsafe {
            let layer: id = msg_send![self.native_view, layer];
            if layer != nil {
                let _: () = msg_send![layer, setContentsScale: scale];
            }
        }
    }

    /// Attach the window's state to the surface view so mouse events can be
    /// forwarded through the window's event_callback. The raw pointer is an
    /// `Arc::into_raw(Arc<Mutex<MacWindowState>>)` — we take ownership of one
    /// Arc reference and release it on drop.
    pub fn set_window_state(&mut self, raw_state_ptr: *const c_void) {
        unsafe {
            // Clean up any previously set window state
            if self.has_window_state {
                let prev: *mut c_void = *(*self.native_view).get_ivar(WINDOW_STATE_IVAR);
                if !prev.is_null() {
                    let _drop = Arc::from_raw(prev as *mut Mutex<MacWindowState>);
                }
            }
            (*(self.native_view as *mut Object))
                .set_ivar::<*mut c_void>(WINDOW_STATE_IVAR, raw_state_ptr as *mut c_void);
            self.has_window_state = !raw_state_ptr.is_null();
        }
    }
}

impl Drop for GpuiSurface {
    fn drop(&mut self) {
        unsafe {
            if self.native_view != nil {
                // Release the window state Arc reference if we hold one
                if self.has_window_state {
                    let raw: *mut c_void = *(*self.native_view).get_ivar(WINDOW_STATE_IVAR);
                    if !raw.is_null() {
                        let _drop = Arc::from_raw(raw as *mut Mutex<MacWindowState>);
                    }
                }
                let _: () = msg_send![self.native_view, removeFromSuperview];
                let _: () = msg_send![self.native_view, release];
            }
        }
    }
}
