use super::CALLBACK_IVAR;
use cocoa::{
    base::{id, nil},
    foundation::{NSPoint, NSRect, NSSize},
};
use ctor::ctor;
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Sel},
    sel, sel_impl,
};
use std::{ffi::c_void, ptr};

pub(crate) struct TrackingViewCallbacks {
    pub on_enter: Option<Box<dyn Fn()>>,
    pub on_exit: Option<Box<dyn Fn()>>,
    pub on_move: Option<Box<dyn Fn(f64, f64)>>,
}

static mut TRACKING_VIEW_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_tracking_view_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUINativeTrackingView", class!(NSView)).unwrap();
        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);
        decl.add_ivar::<i8>("_isFlipped");

        decl.add_method(
            sel!(mouseEntered:),
            tracking_mouse_entered as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(mouseExited:),
            tracking_mouse_exited as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(mouseMoved:),
            tracking_mouse_moved as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(updateTrackingAreas),
            tracking_update_areas as extern "C" fn(&Object, Sel),
        );
        decl.add_method(
            sel!(isFlipped),
            tracking_is_flipped as extern "C" fn(&Object, Sel) -> i8,
        );

        TRACKING_VIEW_CLASS = decl.register();
    }
}

extern "C" fn tracking_mouse_entered(this: &Object, _sel: Sel, _event: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if !ptr.is_null() {
            let callbacks = &*(ptr as *const TrackingViewCallbacks);
            if let Some(ref on_enter) = callbacks.on_enter {
                on_enter();
            }
        }
    }
}

extern "C" fn tracking_mouse_exited(this: &Object, _sel: Sel, _event: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if !ptr.is_null() {
            let callbacks = &*(ptr as *const TrackingViewCallbacks);
            if let Some(ref on_exit) = callbacks.on_exit {
                on_exit();
            }
        }
    }
}

extern "C" fn tracking_mouse_moved(this: &Object, _sel: Sel, event: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if !ptr.is_null() {
            let callbacks = &*(ptr as *const TrackingViewCallbacks);
            if let Some(ref on_move) = callbacks.on_move {
                let location: NSPoint = msg_send![event, locationInWindow];
                let local: NSPoint = msg_send![this, convertPoint: location fromView: nil];
                on_move(local.x, local.y);
            }
        }
    }
}

extern "C" fn tracking_update_areas(this: &Object, _sel: Sel) {
    unsafe {
        // Remove old tracking areas
        let areas: id = msg_send![this, trackingAreas];
        let count: u64 = msg_send![areas, count];
        for i in (0..count).rev() {
            let area: id = msg_send![areas, objectAtIndex: i];
            let _: () = msg_send![this, removeTrackingArea: area];
        }

        // Add fresh tracking area covering entire bounds
        let bounds: NSRect = msg_send![this, bounds];
        // MouseEnteredAndExited | MouseMoved | ActiveInActiveApp | InVisibleRect
        let options: u64 = 0x01 | 0x02 | 0x40 | 0x200;
        let area: id = msg_send![class!(NSTrackingArea), alloc];
        let area: id = msg_send![area, initWithRect: bounds options: options owner: this userInfo: nil];
        let _: () = msg_send![this, addTrackingArea: area];
        let _: () = msg_send![area, release];
    }
}

extern "C" fn tracking_is_flipped(_this: &Object, _sel: Sel) -> i8 {
    1 // YES — match GPUI's top-down coordinate system
}

pub(crate) unsafe fn create_native_tracking_view() -> id {
    unsafe {
        let view: id = msg_send![TRACKING_VIEW_CLASS, alloc];
        let view: id = msg_send![view, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(100.0, 100.0),
        )];
        let _: () = msg_send![view, setAutoresizingMask: 0u64];

        // Initialize callback pointer to null
        (*(view as *mut Object)).set_ivar::<*mut c_void>(CALLBACK_IVAR, ptr::null_mut());

        // Set up initial tracking area
        let bounds: NSRect = msg_send![view, bounds];
        let options: u64 = 0x01 | 0x02 | 0x40 | 0x200;
        let area: id = msg_send![class!(NSTrackingArea), alloc];
        let area: id = msg_send![area, initWithRect: bounds options: options owner: view userInfo: nil];
        let _: () = msg_send![view, addTrackingArea: area];
        let _: () = msg_send![area, release];

        view
    }
}

pub(crate) unsafe fn set_native_tracking_view_callbacks(
    view: id,
    callbacks: TrackingViewCallbacks,
) -> *mut c_void {
    unsafe {
        // Free previous callbacks if any
        let old_ptr: *mut c_void = *(*(view as *mut Object)).get_ivar(CALLBACK_IVAR);
        if !old_ptr.is_null() {
            let _ = Box::from_raw(old_ptr as *mut TrackingViewCallbacks);
        }

        let callbacks_ptr = Box::into_raw(Box::new(callbacks)) as *mut c_void;
        (*(view as *mut Object)).set_ivar::<*mut c_void>(CALLBACK_IVAR, callbacks_ptr);
        callbacks_ptr
    }
}

pub(crate) unsafe fn release_native_tracking_view_target(target: *mut c_void) {
    unsafe {
        if !target.is_null() {
            let _ = Box::from_raw(target as *mut TrackingViewCallbacks);
        }
    }
}

pub(crate) unsafe fn release_native_tracking_view(view: id) {
    unsafe {
        if view != nil {
            // Clean up callbacks
            let ptr: *mut c_void = *(*(view as *mut Object)).get_ivar(CALLBACK_IVAR);
            if !ptr.is_null() {
                let _ = Box::from_raw(ptr as *mut TrackingViewCallbacks);
                (*(view as *mut Object)).set_ivar::<*mut c_void>(CALLBACK_IVAR, ptr::null_mut());
            }
            let _: () = msg_send![view, removeFromSuperview];
            let _: () = msg_send![view, release];
        }
    }
}
