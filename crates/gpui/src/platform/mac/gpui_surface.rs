use super::metal_renderer::{SharedRenderResources, SurfaceRenderer};
use crate::{DevicePixels, Pixels, Size, px, size};
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
use std::{ffi::c_void, ptr, sync::Arc};

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

        // Store the CAMetalLayer pointer so makeBackingLayer can return it
        decl.add_ivar::<*mut c_void>("metalLayerPtr");

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

/// A secondary GPUI rendering surface that can be embedded in any NSView container.
/// It owns a `SurfaceRenderer` (lightweight, shares GPU resources with the main renderer)
/// and a `GPUISurfaceView` (NSView backed by the surface's CAMetalLayer).
pub(crate) struct GpuiSurface {
    renderer: SurfaceRenderer,
    native_view: id, // GPUISurfaceView
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

            // Force the view to create its layer now
            let _: () = msg_send![view, setWantsLayer: 1i8];

            view
        };

        Self {
            renderer,
            native_view,
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
                // Fallback: try the screen
                let screen: id = msg_send![self.native_view, window];
                if screen != nil {
                    let factor: f64 = msg_send![screen, backingScaleFactor];
                    factor as f32
                } else {
                    2.0 // Default retina
                }
            }
        }
    }
}

impl Drop for GpuiSurface {
    fn drop(&mut self) {
        unsafe {
            if self.native_view != nil {
                let _: () = msg_send![self.native_view, removeFromSuperview];
                let _: () = msg_send![self.native_view, release];
            }
        }
    }
}
