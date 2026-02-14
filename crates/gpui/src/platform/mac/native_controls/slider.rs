use super::CALLBACK_IVAR;
use cocoa::{
    base::id,
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

// =============================================================================
// NSSlider target (fires Fn(f64) with slider value)
// =============================================================================

static mut SLIDER_TARGET_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_slider_target_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUINativeSliderTarget", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);

        decl.add_method(
            sel!(sliderAction:),
            slider_action as extern "C" fn(&Object, Sel, id),
        );

        SLIDER_TARGET_CLASS = decl.register();
    }
}

extern "C" fn slider_action(this: &Object, _sel: Sel, sender: id) {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if !ptr.is_null() {
            let value: f64 = msg_send![sender, doubleValue];
            let callback = &*(ptr as *const Box<dyn Fn(f64)>);
            callback(value);
        }
    }
}

// =============================================================================
// NSSlider — creation & lifecycle
// =============================================================================

/// Creates a new NSSlider with minimum, maximum, and current value.
pub(crate) unsafe fn create_native_slider(min: f64, max: f64, value: f64) -> id {
    unsafe {
        let slider: id = msg_send![class!(NSSlider), alloc];
        let slider: id = msg_send![slider, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(200.0, 22.0),
        )];
        let _: () = msg_send![slider, setAutoresizingMask: 0u64];
        let _: () = msg_send![slider, setMinValue: min];
        let _: () = msg_send![slider, setMaxValue: max];
        let _: () = msg_send![slider, setDoubleValue: value];
        let _: () = msg_send![slider, setContinuous: 1i8];
        slider
    }
}

/// Sets the slider value.
pub(crate) unsafe fn set_native_slider_value(slider: id, value: f64) {
    unsafe {
        let _: () = msg_send![slider, setDoubleValue: value];
    }
}

/// Sets the slider minimum value.
pub(crate) unsafe fn set_native_slider_min(slider: id, min: f64) {
    unsafe {
        let _: () = msg_send![slider, setMinValue: min];
    }
}

/// Sets the slider maximum value.
pub(crate) unsafe fn set_native_slider_max(slider: id, max: f64) {
    unsafe {
        let _: () = msg_send![slider, setMaxValue: max];
    }
}

/// Sets whether slider callbacks fire continuously while dragging.
pub(crate) unsafe fn set_native_slider_continuous(slider: id, continuous: bool) {
    unsafe {
        let _: () = msg_send![slider, setContinuous: continuous as i8];
    }
}

/// Sets slider tick marks and whether values snap to them.
pub(crate) unsafe fn set_native_slider_tick_marks(slider: id, count: i64, snap: bool) {
    unsafe {
        let _: () = msg_send![slider, setNumberOfTickMarks: count];
        let _: () = msg_send![slider, setAllowsTickMarkValuesOnly: snap as i8];
    }
}

/// Sets target/action callback for a slider.
/// Returns a pointer to the target object.
pub(crate) unsafe fn set_native_slider_action(
    slider: id,
    callback: Box<dyn Fn(f64)>,
) -> *mut c_void {
    unsafe {
        let target: id = msg_send![SLIDER_TARGET_CLASS, alloc];
        let target: id = msg_send![target, init];

        let callback_ptr = Box::into_raw(Box::new(callback)) as *mut c_void;
        (*target).set_ivar::<*mut c_void>(CALLBACK_IVAR, callback_ptr);

        let _: () = msg_send![slider, setTarget: target];
        let _: () = msg_send![slider, setAction: sel!(sliderAction:)];

        target as *mut c_void
    }
}

/// Releases the slider target and stored callback.
pub(crate) unsafe fn release_native_slider_target(target: *mut c_void) {
    unsafe {
        if !target.is_null() {
            let target = target as id;
            let callback_ptr: *mut c_void = *(*target).get_ivar(CALLBACK_IVAR);
            if !callback_ptr.is_null() {
                let _ = Box::from_raw(callback_ptr as *mut Box<dyn Fn(f64)>);
            }
            let _: () = msg_send![target, release];
        }
    }
}

/// Releases a slider control.
pub(crate) unsafe fn release_native_slider(slider: id) {
    unsafe {
        let _: () = msg_send![slider, release];
    }
}
