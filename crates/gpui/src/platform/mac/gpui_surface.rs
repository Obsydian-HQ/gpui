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
    runtime::{Class, Object, Sel, BOOL},
    sel, sel_impl,
};
use parking_lot::Mutex;
use std::{ffi::c_void, mem, ptr, sync::Arc};

const WINDOW_STATE_IVAR: &str = "windowStatePtr";

// NSTrackingAreaOptions
const NS_TRACKING_MOUSE_ENTERED_AND_EXITED: u64 = 0x01;
const NS_TRACKING_MOUSE_MOVED: u64 = 0x02;
const NS_TRACKING_ACTIVE_ALWAYS: u64 = 0x80;
const NS_TRACKING_IN_VISIBLE_RECT: u64 = 0x200;

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

        // Mouse event handlers — forward to the window's surface_event_callback
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
            sel!(mouseExited:),
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

        // Tracking area for hover (mouseMoved) delivery
        decl.add_method(
            sel!(updateTrackingAreas),
            update_tracking_areas as extern "C" fn(&Object, Sel),
        );

        // Keyboard event handlers — forward to the main window view which has
        // the full NSTextInputClient / IME infrastructure.
        decl.add_method(
            sel!(keyDown:),
            handle_surface_key_down as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(keyUp:),
            handle_surface_key_up as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(flagsChanged:),
            handle_surface_flags_changed as extern "C" fn(&Object, Sel, id),
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

/// Reconstructs an Arc<Mutex<MacWindowState>> from the view's ivar without
/// consuming the reference. Returns None if the ivar is null.
fn get_window_state(view: &Object) -> Option<Arc<Mutex<MacWindowState>>> {
    unsafe {
        let raw: *mut c_void = *view.get_ivar(WINDOW_STATE_IVAR);
        if raw.is_null() {
            log::warn!("[GPUISurfaceView] window state ivar is null");
            return None;
        }
        let rc = Arc::from_raw(raw as *mut Mutex<MacWindowState>);
        let clone = rc.clone();
        mem::forget(rc);
        Some(clone)
    }
}

/// Returns the main window's native view pointer from the window state.
fn get_main_native_view(window_state: &Arc<Mutex<MacWindowState>>) -> id {
    let lock = window_state.lock();
    lock.native_view.as_ptr() as id
}

/// Transfers first responder from the surface view to the main window view.
/// This ensures keyboard events (keyDown, IME, etc.) are handled by the main
/// view which has the full NSTextInputClient infrastructure.
fn transfer_first_responder_to_main_view(surface_view: &Object, window_state: &Arc<Mutex<MacWindowState>>) {
    let main_view = get_main_native_view(window_state);
    unsafe {
        let window: id = msg_send![surface_view, window];
        if window == nil {
            log::warn!("[GPUISurfaceView] transfer_first_responder: surface view has no window");
            return;
        }
        let current_responder: id = msg_send![window, firstResponder];
        let success: BOOL = msg_send![window, makeFirstResponder: main_view];
        let new_responder: id = msg_send![window, firstResponder];
        log::info!(
            "[GPUISurfaceView] transfer_first_responder: success={}, prev_responder={:?}, new_responder={:?}, main_view={:?}",
            success,
            current_responder,
            new_responder,
            main_view,
        );
    }
}

/// Installs an NSTrackingArea so macOS delivers mouseMoved: and
/// mouseExited: events to this view (required for hover effects).
extern "C" fn update_tracking_areas(this: &Object, _sel: Sel) {
    unsafe {
        // Call super to preserve default behavior
        let superclass = class!(NSView);
        let _: () = msg_send![super(this, superclass), updateTrackingAreas];

        // Remove existing tracking areas
        let areas: id = msg_send![this, trackingAreas];
        let count: u64 = msg_send![areas, count];
        for i in (0..count).rev() {
            let area: id = msg_send![areas, objectAtIndex: i];
            let _: () = msg_send![this, removeTrackingArea: area];
        }

        // Add a new tracking area covering the visible rect
        let options: u64 = NS_TRACKING_MOUSE_ENTERED_AND_EXITED
            | NS_TRACKING_MOUSE_MOVED
            | NS_TRACKING_ACTIVE_ALWAYS
            | NS_TRACKING_IN_VISIBLE_RECT;
        let tracking_area: id = msg_send![class!(NSTrackingArea), alloc];
        let tracking_area: id = msg_send![
            tracking_area,
            initWithRect: NSRect::new(NSPoint::new(0., 0.), NSSize::new(0., 0.))
            options: options
            owner: this
            userInfo: nil
        ];
        let _: () = msg_send![this, addTrackingArea: tracking_area];
        let _: () = msg_send![tracking_area, release];

        let bounds: NSRect = msg_send![this, bounds];
        log::info!(
            "[GPUISurfaceView] updateTrackingAreas: installed tracking area, bounds=({}, {}, {}, {}), removed {} old areas",
            bounds.origin.x, bounds.origin.y, bounds.size.width, bounds.size.height,
            count,
        );
    }
}

/// Handles mouse events on the surface view by converting coordinates to
/// view-local space and forwarding through the window's surface_event_callback.
/// Since isFlipped=YES, convertPoint:fromView:nil gives top-down coords
/// that match the surface's GPUI hitbox coordinates.
extern "C" fn handle_surface_view_event(this: &Object, _sel: Sel, native_event: id) {
    let Some(window_state) = get_window_state(this) else {
        return;
    };

    let event = unsafe {
        PlatformInput::from_native(
            native_event,
            None,
            Some(this as *const _ as id),
        )
    };

    if let Some(mut event) = event {
        let event_name = match &event {
            PlatformInput::MouseDown(_) => "MouseDown",
            PlatformInput::MouseUp(_) => "MouseUp",
            PlatformInput::MouseMove(_) => "MouseMove",
            PlatformInput::MouseExited(_) => "MouseExited",
            PlatformInput::ScrollWheel(_) => "ScrollWheel",
            PlatformInput::KeyDown(_) => "KeyDown",
            PlatformInput::KeyUp(_) => "KeyUp",
            PlatformInput::ModifiersChanged(_) => "ModifiersChanged",
            _ => "Other",
        };
        log::info!("[GPUISurfaceView] handle_surface_view_event: {}", event_name);

        let is_mouse_down = matches!(&event, PlatformInput::MouseDown(_));

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
        let mut lock = window_state.lock();
        if let Some(mut callback) = lock.surface_event_callback.take() {
            drop(lock);
            callback(native_view_ptr, event);
            window_state.lock().surface_event_callback = Some(callback);
        } else {
            drop(lock);
            log::warn!("[GPUISurfaceView] no surface_event_callback registered for event: {}", event_name);
        }

        // After a mouseDown, transfer first responder to the main window view
        // so that subsequent keyboard events are handled by the main view's
        // NSTextInputClient/IME infrastructure.
        if is_mouse_down {
            transfer_first_responder_to_main_view(this, &window_state);
        }
    }
}

/// Forwards keyDown: to the main window view after ensuring it is first responder.
extern "C" fn handle_surface_key_down(this: &Object, _sel: Sel, native_event: id) {
    log::info!("[GPUISurfaceView] handle_surface_key_down: forwarding to main view");
    let Some(window_state) = get_window_state(this) else {
        return;
    };
    transfer_first_responder_to_main_view(this, &window_state);
    let main_view = get_main_native_view(&window_state);
    unsafe {
        let _: () = msg_send![main_view, keyDown: native_event];
    }
}

/// Forwards keyUp: to the main window view.
extern "C" fn handle_surface_key_up(this: &Object, _sel: Sel, native_event: id) {
    log::info!("[GPUISurfaceView] handle_surface_key_up: forwarding to main view");
    let Some(window_state) = get_window_state(this) else {
        return;
    };
    let main_view = get_main_native_view(&window_state);
    unsafe {
        let _: () = msg_send![main_view, keyUp: native_event];
    }
}

/// Forwards flagsChanged: to the main window view (modifier key tracking).
extern "C" fn handle_surface_flags_changed(this: &Object, _sel: Sel, native_event: id) {
    log::info!("[GPUISurfaceView] handle_surface_flags_changed: forwarding to main view");
    let Some(window_state) = get_window_state(this) else {
        return;
    };
    let main_view = get_main_native_view(&window_state);
    unsafe {
        let _: () = msg_send![main_view, flagsChanged: native_event];
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

    /// Attach the window's state to the surface view so events can be
    /// forwarded through the window's callbacks. The raw pointer is an
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
