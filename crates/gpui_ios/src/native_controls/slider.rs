use super::{id, ns_string, CALLBACK_IVAR, UI_CONTROL_EVENT_VALUE_CHANGED};
use ctor::ctor;
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Sel},
    sel, sel_impl,
};
use std::{ffi::c_void, ptr};

static mut SLIDER_TARGET_CLASS: *const Class = ptr::null();

#[ctor]
unsafe fn build_slider_target_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUIiOSNativeSliderTarget", class!(NSObject)).unwrap();
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
            let value: f32 = msg_send![sender, value];
            let callback = &*(ptr as *const Box<dyn Fn(f64)>);
            callback(value as f64);
        }
    }
}

/// Creates a new UISlider.
pub(crate) unsafe fn create_native_slider(min: f64, max: f64, value: f64) -> id {
    unsafe {
        let slider: id = msg_send![class!(UISlider), alloc];
        let slider: id = msg_send![slider, init];
        let _: () = msg_send![slider, setMinimumValue: min as f32];
        let _: () = msg_send![slider, setMaximumValue: max as f32];
        let _: () = msg_send![slider, setValue: value as f32 animated: false as i8];

        // UIAccessibility — UISlider already has UIAccessibilityTraitAdjustable by default
        let _: () = msg_send![slider, setAccessibilityLabel: ns_string("Slider")];

        slider
    }
}

/// Sets the slider value.
pub(crate) unsafe fn set_native_slider_value(slider: id, value: f64) {
    unsafe {
        let _: () = msg_send![slider, setValue: value as f32 animated: false as i8];
        let value_str = format!("{:.1}", value);
        let _: () = msg_send![slider, setAccessibilityValue: ns_string(&value_str)];
    }
}

/// Sets the slider minimum.
pub(crate) unsafe fn set_native_slider_min(slider: id, min: f64) {
    unsafe {
        let _: () = msg_send![slider, setMinimumValue: min as f32];
    }
}

/// Sets the slider maximum.
pub(crate) unsafe fn set_native_slider_max(slider: id, max: f64) {
    unsafe {
        let _: () = msg_send![slider, setMaximumValue: max as f32];
    }
}

/// Sets whether the slider sends actions continuously.
pub(crate) unsafe fn set_native_slider_continuous(slider: id, continuous: bool) {
    unsafe {
        let _: () = msg_send![slider, setContinuous: continuous as i8];
    }
}

/// No-op on iOS — UISlider doesn't support tick marks.
pub(crate) unsafe fn set_native_slider_tick_marks(_slider: id, _count: i64, _snap: bool) {}

/// Sets the slider's target/action callback.
pub(crate) unsafe fn set_native_slider_action(
    slider: id,
    callback: Box<dyn Fn(f64)>,
) -> *mut c_void {
    unsafe {
        let target: id = msg_send![SLIDER_TARGET_CLASS, alloc];
        let target: id = msg_send![target, init];

        let callback_ptr = Box::into_raw(Box::new(callback)) as *mut c_void;
        (*target).set_ivar::<*mut c_void>(CALLBACK_IVAR, callback_ptr);

        let _: () = msg_send![slider,
            addTarget: target
            action: sel!(sliderAction:)
            forControlEvents: UI_CONTROL_EVENT_VALUE_CHANGED
        ];

        target as *mut c_void
    }
}

/// Releases the slider target.
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

/// Releases a UISlider.
pub(crate) unsafe fn release_native_slider(slider: id) {
    unsafe {
        if !slider.is_null() {
            let _: () = msg_send![slider, release];
        }
    }
}
