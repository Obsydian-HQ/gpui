#[cfg(feature = "font-kit")]
mod open_type;
#[cfg(feature = "font-kit")]
mod text_system;

use crate::{
    Action, AnyWindowHandle, BackgroundExecutor, Bounds, ClipboardItem, CursorStyle,
    DispatchEventResult, DisplayId, DummyKeyboardMapper, ForegroundExecutor, GLOBAL_THREAD_TIMINGS,
    GpuSpecs, KeyDownEvent, KeyUpEvent, Keymap, Keystroke, Menu, MenuItem, Modifiers,
    ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    NoopTextSystem, OwnedMenu, PathPromptOptions, Pixels, PinchEvent, Platform, PlatformAtlas,
    PlatformDispatcher, PlatformDisplay, PlatformInput, PlatformInputHandler,
    PlatformKeyboardLayout, PlatformKeyboardMapper, PlatformTextSystem, PlatformWindow, Point,
    Priority, PromptButton, RequestFrameOptions, RotationEvent, ScrollDelta, ScrollWheelEvent,
    Task, TaskTiming, ThermalState, THREAD_TIMINGS, ThreadTaskTimings, TouchPhase,
    WindowAppearance, WindowBackgroundAppearance, WindowBounds, WindowControlArea, WindowParams,
    point, px, size,
};
use crate::platform::metal::renderer::{InstanceBufferPool, MetalRenderer, SharedRenderResources};
use foreign_types::ForeignType as _;
use anyhow::{Result, anyhow};
use ctor::ctor;
use futures::channel::oneshot;
use objc::{
    class, msg_send,
    declare::ClassDecl,
    runtime::{Class, Object, Sel, BOOL, NO, YES},
    sel, sel_impl,
};
use parking_lot::Mutex;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, UiKitWindowHandle, WindowHandle,
};
use core_foundation::{
    base::{CFType, CFTypeRef, OSStatus, TCFType},
    boolean::CFBoolean,
    data::CFData,
    dictionary::{CFDictionary, CFDictionaryRef, CFMutableDictionary},
    string::{CFString, CFStringRef},
};
use std::{
    cell::Cell,
    ffi::c_void,
    path::{Path, PathBuf},
    ptr::{self, NonNull, addr_of},
    rc::Rc,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
#[cfg(feature = "font-kit")]
use text_system::IosTextSystem;

pub(crate) type PlatformScreenCaptureFrame = ();

type DispatchQueue = *mut c_void;
type DispatchTime = u64;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct NSRange {
    location: usize,
    length: usize,
}

unsafe impl objc::Encode for NSRange {
    fn encode() -> objc::Encoding {
        let encoding = format!(
            "{{NSRange={}{}}}",
            usize::encode().as_str(),
            usize::encode().as_str()
        );
        unsafe { objc::Encoding::from_str(&encoding) }
    }
}

const DISPATCH_TIME_NOW: DispatchTime = 0;
const DISPATCH_QUEUE_PRIORITY_HIGH: isize = 2;
const DISPATCH_QUEUE_PRIORITY_DEFAULT: isize = 0;
const DISPATCH_QUEUE_PRIORITY_LOW: isize = -2;

const CALLBACK_IVAR: &str = "gpui_callback";
const WINDOW_STATE_IVAR: &str = "gpui_window_state";

const UISCENE_DID_ACTIVATE: &[u8] = b"UISceneDidActivateNotification\0";
const UISCENE_WILL_DEACTIVATE: &[u8] = b"UISceneWillDeactivateNotification\0";
const UISCENE_DID_ENTER_BACKGROUND: &[u8] = b"UISceneDidEnterBackgroundNotification\0";
const UISCENE_WILL_ENTER_FOREGROUND: &[u8] = b"UISceneWillEnterForegroundNotification\0";

unsafe extern "C" {
    static _dispatch_main_q: c_void;
    static NSRunLoopCommonModes: *mut Object;
    fn dispatch_get_global_queue(identifier: isize, flags: usize) -> DispatchQueue;
    fn dispatch_async_f(
        queue: DispatchQueue,
        context: *mut c_void,
        work: Option<unsafe extern "C" fn(*mut c_void)>,
    );
    fn dispatch_after_f(
        when: DispatchTime,
        queue: DispatchQueue,
        context: *mut c_void,
        work: Option<unsafe extern "C" fn(*mut c_void)>,
    );
    fn dispatch_time(when: DispatchTime, delta: i64) -> DispatchTime;
}

// ---------------------------------------------------------------------------
// CADisplayLink target — an ObjC class whose `step:` method drives the frame
// loop on iOS, equivalent to CVDisplayLink on macOS.
// ---------------------------------------------------------------------------

static mut DISPLAY_LINK_TARGET_CLASS: *const Class = std::ptr::null();

#[ctor]
unsafe fn register_display_link_target_class() {
    let superclass = class!(NSObject);
    let mut decl = ClassDecl::new("GPUIDisplayLinkTarget", superclass)
        .expect("failed to declare GPUIDisplayLinkTarget class");
    decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);
    decl.add_method(
        sel!(step:),
        display_link_step as extern "C" fn(&Object, Sel, *mut Object),
    );
    unsafe {
        DISPLAY_LINK_TARGET_CLASS = decl.register();
    }
}

extern "C" fn display_link_step(this: &Object, _sel: Sel, _display_link: *mut Object) {
    unsafe {
        let callback_ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if !callback_ptr.is_null() {
            let callback = &*(callback_ptr as *const Box<dyn Fn()>);
            callback();
        }
    }
}

// ---------------------------------------------------------------------------
// GPUIView — custom UIView subclass for touch input, Metal layer, and
// lifecycle callbacks (resize, appearance change).
// ---------------------------------------------------------------------------

static mut GPUI_VIEW_CLASS: *const Class = std::ptr::null();

#[ctor]
unsafe fn register_gpui_view_class() {
    let superclass = class!(UIView);
    let mut decl = ClassDecl::new("GPUIView", superclass)
        .expect("failed to declare GPUIView class");

    // Ivar to hold a raw pointer to Rc<Mutex<IosWindowState>>
    decl.add_ivar::<*mut c_void>(WINDOW_STATE_IVAR);

    // Touch input
    decl.add_method(
        sel!(touchesBegan:withEvent:),
        handle_touches_began as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
    );
    decl.add_method(
        sel!(touchesMoved:withEvent:),
        handle_touches_moved as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
    );
    decl.add_method(
        sel!(touchesEnded:withEvent:),
        handle_touches_ended as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
    );
    decl.add_method(
        sel!(touchesCancelled:withEvent:),
        handle_touches_cancelled as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
    );

    // Layout (resize, rotation, split view)
    decl.add_method(
        sel!(layoutSubviews),
        handle_layout_subviews as extern "C" fn(&Object, Sel),
    );

    // Safe area insets change
    decl.add_method(
        sel!(safeAreaInsetsDidChange),
        handle_safe_area_insets_change as extern "C" fn(&Object, Sel),
    );

    // Appearance (dark/light mode change)
    decl.add_method(
        sel!(traitCollectionDidChange:),
        handle_trait_collection_change as extern "C" fn(&Object, Sel, *mut Object),
    );

    // Two-finger scroll pan gesture
    decl.add_method(
        sel!(handleScrollPan:),
        handle_scroll_pan as extern "C" fn(&Object, Sel, *mut Object),
    );

    // Single-finger scroll pan gesture
    decl.add_method(
        sel!(handleSingleFingerPan:),
        handle_single_finger_pan as extern "C" fn(&Object, Sel, *mut Object),
    );

    // Pinch gesture
    decl.add_method(
        sel!(handlePinch:),
        handle_pinch as extern "C" fn(&Object, Sel, *mut Object),
    );

    // Rotation gesture
    decl.add_method(
        sel!(handleRotation:),
        handle_rotation as extern "C" fn(&Object, Sel, *mut Object),
    );

    // UITextFieldDelegate — intercept text from the hidden UITextField keyboard proxy
    decl.add_method(
        sel!(textField:shouldChangeCharactersInRange:replacementString:),
        handle_text_field_change as extern "C" fn(&Object, Sel, *mut Object, NSRange, *mut Object) -> BOOL,
    );
    decl.add_method(
        sel!(textFieldShouldReturn:),
        handle_text_field_return as extern "C" fn(&Object, Sel, *mut Object) -> BOOL,
    );

    // UIKeyInput — GPUIView is the first responder for keyboard input
    decl.add_method(
        sel!(canBecomeFirstResponder),
        can_become_first_responder as extern "C" fn(&Object, Sel) -> BOOL,
    );
    decl.add_method(
        sel!(hasText),
        has_text as extern "C" fn(&Object, Sel) -> BOOL,
    );
    decl.add_method(
        sel!(insertText:),
        insert_text as extern "C" fn(&Object, Sel, *mut Object),
    );
    decl.add_method(
        sel!(deleteBackward),
        delete_backward as extern "C" fn(&Object, Sel),
    );

    // UITextInputTraits
    decl.add_method(
        sel!(keyboardType),
        keyboard_type as extern "C" fn(&Object, Sel) -> isize,
    );
    decl.add_method(
        sel!(autocorrectionType),
        autocorrection_type as extern "C" fn(&Object, Sel) -> isize,
    );
    decl.add_method(
        sel!(autocapitalizationType),
        autocapitalization_type as extern "C" fn(&Object, Sel) -> isize,
    );
    decl.add_method(
        sel!(spellCheckingType),
        spell_checking_type as extern "C" fn(&Object, Sel) -> isize,
    );

    // Hardware keyboard via UIPresses
    decl.add_method(
        sel!(pressesBegan:withEvent:),
        handle_presses_began as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
    );
    decl.add_method(
        sel!(pressesEnded:withEvent:),
        handle_presses_ended as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
    );
    decl.add_method(
        sel!(pressesCancelled:withEvent:),
        handle_presses_cancelled as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
    );

    // Make CAMetalLayer the view's own backing layer
    decl.add_class_method(
        sel!(layerClass),
        gpui_view_layer_class as extern "C" fn(&Class, Sel) -> *const Class,
    );

    unsafe {
        GPUI_VIEW_CLASS = decl.register();
    }
}

extern "C" fn gpui_view_layer_class(_self: &Class, _sel: Sel) -> *const Class {
    class!(CAMetalLayer)
}

/// Recover the `Rc<Mutex<IosWindowState>>` from the view's ivar without
/// consuming the Rc (the ivar still holds its reference).
unsafe fn get_window_state(view: &Object) -> Option<Rc<Mutex<IosWindowState>>> {
    let ptr: *mut c_void = *view.get_ivar(WINDOW_STATE_IVAR);
    if ptr.is_null() {
        return None;
    }
    let rc = Rc::from_raw(ptr as *const Mutex<IosWindowState>);
    let clone = rc.clone();
    std::mem::forget(rc); // Don't drop — ivar still holds it
    Some(clone)
}

/// Extract the primary touch position from a UITouch set relative to the view.
/// Returns `(position, tap_count)`.
unsafe fn primary_touch_info(
    touches: *mut Object,
    view: &Object,
    state: &Mutex<IosWindowState>,
) -> Option<(Point<Pixels>, usize)> {
    let all_objects: *mut Object = msg_send![touches, allObjects];
    let count: usize = msg_send![all_objects, count];
    if count == 0 {
        return None;
    }

    let mut lock = state.lock();

    // Find the tracked touch, or pick the first one if we're not tracking yet
    let touch = if let Some(tracked) = lock.tracked_touch {
        let mut found: *mut Object = std::ptr::null_mut();
        for i in 0..count {
            let t: *mut Object = msg_send![all_objects, objectAtIndex: i];
            if t == tracked {
                found = t;
                break;
            }
        }
        if found.is_null() {
            return None;
        }
        found
    } else {
        let touch: *mut Object = msg_send![all_objects, objectAtIndex: 0usize];
        lock.tracked_touch = Some(touch);
        touch
    };

    let location: CGPoint = msg_send![touch, locationInView: view as *const Object as *mut Object];
    let tap_count: usize = msg_send![touch, tapCount];
    let position = point(px(location.x as f32), px(location.y as f32));

    // Update last known mouse position
    lock.last_touch_position = Some(position);

    Some((position, tap_count))
}

fn dispatch_input(
    state: &Mutex<IosWindowState>,
    input: PlatformInput,
) -> crate::DispatchEventResult {
    let mut lock = state.lock();
    if let Some(mut callback) = lock.on_input.take() {
        drop(lock);
        let result = callback(input);
        state.lock().on_input = Some(callback);
        result
    } else {
        crate::DispatchEventResult {
            propagate: true,
            default_prevented: false,
        }
    }
}

unsafe extern "C" fn become_first_responder_trampoline(context: *mut c_void) {
    let view = context as *mut Object;
    let _: BOOL = msg_send![view, becomeFirstResponder];
}

unsafe extern "C" fn resign_first_responder_trampoline(context: *mut c_void) {
    let view = context as *mut Object;
    let _: BOOL = msg_send![view, resignFirstResponder];
}

extern "C" fn handle_touches_began(
    this: &Object,
    _sel: Sel,
    touches: *mut Object,
    _event: *mut Object,
) {
    let Some(state) = (unsafe { get_window_state(this) }) else {
        return;
    };

    // Cancel any active scroll momentum when a new touch begins
    state.lock().scroll_momentum = None;

    let Some((position, click_count)) = (unsafe { primary_touch_info(touches, this, &state) })
    else {
        return;
    };

    let result = dispatch_input(
        &state,
        PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count,
            first_mouse: false,
        }),
    );

    // Show the software keyboard by making the hidden UITextField proxy
    // become first responder. Deferred to next run loop iteration because
    // UIKit may not allow first responder changes during touch handling.
    let proxy = state.lock().keyboard_proxy;
    if !proxy.is_null() {
        unsafe {
            let is_first: BOOL = msg_send![proxy, isFirstResponder];
            if is_first == NO {
                dispatch_async_f(
                    dispatch_get_main_queue_ptr(),
                    proxy as *mut c_void,
                    Some(become_first_responder_trampoline),
                );
            }
        }
    }
}

extern "C" fn handle_touches_moved(
    this: &Object,
    _sel: Sel,
    touches: *mut Object,
    _event: *mut Object,
) {
    let Some(state) = (unsafe { get_window_state(this) }) else {
        return;
    };
    let Some((position, _)) = (unsafe { primary_touch_info(touches, this, &state) }) else {
        return;
    };

    dispatch_input(
        &state,
        PlatformInput::MouseMove(MouseMoveEvent {
            position,
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::default(),
        }),
    );
}

extern "C" fn handle_touches_ended(
    this: &Object,
    _sel: Sel,
    touches: *mut Object,
    _event: *mut Object,
) {
    let Some(state) = (unsafe { get_window_state(this) }) else {
        return;
    };
    let Some((position, click_count)) = (unsafe { primary_touch_info(touches, this, &state) })
    else {
        return;
    };

    // Clear tracked touch
    state.lock().tracked_touch = None;

    dispatch_input(
        &state,
        PlatformInput::MouseUp(MouseUpEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count,
        }),
    );
}

extern "C" fn handle_touches_cancelled(
    this: &Object,
    _sel: Sel,
    touches: *mut Object,
    _event: *mut Object,
) {
    let Some(state) = (unsafe { get_window_state(this) }) else {
        return;
    };

    // Use last known position or zero
    let position = state
        .lock()
        .last_touch_position
        .unwrap_or_else(Point::default);

    // Clear tracked touch
    state.lock().tracked_touch = None;

    dispatch_input(
        &state,
        PlatformInput::MouseUp(MouseUpEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
        }),
    );
}

extern "C" fn handle_layout_subviews(this: &Object, _sel: Sel) {
    unsafe {
        // Call [super layoutSubviews]
        let superclass = class!(UIView);
        let _: () = msg_send![super(this, superclass), layoutSubviews];

        let Some(state) = get_window_state(this) else {
            return;
        };

        let bounds: CGRect = msg_send![this, bounds];
        let scale: f64 = msg_send![this, contentScaleFactor];

        // The view's layer IS the Metal layer (via layerClass override)
        let metal_layer: *mut Object = msg_send![this, layer];
        let _: () = msg_send![metal_layer, setContentsScale: scale];

        let new_size = crate::Size {
            width: px(bounds.size.width as f32),
            height: px(bounds.size.height as f32),
        };
        let scale_factor = scale as f32;
        let device_width = new_size.width.0 * scale_factor;
        let device_height = new_size.height.0 * scale_factor;

        let mut lock = state.lock();
        let size_changed = lock.bounds.size != new_size || lock.scale_factor != scale_factor;
        if !size_changed {
            return;
        }

        lock.bounds.size = new_size;
        lock.scale_factor = scale_factor;

        // The view's layer IS the Metal layer (via replace_layer), so UIKit
        // auto-sizes it. Just update the drawable size for rendering.
        lock.renderer.update_drawable_size(crate::size(
            crate::DevicePixels(device_width as i32),
            crate::DevicePixels(device_height as i32),
        ));

        if let Some(mut callback) = lock.on_resize.take() {
            drop(lock);
            callback(new_size, scale_factor);
            state.lock().on_resize = Some(callback);
        }
    }
}

extern "C" fn handle_trait_collection_change(
    this: &Object,
    _sel: Sel,
    _previous_trait_collection: *mut Object,
) {
    unsafe {
        let superclass = class!(UIView);
        let _: () = msg_send![super(this, superclass), traitCollectionDidChange: _previous_trait_collection];

        let Some(state) = get_window_state(this) else {
            return;
        };

        // Check if the user interface style actually changed
        let current_traits: *mut Object = msg_send![this, traitCollection];
        let current_style: isize = msg_send![current_traits, userInterfaceStyle];

        if !_previous_trait_collection.is_null() {
            let previous_style: isize =
                msg_send![_previous_trait_collection, userInterfaceStyle];
            if current_style == previous_style {
                return;
            }
        }

        log::info!(
            "appearance changed to {}",
            if current_style == 2 { "dark" } else { "light" }
        );

        let mut lock = state.lock();
        if let Some(mut callback) = lock.on_appearance_change.take() {
            drop(lock);
            callback();
            state.lock().on_appearance_change = Some(callback);
        }
    }
}

// ---------------------------------------------------------------------------
// UITextFieldDelegate — intercept text from the hidden UITextField keyboard
// proxy and forward to GPUI as key events.
// ---------------------------------------------------------------------------

extern "C" fn handle_text_field_change(
    this: &Object,
    _sel: Sel,
    _text_field: *mut Object,
    _range: NSRange,
    replacement: *mut Object,
) -> BOOL {
    let Some(state) = (unsafe { get_window_state(this) }) else {
        return NO;
    };

    let text = unsafe {
        if replacement.is_null() {
            return NO;
        }
        let utf8: *const u8 = msg_send![replacement, UTF8String];
        if utf8.is_null() {
            return NO;
        }
        let c_str = std::ffi::CStr::from_ptr(utf8 as *const std::os::raw::c_char);
        c_str.to_string_lossy().into_owned()
    };

    if text.is_empty() {
        // Empty replacement = backspace / delete
        dispatch_input(
            &state,
            PlatformInput::KeyDown(KeyDownEvent {
                keystroke: Keystroke {
                    modifiers: Modifiers::default(),
                    key: "backspace".into(),
                    key_char: None,

                    native_key_code: None,
                },
                is_held: false,
                prefer_character_input: false,
            }),
        );
        return NO;
    }

    // Try the input handler first (full text editing support)
    {
        let mut lock = state.lock();
        if let Some(ref mut handler) = lock.input_handler {
            handler.replace_text_in_range(None, &text);
            return NO;
        }
    }

    // Fall back to dispatching as a KeyDown event
    dispatch_input(
        &state,
        PlatformInput::KeyDown(KeyDownEvent {
            keystroke: Keystroke {
                modifiers: Modifiers::default(),
                key: text.clone(),
                key_char: Some(text),
                native_key_code: None,
            },
            is_held: false,
            prefer_character_input: false,
        }),
    );
    // Return NO so UITextField stays empty — all text is handled by GPUI
    NO
}

extern "C" fn handle_text_field_return(
    this: &Object,
    _sel: Sel,
    _text_field: *mut Object,
) -> BOOL {
    let Some(state) = (unsafe { get_window_state(this) }) else {
        return YES;
    };
    dispatch_input(
        &state,
        PlatformInput::KeyDown(KeyDownEvent {
            keystroke: Keystroke {
                modifiers: Modifiers::default(),
                key: "enter".into(),
                key_char: Some("\n".into()),
                native_key_code: None,
            },
            is_held: false,
            prefer_character_input: false,
        }),
    );
    NO
}

// ---------------------------------------------------------------------------
// UIKeyInput — GPUIView is the first responder for hardware keyboard input.
// ---------------------------------------------------------------------------

extern "C" fn can_become_first_responder(_this: &Object, _sel: Sel) -> BOOL {
    YES
}

extern "C" fn has_text(this: &Object, _sel: Sel) -> BOOL {
    let Some(state) = (unsafe { get_window_state(this) }) else {
        return YES;
    };
    if state.lock().input_handler.is_some() { YES } else { YES }
}

extern "C" fn insert_text(this: &Object, _sel: Sel, text_obj: *mut Object) {
    let Some(state) = (unsafe { get_window_state(this) }) else {
        return;
    };
    unsafe {
        let utf8: *const std::os::raw::c_char = msg_send![text_obj, UTF8String];
        if utf8.is_null() {
            return;
        }
        let text = std::ffi::CStr::from_ptr(utf8).to_string_lossy().into_owned();
        if text.is_empty() {
            return;
        }

        // If a hardware key press already dispatched this character, skip
        {
            let mut lock = state.lock();
            if lock.last_press_had_key {
                lock.last_press_had_key = false;
                return;
            }
        }

        // Try the input handler first (full text editing support)
        {
            let mut lock = state.lock();
            if let Some(ref mut handler) = lock.input_handler {
                handler.replace_text_in_range(None, &text);
                return;
            }
        }

        // No input handler — dispatch as KeyDown
        dispatch_input(
            &state,
            PlatformInput::KeyDown(KeyDownEvent {
                keystroke: Keystroke {
                    modifiers: Modifiers::default(),
                    key: text.clone(),
                    key_char: Some(text),
                    native_key_code: None,
                },
                is_held: false,
                prefer_character_input: true,
            }),
        );
    }
}

extern "C" fn delete_backward(this: &Object, _sel: Sel) {
    let Some(state) = (unsafe { get_window_state(this) }) else {
        return;
    };
    // If hardware key press already dispatched backspace, skip
    {
        let mut lock = state.lock();
        if lock.last_press_had_key {
            lock.last_press_had_key = false;
            return;
        }
    }
    dispatch_input(
        &state,
        PlatformInput::KeyDown(KeyDownEvent {
            keystroke: Keystroke {
                modifiers: Modifiers::default(),
                key: "backspace".into(),
                key_char: None,
                native_key_code: None,
            },
            is_held: false,
            prefer_character_input: false,
        }),
    );
}

// UITextInputTraits
extern "C" fn keyboard_type(_this: &Object, _sel: Sel) -> isize { 0 } // UIKeyboardTypeDefault
extern "C" fn autocorrection_type(_this: &Object, _sel: Sel) -> isize { 1 } // UITextAutocorrectionTypeNo
extern "C" fn autocapitalization_type(_this: &Object, _sel: Sel) -> isize { 0 } // UITextAutocapitalizationTypeNone
extern "C" fn spell_checking_type(_this: &Object, _sel: Sel) -> isize { 1 } // UITextSpellCheckingTypeNo

// ---------------------------------------------------------------------------
// Hardware keyboard via UIPresses (iOS 13.4+)
// ---------------------------------------------------------------------------

/// Map a UIKeyboardHIDUsage keyCode to a GPUI key name.
fn keycode_to_key_name(keycode: isize) -> Option<&'static str> {
    match keycode {
        40 => Some("enter"),
        41 => Some("escape"),
        42 => Some("backspace"),
        43 => Some("tab"),
        44 => Some("space"),
        58..=69 => {
            // F1 (58) through F12 (69)
            static F_KEYS: [&str; 12] = [
                "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12",
            ];
            Some(F_KEYS[(keycode - 58) as usize])
        }
        74 => Some("home"),
        75 => Some("pageup"),
        76 => Some("delete"),
        77 => Some("end"),
        78 => Some("pagedown"),
        79 => Some("right"),
        80 => Some("left"),
        81 => Some("down"),
        82 => Some("up"),
        _ => None,
    }
}

/// Extract Modifiers from UIKeyModifierFlags bitmask.
fn modifiers_from_flags(flags: isize) -> Modifiers {
    Modifiers {
        control: flags & 0x040000 != 0,
        alt: flags & 0x080000 != 0,
        shift: flags & 0x020000 != 0,
        platform: flags & 0x100000 != 0,
        function: false,
    }
}

/// Returns true if the keycode is a modifier-only key (224-231 HID usage).
fn is_modifier_key(keycode: isize) -> bool {
    (224..=231).contains(&keycode)
}

extern "C" fn handle_presses_began(
    this: &Object,
    _sel: Sel,
    presses: *mut Object,
    _event: *mut Object,
) {
    let Some(state) = (unsafe { get_window_state(this) }) else {
        return;
    };
    unsafe {
        let all: *mut Object = msg_send![presses, allObjects];
        let count: usize = msg_send![all, count];
        for i in 0..count {
            let press: *mut Object = msg_send![all, objectAtIndex: i];
            let key: *mut Object = msg_send![press, key];
            if key.is_null() {
                continue;
            }
            let keycode: isize = msg_send![key, keyCode];
            let modifier_flags: isize = msg_send![key, modifierFlags];
            let modifiers = modifiers_from_flags(modifier_flags);

            if is_modifier_key(keycode) {
                dispatch_input(
                    &state,
                    PlatformInput::ModifiersChanged(ModifiersChangedEvent {
                        modifiers,
                        capslock: crate::Capslock::default(),
                    }),
                );
                continue;
            }

            let key_name = if let Some(name) = keycode_to_key_name(keycode) {
                name.to_string()
            } else {
                let chars: *mut Object = msg_send![key, charactersIgnoringModifiers];
                if chars.is_null() {
                    continue;
                }
                let utf8: *const std::os::raw::c_char = msg_send![chars, UTF8String];
                if utf8.is_null() {
                    continue;
                }
                std::ffi::CStr::from_ptr(utf8)
                    .to_string_lossy()
                    .to_lowercase()
            };

            let key_char = {
                let chars: *mut Object = msg_send![key, characters];
                if !chars.is_null() {
                    let utf8: *const std::os::raw::c_char = msg_send![chars, UTF8String];
                    if !utf8.is_null() {
                        Some(std::ffi::CStr::from_ptr(utf8).to_string_lossy().into_owned())
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            // Mark that a hardware press dispatched this key, so insertText:
            // can skip duplicating it
            state.lock().last_press_had_key = true;

            dispatch_input(
                &state,
                PlatformInput::KeyDown(KeyDownEvent {
                    keystroke: Keystroke {
                        modifiers,
                        key: key_name,
                        key_char,
                        native_key_code: Some(keycode as u16),
                    },
                    is_held: false,
                    prefer_character_input: false,
                }),
            );
        }
    }
}

extern "C" fn handle_presses_ended(
    this: &Object,
    _sel: Sel,
    presses: *mut Object,
    _event: *mut Object,
) {
    let Some(state) = (unsafe { get_window_state(this) }) else {
        return;
    };
    unsafe {
        let all: *mut Object = msg_send![presses, allObjects];
        let count: usize = msg_send![all, count];
        for i in 0..count {
            let press: *mut Object = msg_send![all, objectAtIndex: i];
            let key: *mut Object = msg_send![press, key];
            if key.is_null() {
                continue;
            }
            let keycode: isize = msg_send![key, keyCode];
            let modifier_flags: isize = msg_send![key, modifierFlags];
            let modifiers = modifiers_from_flags(modifier_flags);

            if is_modifier_key(keycode) {
                dispatch_input(
                    &state,
                    PlatformInput::ModifiersChanged(ModifiersChangedEvent {
                        modifiers,
                        capslock: crate::Capslock::default(),
                    }),
                );
                continue;
            }

            let key_name = if let Some(name) = keycode_to_key_name(keycode) {
                name.to_string()
            } else {
                let chars: *mut Object = msg_send![key, charactersIgnoringModifiers];
                if chars.is_null() {
                    continue;
                }
                let utf8: *const std::os::raw::c_char = msg_send![chars, UTF8String];
                if utf8.is_null() {
                    continue;
                }
                std::ffi::CStr::from_ptr(utf8)
                    .to_string_lossy()
                    .to_lowercase()
            };

            dispatch_input(
                &state,
                PlatformInput::KeyUp(KeyUpEvent {
                    keystroke: Keystroke {
                        modifiers,
                        key: key_name,
                        key_char: None,
                        native_key_code: Some(keycode as u16),
                    },
                }),
            );
        }
    }
}

extern "C" fn handle_presses_cancelled(
    this: &Object,
    _sel: Sel,
    presses: *mut Object,
    event: *mut Object,
) {
    // Treat cancelled the same as ended
    handle_presses_ended(this, _sel, presses, event);
}

extern "C" fn handle_scroll_pan(this: &Object, _sel: Sel, gesture: *mut Object) {
    let Some(state) = (unsafe { get_window_state(this) }) else {
        return;
    };
    unsafe {
        let gesture_state: isize = msg_send![gesture, state];
        // UIGestureRecognizerState: 1=Began, 2=Changed, 3=Ended, 4=Cancelled
        let touch_phase = match gesture_state {
            1 => TouchPhase::Started,
            2 => TouchPhase::Moved,
            3 | 4 => TouchPhase::Ended,
            _ => return,
        };

        // Get translation (cumulative) and reset to zero for incremental deltas
        let translation: CGPoint = msg_send![gesture, translationInView: this as *const Object as *mut Object];
        let zero = CGPoint { x: 0.0, y: 0.0 };
        let _: () = msg_send![gesture, setTranslation: zero inView: this as *const Object as *mut Object];

        // Get position of the gesture centroid
        let location: CGPoint = msg_send![gesture, locationInView: this as *const Object as *mut Object];
        let position = point(px(location.x as f32), px(location.y as f32));

        let delta = ScrollDelta::Pixels(point(
            px(translation.x as f32),
            px(translation.y as f32),
        ));

        dispatch_input(
            &state,
            PlatformInput::ScrollWheel(ScrollWheelEvent {
                position,
                delta,
                modifiers: Modifiers::default(),
                touch_phase,
            }),
        );
    }
}

// ---------------------------------------------------------------------------
// Single-finger scroll pan gesture
// ---------------------------------------------------------------------------

extern "C" fn handle_single_finger_pan(this: &Object, _sel: Sel, gesture: *mut Object) {
    let Some(state) = (unsafe { get_window_state(this) }) else {
        return;
    };
    unsafe {
        let gesture_state: isize = msg_send![gesture, state];
        let touch_phase = match gesture_state {
            1 => TouchPhase::Started,
            2 => TouchPhase::Moved,
            3 | 4 => TouchPhase::Ended,
            _ => return,
        };

        let translation: CGPoint =
            msg_send![gesture, translationInView: this as *const Object as *mut Object];
        let zero = CGPoint { x: 0.0, y: 0.0 };
        let _: () =
            msg_send![gesture, setTranslation: zero inView: this as *const Object as *mut Object];

        let location: CGPoint =
            msg_send![gesture, locationInView: this as *const Object as *mut Object];
        let position = point(px(location.x as f32), px(location.y as f32));

        let delta = ScrollDelta::Pixels(point(
            px(translation.x as f32),
            px(translation.y as f32),
        ));

        dispatch_input(
            &state,
            PlatformInput::ScrollWheel(ScrollWheelEvent {
                position,
                delta,
                modifiers: Modifiers::default(),
                touch_phase,
            }),
        );

        // Capture velocity for momentum on end
        if matches!(touch_phase, TouchPhase::Ended) {
            let velocity: CGPoint =
                msg_send![gesture, velocityInView: this as *const Object as *mut Object];
            let vx = velocity.x as f32;
            let vy = velocity.y as f32;
            if vx.abs() > 0.5 || vy.abs() > 0.5 {
                state.lock().scroll_momentum = Some(ScrollMomentum {
                    velocity: point(vx, vy),
                    position,
                    last_time: Instant::now(),
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Pinch gesture
// ---------------------------------------------------------------------------

extern "C" fn handle_pinch(this: &Object, _sel: Sel, gesture: *mut Object) {
    let Some(state) = (unsafe { get_window_state(this) }) else {
        return;
    };
    unsafe {
        let gesture_state: isize = msg_send![gesture, state];
        let touch_phase = match gesture_state {
            1 => TouchPhase::Started,
            2 => TouchPhase::Moved,
            3 | 4 => TouchPhase::Ended,
            _ => return,
        };

        let scale: f64 = msg_send![gesture, scale];
        // Reset to 1.0 so next callback gives incremental scale
        let _: () = msg_send![gesture, setScale: 1.0f64];

        let location: CGPoint =
            msg_send![gesture, locationInView: this as *const Object as *mut Object];
        let center = point(px(location.x as f32), px(location.y as f32));

        dispatch_input(
            &state,
            PlatformInput::Pinch(PinchEvent {
                center,
                scale: scale as f32,
                modifiers: Modifiers::default(),
                touch_phase,
            }),
        );
    }
}

// ---------------------------------------------------------------------------
// Rotation gesture
// ---------------------------------------------------------------------------

extern "C" fn handle_rotation(this: &Object, _sel: Sel, gesture: *mut Object) {
    let Some(state) = (unsafe { get_window_state(this) }) else {
        return;
    };
    unsafe {
        let gesture_state: isize = msg_send![gesture, state];
        let touch_phase = match gesture_state {
            1 => TouchPhase::Started,
            2 => TouchPhase::Moved,
            3 | 4 => TouchPhase::Ended,
            _ => return,
        };

        let rotation: f64 = msg_send![gesture, rotation];
        // Reset to 0 so next callback gives incremental rotation
        let _: () = msg_send![gesture, setRotation: 0.0f64];

        let location: CGPoint =
            msg_send![gesture, locationInView: this as *const Object as *mut Object];
        let center = point(px(location.x as f32), px(location.y as f32));

        dispatch_input(
            &state,
            PlatformInput::Rotation(RotationEvent {
                center,
                rotation: rotation as f32,
                modifiers: Modifiers::default(),
                touch_phase,
            }),
        );
    }
}

// ---------------------------------------------------------------------------
// Safe area insets change
// ---------------------------------------------------------------------------

extern "C" fn handle_safe_area_insets_change(this: &Object, _sel: Sel) {
    unsafe {
        let superclass = class!(UIView);
        let _: () = msg_send![super(this, superclass), safeAreaInsetsDidChange];

        let Some(state) = get_window_state(this) else {
            return;
        };

        let insets: UIEdgeInsets = msg_send![this, safeAreaInsets];
        state.lock().safe_area_insets = insets;
    }
}

// ---------------------------------------------------------------------------
// GPUIGestureDelegate — allows simultaneous gesture recognition
// ---------------------------------------------------------------------------

static mut GPUI_GESTURE_DELEGATE_CLASS: *const Class = std::ptr::null();

#[ctor]
unsafe fn register_gesture_delegate_class() {
    let superclass = class!(NSObject);
    let mut decl = ClassDecl::new("GPUIGestureDelegate", superclass)
        .expect("failed to declare GPUIGestureDelegate class");

    decl.add_method(
        sel!(gestureRecognizer:shouldRecognizeSimultaneouslyWithGestureRecognizer:),
        gesture_should_recognize_simultaneously
            as extern "C" fn(&Object, Sel, *mut Object, *mut Object) -> BOOL,
    );

    unsafe {
        GPUI_GESTURE_DELEGATE_CLASS = decl.register();
    }
}

extern "C" fn gesture_should_recognize_simultaneously(
    _this: &Object,
    _sel: Sel,
    _gesture1: *mut Object,
    _gesture2: *mut Object,
) -> BOOL {
    YES
}

// ---------------------------------------------------------------------------
// GPUIThermalObserver — observes thermal state change notifications
// ---------------------------------------------------------------------------

static mut GPUI_THERMAL_OBSERVER_CLASS: *const Class = std::ptr::null();

#[ctor]
unsafe fn register_thermal_observer_class() {
    let superclass = class!(NSObject);
    let mut decl = ClassDecl::new("GPUIThermalObserver", superclass)
        .expect("failed to declare GPUIThermalObserver class");

    decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);

    decl.add_method(
        sel!(thermalStateChanged:),
        handle_thermal_state_changed as extern "C" fn(&Object, Sel, *mut Object),
    );

    unsafe {
        GPUI_THERMAL_OBSERVER_CLASS = decl.register();
    }
}

extern "C" fn handle_thermal_state_changed(this: &Object, _sel: Sel, _notification: *mut Object) {
    unsafe {
        let callback_ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if !callback_ptr.is_null() {
            let callback = &mut *(callback_ptr as *mut Box<dyn FnMut()>);
            callback();
        }
    }
}

// ---------------------------------------------------------------------------
// GPUIInputModeObserver — observes keyboard layout / input mode changes
// ---------------------------------------------------------------------------

static mut GPUI_INPUT_MODE_OBSERVER_CLASS: *const Class = std::ptr::null();

#[ctor]
unsafe fn register_input_mode_observer_class() {
    let superclass = class!(NSObject);
    let mut decl = ClassDecl::new("GPUIInputModeObserver", superclass)
        .expect("failed to declare GPUIInputModeObserver class");

    decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);

    decl.add_method(
        sel!(inputModeChanged:),
        handle_input_mode_changed as extern "C" fn(&Object, Sel, *mut Object),
    );

    unsafe {
        GPUI_INPUT_MODE_OBSERVER_CLASS = decl.register();
    }
}

extern "C" fn handle_input_mode_changed(this: &Object, _sel: Sel, _notification: *mut Object) {
    // Defer to next run loop iteration to avoid re-entrant RefCell borrows.
    // This notification can fire synchronously during window setup (e.g. when
    // the keyboard proxy becomes first responder), at which point the App
    // RefCell is already borrowed by open_window.
    unsafe {
        let callback_ptr: *mut c_void = *this.get_ivar(CALLBACK_IVAR);
        if !callback_ptr.is_null() {
            // Prevent the callback from being freed or moved — the observer
            // object (and its ivar) outlives this dispatch. We pass the raw
            // pointer through dispatch_async_f; the trampoline dereferences it.
            dispatch_async_f(
                dispatch_get_main_queue_ptr(),
                callback_ptr,
                Some(input_mode_changed_trampoline),
            );
        }
    }
}

unsafe extern "C" fn input_mode_changed_trampoline(context: *mut c_void) {
    let callback = &mut *(context as *mut Box<dyn FnMut()>);
    callback();
}

// ---------------------------------------------------------------------------
// GPUISceneObserver — receives UIScene lifecycle notifications and forwards
// them to the window state callbacks.
// ---------------------------------------------------------------------------

static mut GPUI_SCENE_OBSERVER_CLASS: *const Class = std::ptr::null();

#[ctor]
unsafe fn register_scene_observer_class() {
    let superclass = class!(NSObject);
    let mut decl = ClassDecl::new("GPUISceneObserver", superclass)
        .expect("failed to declare GPUISceneObserver class");

    decl.add_ivar::<*mut c_void>(WINDOW_STATE_IVAR);

    decl.add_method(
        sel!(sceneDidActivate:),
        handle_scene_did_activate as extern "C" fn(&Object, Sel, *mut Object),
    );
    decl.add_method(
        sel!(sceneWillDeactivate:),
        handle_scene_will_deactivate as extern "C" fn(&Object, Sel, *mut Object),
    );
    decl.add_method(
        sel!(sceneDidEnterBackground:),
        handle_scene_did_enter_background as extern "C" fn(&Object, Sel, *mut Object),
    );
    decl.add_method(
        sel!(sceneWillEnterForeground:),
        handle_scene_will_enter_foreground as extern "C" fn(&Object, Sel, *mut Object),
    );

    unsafe {
        GPUI_SCENE_OBSERVER_CLASS = decl.register();
    }
}

extern "C" fn handle_scene_did_activate(this: &Object, _sel: Sel, _notification: *mut Object) {
    let Some(state) = (unsafe { get_scene_observer_state(this) }) else {
        return;
    };
    log::debug!("scene did activate");
    let mut lock = state.lock();
    lock.is_active = true;
    if let Some(mut callback) = lock.on_active_change.take() {
        drop(lock);
        callback(true);
        state.lock().on_active_change = Some(callback);
    }
}

extern "C" fn handle_scene_will_deactivate(this: &Object, _sel: Sel, _notification: *mut Object) {
    let Some(state) = (unsafe { get_scene_observer_state(this) }) else {
        return;
    };
    log::debug!("scene will deactivate");
    let mut lock = state.lock();
    lock.is_active = false;
    if let Some(mut callback) = lock.on_active_change.take() {
        drop(lock);
        callback(false);
        state.lock().on_active_change = Some(callback);
    }
}

extern "C" fn handle_scene_did_enter_background(
    this: &Object,
    _sel: Sel,
    _notification: *mut Object,
) {
    let Some(state) = (unsafe { get_scene_observer_state(this) }) else {
        return;
    };
    log::debug!("scene entered background — pausing display link");
    let lock = state.lock();
    if !lock.display_link.is_null() {
        unsafe {
            let _: () = msg_send![lock.display_link, setPaused: YES];
        }
    }
}

extern "C" fn handle_scene_will_enter_foreground(
    this: &Object,
    _sel: Sel,
    _notification: *mut Object,
) {
    let Some(state) = (unsafe { get_scene_observer_state(this) }) else {
        return;
    };
    log::debug!("scene will enter foreground — resuming display link");
    let lock = state.lock();
    if !lock.display_link.is_null() {
        unsafe {
            let _: () = msg_send![lock.display_link, setPaused: NO];
        }
    }
}

unsafe fn get_scene_observer_state(observer: &Object) -> Option<Rc<Mutex<IosWindowState>>> {
    let ptr: *mut c_void = *observer.get_ivar(WINDOW_STATE_IVAR);
    if ptr.is_null() {
        return None;
    }
    let rc = Rc::from_raw(ptr as *const Mutex<IosWindowState>);
    let clone = rc.clone();
    std::mem::forget(rc);
    Some(clone)
}

// ---------------------------------------------------------------------------
// Platform types
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub(crate) struct IosKeyboardLayout {
    id: String,
    name: String,
}

impl IosKeyboardLayout {
    fn current() -> Self {
        // Try to query the current text input mode for a language tag
        let (id, name) = unsafe {
            let app: *mut Object = msg_send![class!(UIApplication), sharedApplication];
            if app.is_null() {
                return Self {
                    id: "ios".into(),
                    name: "iOS".into(),
                };
            }
            // Get active text input mode
            let input_modes: *mut Object =
                msg_send![class!(UITextInputMode), activeInputModes];
            if !input_modes.is_null() {
                let count: usize = msg_send![input_modes, count];
                if count > 0 {
                    let mode: *mut Object = msg_send![input_modes, objectAtIndex: 0usize];
                    let lang: *mut Object = msg_send![mode, primaryLanguage];
                    if !lang.is_null() {
                        let utf8: *const std::os::raw::c_char = msg_send![lang, UTF8String];
                        if !utf8.is_null() {
                            let s = std::ffi::CStr::from_ptr(utf8)
                                .to_string_lossy()
                                .into_owned();
                            return Self {
                                id: s.clone(),
                                name: s,
                            };
                        }
                    }
                }
            }
            ("ios".into(), "iOS".into())
        };
        Self { id, name }
    }
}

impl PlatformKeyboardLayout for IosKeyboardLayout {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Scroll momentum state for simulating iOS-like deceleration after a finger lift.
struct ScrollMomentum {
    velocity: Point<f32>,
    position: Point<Pixels>,
    last_time: Instant,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct UIEdgeInsets {
    top: f64,
    left: f64,
    bottom: f64,
    right: f64,
}

unsafe impl objc::Encode for UIEdgeInsets {
    fn encode() -> objc::Encoding {
        let encoding = format!(
            "{{UIEdgeInsets={}{}{}{}}}",
            f64::encode().as_str(),
            f64::encode().as_str(),
            f64::encode().as_str(),
            f64::encode().as_str()
        );
        unsafe { objc::Encoding::from_str(&encoding) }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct CGSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

#[derive(Debug)]
pub(crate) struct IosDisplay {
    id: DisplayId,
    bounds: Bounds<Pixels>,
}

impl IosDisplay {
    fn primary() -> Self {
        let (width, height) = unsafe {
            let screen: *mut Object = msg_send![class!(UIScreen), mainScreen];
            let bounds: CGRect = msg_send![screen, bounds];
            (bounds.size.width as f32, bounds.size.height as f32)
        };

        Self {
            id: DisplayId(1),
            bounds: Bounds::new(point(px(0.0), px(0.0)), size(px(width), px(height))),
        }
    }
}

impl PlatformDisplay for IosDisplay {
    fn id(&self) -> DisplayId {
        self.id
    }

    fn uuid(&self) -> Result<uuid::Uuid> {
        Ok(uuid::Uuid::from_bytes([0x01; 16]))
    }

    fn bounds(&self) -> Bounds<Pixels> {
        self.bounds
    }
}

pub(crate) struct IosDispatcher;

impl IosDispatcher {
    fn new() -> Self {
        Self
    }

    fn run_runnable(runnable: crate::RunnableVariant) {
        let metadata = runnable.metadata();
        if metadata.is_closed() {
            return;
        }

        let location = metadata.location;
        let start = std::time::Instant::now();
        let timing = TaskTiming {
            location,
            start,
            end: None,
        };

        THREAD_TIMINGS.with(|timings| {
            let mut timings = timings.lock();
            let timings = &mut timings.timings;
            if let Some(last_timing) = timings.iter_mut().rev().next() {
                if last_timing.location == timing.location {
                    return;
                }
            }
            timings.push_back(timing);
        });

        runnable.run();
        let end = std::time::Instant::now();
        THREAD_TIMINGS.with(|timings| {
            let mut timings = timings.lock();
            let timings = &mut timings.timings;
            if let Some(last_timing) = timings.iter_mut().rev().next() {
                last_timing.end = Some(end);
            }
        });
    }

    fn dispatch_get_main_queue() -> DispatchQueue {
        dispatch_get_main_queue_ptr()
    }

    fn queue_priority(priority: Priority) -> isize {
        match priority {
            Priority::RealtimeAudio => {
                panic!("RealtimeAudio priority should use spawn_realtime, not dispatch")
            }
            Priority::High => DISPATCH_QUEUE_PRIORITY_HIGH,
            Priority::Medium => DISPATCH_QUEUE_PRIORITY_DEFAULT,
            Priority::Low => DISPATCH_QUEUE_PRIORITY_LOW,
        }
    }

    fn duration_to_dispatch_delta(duration: Duration) -> i64 {
        let nanos = duration.as_nanos();
        if nanos > i64::MAX as u128 {
            i64::MAX
        } else {
            nanos as i64
        }
    }
}

fn dispatch_get_main_queue_ptr() -> DispatchQueue {
    addr_of!(_dispatch_main_q) as *const _ as DispatchQueue
}

/// Query UIKit for the current system appearance (Light/Dark mode).
fn detect_system_appearance() -> WindowAppearance {
    unsafe {
        let screen: *mut Object = msg_send![class!(UIScreen), mainScreen];
        let traits: *mut Object = msg_send![screen, traitCollection];
        let style: isize = msg_send![traits, userInterfaceStyle];
        // UIUserInterfaceStyle: 0 = Unspecified, 1 = Light, 2 = Dark
        match style {
            2 => WindowAppearance::Dark,
            _ => WindowAppearance::Light,
        }
    }
}

extern "C" fn dispatch_trampoline(context: *mut c_void) {
    let runnable = unsafe {
        crate::RunnableVariant::from_raw(NonNull::new_unchecked(context.cast::<()>()))
    };
    IosDispatcher::run_runnable(runnable);
}

impl PlatformDispatcher for IosDispatcher {
    fn get_all_timings(&self) -> Vec<ThreadTaskTimings> {
        let global_timings = GLOBAL_THREAD_TIMINGS.lock();
        ThreadTaskTimings::convert(&global_timings)
    }

    fn get_current_thread_timings(&self) -> Vec<TaskTiming> {
        THREAD_TIMINGS.with(|timings| {
            let timings = &timings.lock().timings;
            let mut vec = Vec::with_capacity(timings.len());
            let (s1, s2) = timings.as_slices();
            vec.extend_from_slice(s1);
            vec.extend_from_slice(s2);
            vec
        })
    }

    fn is_main_thread(&self) -> bool {
        unsafe {
            let result: objc::runtime::BOOL = msg_send![class!(NSThread), isMainThread];
            result != objc::runtime::NO
        }
    }

    fn dispatch(&self, runnable: crate::RunnableVariant, _priority: Priority) {
        let context = runnable.into_raw().as_ptr() as *mut c_void;
        let queue_priority = Self::queue_priority(_priority);
        unsafe {
            dispatch_async_f(
                dispatch_get_global_queue(queue_priority, 0),
                context,
                Some(dispatch_trampoline),
            );
        }
    }

    fn dispatch_on_main_thread(&self, runnable: crate::RunnableVariant, _priority: Priority) {
        let context = runnable.into_raw().as_ptr() as *mut c_void;
        unsafe {
            dispatch_async_f(
                Self::dispatch_get_main_queue(),
                context,
                Some(dispatch_trampoline),
            );
        }
    }

    fn dispatch_after(&self, duration: Duration, runnable: crate::RunnableVariant) {
        let context = runnable.into_raw().as_ptr() as *mut c_void;
        let delta = Self::duration_to_dispatch_delta(duration);
        unsafe {
            let when = dispatch_time(DISPATCH_TIME_NOW, delta);
            dispatch_after_f(
                when,
                dispatch_get_global_queue(DISPATCH_QUEUE_PRIORITY_HIGH, 0),
                context,
                Some(dispatch_trampoline),
            );
        }
    }

    fn spawn_realtime(&self, f: Box<dyn FnOnce() + Send>) {
        let _ = thread::Builder::new()
            .name("gpui-ios-realtime".into())
            .spawn(f);
    }
}

pub(crate) struct IosPlatform {
    state: Mutex<IosPlatformState>,
}

struct IosPlatformState {
    background_executor: BackgroundExecutor,
    foreground_executor: ForegroundExecutor,
    text_system: Arc<dyn PlatformTextSystem>,
    display: Rc<IosDisplay>,
    active_window: Option<AnyWindowHandle>,
    open_urls: Option<Box<dyn FnMut(Vec<String>)>>,
    on_quit: Option<Box<dyn FnMut()>>,
    on_reopen: Option<Box<dyn FnMut()>>,
    on_thermal_state_change: Option<Box<dyn FnMut()>>,
    thermal_observer: *mut Object,
    input_mode_observer: *mut Object,
    app_menu_action: Option<Box<dyn FnMut(&dyn Action)>>,
    will_open_menu: Option<Box<dyn FnMut()>>,
    validate_app_menu: Option<Box<dyn FnMut(&dyn Action) -> bool>>,
    keyboard_layout_change: Option<Box<dyn FnMut()>>,
}

impl IosPlatform {
    pub(crate) fn new(_headless: bool) -> Self {
        log::info!("iOS platform initialized");
        let dispatcher = Arc::new(IosDispatcher::new());
        let background_executor = BackgroundExecutor::new(dispatcher.clone());
        let foreground_executor = ForegroundExecutor::new(dispatcher);
        let platform = Self {
            state: Mutex::new(IosPlatformState {
                background_executor,
                foreground_executor,
                text_system: {
                    #[cfg(feature = "font-kit")]
                    { Arc::new(IosTextSystem::new()) }
                    #[cfg(not(feature = "font-kit"))]
                    { Arc::new(NoopTextSystem::new()) }
                },
                display: Rc::new(IosDisplay::primary()),
                active_window: None,
                open_urls: None,
                on_quit: None,
                on_reopen: None,
                on_thermal_state_change: None,
                thermal_observer: std::ptr::null_mut(),
                input_mode_observer: std::ptr::null_mut(),
                app_menu_action: None,
                will_open_menu: None,
                validate_app_menu: None,
                keyboard_layout_change: None,
            }),
        };

        // Store a global pointer to the platform state for gpui_ios_handle_open_url
        let state_ptr = &platform.state as *const Mutex<IosPlatformState> as *mut c_void;
        IOS_PLATFORM_STATE_PTR.store(state_ptr, std::sync::atomic::Ordering::Release);

        platform
    }
}

impl Platform for IosPlatform {
    fn background_executor(&self) -> BackgroundExecutor {
        self.state.lock().background_executor.clone()
    }

    fn foreground_executor(&self) -> ForegroundExecutor {
        self.state.lock().foreground_executor.clone()
    }

    fn text_system(&self) -> Arc<dyn PlatformTextSystem> {
        self.state.lock().text_system.clone()
    }

    fn run(&self, on_finish_launching: Box<dyn FnOnce()>) {
        on_finish_launching();
    }

    fn quit(&self) {
        if let Some(mut callback) = self.state.lock().on_quit.take() {
            callback();
        }
    }

    fn restart(&self, _binary_path: Option<PathBuf>) {}

    fn activate(&self, _ignoring_other_apps: bool) {}

    fn hide(&self) {}

    fn hide_other_apps(&self) {}

    fn unhide_other_apps(&self) {}

    fn displays(&self) -> Vec<Rc<dyn PlatformDisplay>> {
        vec![self.state.lock().display.clone()]
    }

    fn primary_display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        Some(self.state.lock().display.clone())
    }

    fn active_window(&self) -> Option<AnyWindowHandle> {
        self.state.lock().active_window
    }

    fn open_window(
        &self,
        handle: AnyWindowHandle,
        options: WindowParams,
    ) -> Result<Box<dyn PlatformWindow>> {
        let display = self.state.lock().display.clone();
        let window = IosWindow::new(handle, options, display);
        self.state.lock().active_window = Some(handle);
        Ok(Box::new(window))
    }

    fn window_appearance(&self) -> WindowAppearance {
        detect_system_appearance()
    }

    fn open_url(&self, url: &str) {
        unsafe {
            let ns_url_string: *mut Object = msg_send![class!(NSString),
                stringWithUTF8String: std::ffi::CString::new(url).unwrap_or_default().as_ptr()
            ];
            let ns_url: *mut Object = msg_send![class!(NSURL), URLWithString: ns_url_string];
            if ns_url.is_null() {
                log::error!("failed to create NSURL from: {}", url);
                return;
            }
            let app: *mut Object = msg_send![class!(UIApplication), sharedApplication];
            let options: *mut Object = msg_send![class!(NSDictionary), dictionary];
            let _: () = msg_send![app, openURL: ns_url
                options: options
                completionHandler: std::ptr::null::<c_void>()];
        }
    }

    fn on_open_urls(&self, callback: Box<dyn FnMut(Vec<String>)>) {
        self.state.lock().open_urls = Some(callback);
    }

    fn register_url_scheme(&self, _url: &str) -> Task<Result<()>> {
        Task::ready(Err(anyhow!("register_url_scheme is not yet implemented on iOS")))
    }

    fn prompt_for_paths(
        &self,
        _options: PathPromptOptions,
    ) -> oneshot::Receiver<Result<Option<Vec<PathBuf>>>> {
        let (tx, rx) = oneshot::channel();
        let _ = tx.send(Ok(None));
        rx
    }

    fn prompt_for_new_path(
        &self,
        _directory: &Path,
        _suggested_name: Option<&str>,
    ) -> oneshot::Receiver<Result<Option<PathBuf>>> {
        let (tx, rx) = oneshot::channel();
        let _ = tx.send(Ok(None));
        rx
    }

    fn can_select_mixed_files_and_dirs(&self) -> bool {
        false
    }

    fn reveal_path(&self, _path: &Path) {}

    fn open_with_system(&self, _path: &Path) {}

    fn on_quit(&self, callback: Box<dyn FnMut()>) {
        self.state.lock().on_quit = Some(callback);
    }

    fn on_reopen(&self, callback: Box<dyn FnMut()>) {
        self.state.lock().on_reopen = Some(callback);
    }

    fn set_menus(&self, _menus: Vec<Menu>, _keymap: &Keymap) {}

    fn get_menus(&self) -> Option<Vec<OwnedMenu>> {
        None
    }

    fn set_dock_menu(&self, _menu: Vec<MenuItem>, _keymap: &Keymap) {}

    fn on_app_menu_action(&self, callback: Box<dyn FnMut(&dyn Action)>) {
        self.state.lock().app_menu_action = Some(callback);
    }

    fn on_will_open_app_menu(&self, callback: Box<dyn FnMut()>) {
        self.state.lock().will_open_menu = Some(callback);
    }

    fn on_validate_app_menu_command(&self, callback: Box<dyn FnMut(&dyn Action) -> bool>) {
        self.state.lock().validate_app_menu = Some(callback);
    }

    fn thermal_state(&self) -> ThermalState {
        unsafe {
            let process_info: *mut Object = msg_send![class!(NSProcessInfo), processInfo];
            let state: isize = msg_send![process_info, thermalState];
            // NSProcessInfoThermalState: 0=Nominal, 1=Fair, 2=Serious, 3=Critical
            match state {
                1 => ThermalState::Fair,
                2 => ThermalState::Serious,
                3 => ThermalState::Critical,
                _ => ThermalState::Nominal,
            }
        }
    }

    fn on_thermal_state_change(&self, callback: Box<dyn FnMut()>) {
        let mut platform_state = self.state.lock();
        platform_state.on_thermal_state_change = Some(callback);

        // Register for NSProcessInfoThermalStateDidChangeNotification
        unsafe {
            let observer: *mut Object = msg_send![GPUI_THERMAL_OBSERVER_CLASS, new];
            let callback_ptr = &mut *platform_state.on_thermal_state_change.as_mut().unwrap()
                as *mut Box<dyn FnMut()> as *mut c_void;
            (*observer).set_ivar::<*mut c_void>(CALLBACK_IVAR, callback_ptr);

            let center: *mut Object = msg_send![class!(NSNotificationCenter), defaultCenter];
            let name_bytes = b"NSProcessInfoThermalStateDidChangeNotification\0";
            let name: *mut Object =
                msg_send![class!(NSString), stringWithUTF8String: name_bytes.as_ptr()];
            let process_info: *mut Object = msg_send![class!(NSProcessInfo), processInfo];
            let _: () = msg_send![center,
                addObserver: observer
                selector: sel!(thermalStateChanged:)
                name: name
                object: process_info
            ];

            platform_state.thermal_observer = observer;
        }
    }

    fn app_path(&self) -> Result<PathBuf> {
        std::env::current_exe().map_err(Into::into)
    }

    fn path_for_auxiliary_executable(&self, _name: &str) -> Result<PathBuf> {
        Err(anyhow!("auxiliary executable lookup is not implemented on iOS"))
    }

    fn set_cursor_style(&self, _style: CursorStyle) {}

    fn should_auto_hide_scrollbars(&self) -> bool {
        true
    }

    fn read_from_clipboard(&self) -> Option<ClipboardItem> {
        unsafe {
            let pasteboard: *mut Object = msg_send![class!(UIPasteboard), generalPasteboard];
            let has_strings: BOOL = msg_send![pasteboard, hasStrings];
            if has_strings == YES {
                let ns_string: *mut Object = msg_send![pasteboard, string];
                if !ns_string.is_null() {
                    let utf8: *const std::os::raw::c_char = msg_send![ns_string, UTF8String];
                    if !utf8.is_null() {
                        let text = std::ffi::CStr::from_ptr(utf8)
                            .to_string_lossy()
                            .into_owned();
                        // Check for metadata in custom pasteboard type
                        let meta_type_str = std::ffi::CString::new("dev.gpui.clipboard-metadata")
                            .unwrap();
                        let meta_type: *mut Object = msg_send![class!(NSString),
                            stringWithUTF8String: meta_type_str.as_ptr()];
                        let meta_data: *mut Object =
                            msg_send![pasteboard, dataForPasteboardType: meta_type];
                        if !meta_data.is_null() {
                            let meta_string: *mut Object = msg_send![class!(NSString), alloc];
                            let encoding: usize = 4; // NSUTF8StringEncoding
                            let meta_string: *mut Object = msg_send![meta_string,
                                initWithData: meta_data
                                encoding: encoding
                            ];
                            if !meta_string.is_null() {
                                let meta_utf8: *const std::os::raw::c_char =
                                    msg_send![meta_string, UTF8String];
                                if !meta_utf8.is_null() {
                                    let metadata = std::ffi::CStr::from_ptr(meta_utf8)
                                        .to_string_lossy()
                                        .into_owned();
                                    return Some(ClipboardItem::new_string_with_metadata(
                                        text, metadata,
                                    ));
                                }
                            }
                        }
                        return Some(ClipboardItem::new_string(text));
                    }
                }
            }
            None
        }
    }

    fn write_to_clipboard(&self, item: ClipboardItem) {
        unsafe {
            let pasteboard: *mut Object = msg_send![class!(UIPasteboard), generalPasteboard];
            if let Some(text) = item.text() {
                let ns_string: *mut Object = msg_send![class!(NSString),
                    stringWithUTF8String: std::ffi::CString::new(text).unwrap_or_default().as_ptr()
                ];
                let _: () = msg_send![pasteboard, setString: ns_string];

                // Write metadata if present
                if let Some(metadata) = item.metadata() {
                    let meta_type_str =
                        std::ffi::CString::new("dev.gpui.clipboard-metadata").unwrap();
                    let meta_type: *mut Object = msg_send![class!(NSString),
                        stringWithUTF8String: meta_type_str.as_ptr()
                    ];
                    let meta_ns: *mut Object = msg_send![class!(NSString),
                        stringWithUTF8String: std::ffi::CString::new(metadata.as_str()).unwrap_or_default().as_ptr()
                    ];
                    let encoding: usize = 4; // NSUTF8StringEncoding
                    let meta_data: *mut Object =
                        msg_send![meta_ns, dataUsingEncoding: encoding];
                    let _: () =
                        msg_send![pasteboard, setData: meta_data forPasteboardType: meta_type];
                }
            }
        }
    }

    fn write_credentials(&self, url: &str, username: &str, password: &[u8]) -> Task<Result<()>> {
        let url = url.to_string();
        let username = username.to_string();
        let password = password.to_vec();
        self.state.lock().background_executor.spawn(async move {
            unsafe {
                use ios_security::*;

                let url = CFString::from(url.as_str());
                let username = CFString::from(username.as_str());
                let password = CFData::from_buffer(&password);

                let mut query_attrs = CFMutableDictionary::with_capacity(2);
                query_attrs.set(kSecClass as *const _, kSecClassInternetPassword as *const _);
                query_attrs.set(kSecAttrServer as *const _, url.as_CFTypeRef());

                let mut attrs = CFMutableDictionary::with_capacity(4);
                attrs.set(kSecClass as *const _, kSecClassInternetPassword as *const _);
                attrs.set(kSecAttrServer as *const _, url.as_CFTypeRef());
                attrs.set(kSecAttrAccount as *const _, username.as_CFTypeRef());
                attrs.set(kSecValueData as *const _, password.as_CFTypeRef());

                let mut verb = "updating";
                let mut status = SecItemUpdate(
                    query_attrs.as_concrete_TypeRef(),
                    attrs.as_concrete_TypeRef(),
                );

                if status == errSecItemNotFound {
                    verb = "creating";
                    status = SecItemAdd(attrs.as_concrete_TypeRef(), ptr::null_mut());
                }
                anyhow::ensure!(status == errSecSuccess, "{verb} password failed: {status}");
            }
            Ok(())
        })
    }

    fn read_credentials(&self, url: &str) -> Task<Result<Option<(String, Vec<u8>)>>> {
        let url = url.to_string();
        self.state.lock().background_executor.spawn(async move {
            let url = CFString::from(url.as_str());
            let cf_true = CFBoolean::true_value().as_CFTypeRef();

            unsafe {
                use ios_security::*;

                let mut attrs = CFMutableDictionary::with_capacity(4);
                attrs.set(kSecClass as *const _, kSecClassInternetPassword as *const _);
                attrs.set(kSecAttrServer as *const _, url.as_CFTypeRef());
                attrs.set(kSecReturnAttributes as *const _, cf_true);
                attrs.set(kSecReturnData as *const _, cf_true);

                let mut result = CFTypeRef::from(ptr::null());
                let status = SecItemCopyMatching(attrs.as_concrete_TypeRef(), &mut result);
                match status {
                    ios_security::errSecSuccess => {}
                    ios_security::errSecItemNotFound | ios_security::errSecUserCanceled => {
                        return Ok(None)
                    }
                    _ => anyhow::bail!("reading password failed: {status}"),
                }

                let result = CFType::wrap_under_create_rule(result)
                    .downcast::<CFDictionary>()
                    .ok_or_else(|| anyhow!("keychain item was not a dictionary"))?;
                let username = result
                    .find(kSecAttrAccount as *const _)
                    .ok_or_else(|| anyhow!("account was missing from keychain item"))?;
                let username = CFType::wrap_under_get_rule(*username)
                    .downcast::<CFString>()
                    .ok_or_else(|| anyhow!("account was not a string"))?;
                let password = result
                    .find(kSecValueData as *const _)
                    .ok_or_else(|| anyhow!("password was missing from keychain item"))?;
                let password = CFType::wrap_under_get_rule(*password)
                    .downcast::<CFData>()
                    .ok_or_else(|| anyhow!("password was not data"))?;

                Ok(Some((username.to_string(), password.bytes().to_vec())))
            }
        })
    }

    fn delete_credentials(&self, url: &str) -> Task<Result<()>> {
        let url = url.to_string();
        self.state.lock().background_executor.spawn(async move {
            unsafe {
                use ios_security::*;

                let url = CFString::from(url.as_str());
                let mut query_attrs = CFMutableDictionary::with_capacity(2);
                query_attrs.set(kSecClass as *const _, kSecClassInternetPassword as *const _);
                query_attrs.set(kSecAttrServer as *const _, url.as_CFTypeRef());

                let status = SecItemDelete(query_attrs.as_concrete_TypeRef());
                anyhow::ensure!(status == errSecSuccess, "delete password failed: {status}");
            }
            Ok(())
        })
    }

    fn keyboard_layout(&self) -> Box<dyn PlatformKeyboardLayout> {
        Box::new(IosKeyboardLayout::current())
    }

    fn keyboard_mapper(&self) -> Rc<dyn PlatformKeyboardMapper> {
        Rc::new(DummyKeyboardMapper)
    }

    fn on_keyboard_layout_change(&self, callback: Box<dyn FnMut()>) {
        let mut platform_state = self.state.lock();
        platform_state.keyboard_layout_change = Some(callback);

        // Register for UITextInputCurrentInputModeDidChangeNotification
        unsafe {
            let observer: *mut Object = msg_send![GPUI_INPUT_MODE_OBSERVER_CLASS, new];
            let callback_ptr = &mut *platform_state.keyboard_layout_change.as_mut().unwrap()
                as *mut Box<dyn FnMut()> as *mut c_void;
            (*observer).set_ivar::<*mut c_void>(CALLBACK_IVAR, callback_ptr);

            let center: *mut Object = msg_send![class!(NSNotificationCenter), defaultCenter];
            let name_bytes =
                b"UITextInputCurrentInputModeDidChangeNotification\0";
            let name: *mut Object =
                msg_send![class!(NSString), stringWithUTF8String: name_bytes.as_ptr()];
            let _: () = msg_send![center,
                addObserver: observer
                selector: sel!(inputModeChanged:)
                name: name
                object: std::ptr::null::<Object>()
            ];

            platform_state.input_mode_observer = observer;
        }
    }
}

struct IosWindowState {
    handle: AnyWindowHandle,
    bounds: Bounds<Pixels>,
    display: Rc<dyn PlatformDisplay>,
    scale_factor: f32,
    ui_window: *mut Object,
    ui_view_controller: *mut Object,
    ui_view: *mut Object,
    // Hidden UITextField — shows the software keyboard when it becomes
    // first responder. GPUIView is its delegate, intercepting typed text.
    keyboard_proxy: *mut Object,
    renderer: MetalRenderer,
    // CADisplayLink driving the frame loop
    display_link: *mut Object,
    display_link_target: *mut Object,
    display_link_callback_ptr: *mut c_void,
    // Touch tracking — primary finger only
    tracked_touch: Option<*mut Object>,
    last_touch_position: Option<Point<Pixels>>,
    // Hardware keyboard dedup flag — when pressesBegan dispatches a key,
    // set this so insertText: skips the duplicate
    last_press_had_key: bool,
    // Scroll momentum after single-finger pan ends
    scroll_momentum: Option<ScrollMomentum>,
    // Safe area insets
    safe_area_insets: UIEdgeInsets,
    // Background appearance
    background_appearance: WindowBackgroundAppearance,
    blur_view: *mut Object,
    // Gesture delegate for simultaneous recognition
    gesture_delegate: *mut Object,
    // Scene lifecycle
    is_active: bool,
    scene_observer: *mut Object,
    // Callbacks
    should_close: Option<Box<dyn FnMut() -> bool>>,
    request_frame: Option<Box<dyn FnMut(RequestFrameOptions)>>,
    on_input: Option<Box<dyn FnMut(PlatformInput) -> DispatchEventResult>>,
    on_active_change: Option<Box<dyn FnMut(bool)>>,
    on_hover_change: Option<Box<dyn FnMut(bool)>>,
    on_resize: Option<Box<dyn FnMut(crate::Size<Pixels>, f32)>>,
    on_moved: Option<Box<dyn FnMut()>>,
    on_close: Option<Box<dyn FnOnce()>>,
    on_hit_test_window_control: Option<Box<dyn FnMut() -> Option<WindowControlArea>>>,
    on_appearance_change: Option<Box<dyn FnMut()>>,
    input_handler: Option<PlatformInputHandler>,
    title: String,
}

pub(crate) struct IosWindow(Rc<Mutex<IosWindowState>>);

impl IosWindow {
    fn new(handle: AnyWindowHandle, options: WindowParams, display: Rc<dyn PlatformDisplay>) -> Self {
        log::debug!("creating iOS window");
        let (ui_window, ui_view_controller, ui_view, keyboard_proxy, gesture_delegate, bounds, scale_factor) =
            unsafe {
                let screen: *mut Object = msg_send![class!(UIScreen), mainScreen];
                let screen_bounds: CGRect = msg_send![screen, bounds];
                let scale: f64 = msg_send![screen, scale];

                // On iOS 13+, UIWindow must be associated with a UIWindowScene.
                let app: *mut Object = msg_send![class!(UIApplication), sharedApplication];
                let scenes: *mut Object = msg_send![app, connectedScenes];
                let all_scenes: *mut Object = msg_send![scenes, allObjects];
                let scene_count: usize = msg_send![all_scenes, count];
                let ui_window: *mut Object = if scene_count > 0 {
                    let scene: *mut Object = msg_send![all_scenes, objectAtIndex: 0usize];
                    log::info!("creating UIWindow with UIWindowScene");
                    let w: *mut Object = msg_send![class!(UIWindow), alloc];
                    msg_send![w, initWithWindowScene: scene]
                } else {
                    log::warn!("no UIWindowScene found, falling back to initWithFrame:");
                    let w: *mut Object = msg_send![class!(UIWindow), alloc];
                    msg_send![w, initWithFrame: screen_bounds]
                };

                let ui_view_controller: *mut Object = msg_send![class!(UIViewController), new];

                let ui_view: *mut Object = msg_send![GPUI_VIEW_CLASS, alloc];
                let ui_view: *mut Object = msg_send![ui_view, initWithFrame: screen_bounds];

                // Enable multi-touch for all gesture recognizers
                let _: () = msg_send![ui_view, setMultipleTouchEnabled: YES];

                // Hidden UITextField as keyboard proxy — shows software keyboard
                // when it becomes first responder. GPUIView is its delegate.
                let proxy_frame = CGRect {
                    origin: CGPoint { x: 0.0, y: -100.0 },
                    size: CGSize { width: 1.0, height: 1.0 },
                };
                let keyboard_proxy: *mut Object = msg_send![class!(UITextField), alloc];
                let keyboard_proxy: *mut Object = msg_send![keyboard_proxy, initWithFrame: proxy_frame];
                let _: () = msg_send![keyboard_proxy, setAlpha: 0.01f64];
                let _: () = msg_send![keyboard_proxy, setAutocorrectionType: 1isize]; // UITextAutocorrectionTypeNo
                let _: () = msg_send![keyboard_proxy, setAutocapitalizationType: 0isize]; // UITextAutocapitalizationTypeNone
                let _: () = msg_send![keyboard_proxy, setSpellCheckingType: 1isize]; // UITextSpellCheckingTypeNo
                let _: () = msg_send![ui_view, addSubview: keyboard_proxy];
                // GPUIView is the delegate — intercepts typed text via textField:shouldChangeCharactersInRange:
                let _: () = msg_send![keyboard_proxy, setDelegate: ui_view];

                // Create gesture delegate for simultaneous recognition
                let gesture_delegate: *mut Object =
                    msg_send![GPUI_GESTURE_DELEGATE_CLASS, new];

                // Two-finger pan gesture for scroll
                let pan: *mut Object = msg_send![class!(UIPanGestureRecognizer), alloc];
                let pan: *mut Object =
                    msg_send![pan, initWithTarget: ui_view action: sel!(handleScrollPan:)];
                let _: () = msg_send![pan, setMinimumNumberOfTouches: 2usize];
                let _: () = msg_send![pan, setMaximumNumberOfTouches: 2usize];
                let _: () = msg_send![pan, setDelegate: gesture_delegate];
                let _: () = msg_send![ui_view, addGestureRecognizer: pan];

                // Single-finger pan gesture for scroll
                let single_pan: *mut Object =
                    msg_send![class!(UIPanGestureRecognizer), alloc];
                let single_pan: *mut Object = msg_send![single_pan,
                    initWithTarget: ui_view action: sel!(handleSingleFingerPan:)];
                let _: () = msg_send![single_pan, setMinimumNumberOfTouches: 1usize];
                let _: () = msg_send![single_pan, setMaximumNumberOfTouches: 1usize];
                let _: () = msg_send![ui_view, addGestureRecognizer: single_pan];

                // Pinch gesture
                let pinch: *mut Object =
                    msg_send![class!(UIPinchGestureRecognizer), alloc];
                let pinch: *mut Object =
                    msg_send![pinch, initWithTarget: ui_view action: sel!(handlePinch:)];
                let _: () = msg_send![pinch, setDelegate: gesture_delegate];
                let _: () = msg_send![ui_view, addGestureRecognizer: pinch];

                // Rotation gesture
                let rotation: *mut Object =
                    msg_send![class!(UIRotationGestureRecognizer), alloc];
                let rotation: *mut Object =
                    msg_send![rotation, initWithTarget: ui_view action: sel!(handleRotation:)];
                let _: () = msg_send![rotation, setDelegate: gesture_delegate];
                let _: () = msg_send![ui_view, addGestureRecognizer: rotation];

                let _: () = msg_send![ui_view_controller, setView: ui_view];
                let _: () = msg_send![ui_window, setRootViewController: ui_view_controller];
                let _: () = msg_send![ui_window, makeKeyAndVisible];

                let bounds = Bounds::new(
                    crate::point(px(0.0), px(0.0)),
                    size(
                        px(screen_bounds.size.width as f32),
                        px(screen_bounds.size.height as f32),
                    ),
                );
                (
                    ui_window,
                    ui_view_controller,
                    ui_view,
                    keyboard_proxy,
                    gesture_delegate,
                    bounds,
                    scale as f32,
                )
            };

        // Create the Metal renderer. The view's own layer is already a CAMetalLayer
        // (via the layerClass override), so we attach the renderer to it directly.
        let instance_buffer_pool = Arc::new(Mutex::new(InstanceBufferPool::default()));
        let mut renderer = MetalRenderer::new(instance_buffer_pool, false);

        unsafe {
            // The view's layer IS the CAMetalLayer (via layerClass override).
            // Replace the renderer's internal layer with the view's own layer
            // so drawing goes directly to it — no sublayer needed.
            let view_layer: *mut Object = msg_send![ui_view, layer];
            let view_metal_layer =
                metal::MetalLayer::from_ptr(view_layer as *mut metal::CAMetalLayer);
            // from_ptr creates an owning wrapper; retain so the view keeps its layer alive
            let _: () = msg_send![view_layer, retain];
            renderer.replace_layer(view_metal_layer);
            let _: () = msg_send![view_layer, setContentsScale: scale_factor as f64];
        }

        let device_width = bounds.size.width.0 * scale_factor;
        let device_height = bounds.size.height.0 * scale_factor;
        renderer.update_drawable_size(crate::size(
            crate::DevicePixels(device_width as i32),
            crate::DevicePixels(device_height as i32),
        ));

        log::info!(
            "iOS window created ({}x{} @{}x)",
            bounds.size.width.0,
            bounds.size.height.0,
            scale_factor,
        );

        let window = Self(Rc::new(Mutex::new(IosWindowState {
            handle,
            bounds: if options.bounds.size.width.0 > 0.0 && options.bounds.size.height.0 > 0.0 {
                options.bounds
            } else {
                bounds
            },
            display,
            scale_factor,
            ui_window,
            ui_view_controller,
            ui_view,
            keyboard_proxy,
            renderer,
            display_link: std::ptr::null_mut(),
            display_link_target: std::ptr::null_mut(),
            display_link_callback_ptr: std::ptr::null_mut(),
            tracked_touch: None,
            last_touch_position: None,
            last_press_had_key: false,
            scroll_momentum: None,
            safe_area_insets: UIEdgeInsets::default(),
            background_appearance: WindowBackgroundAppearance::Opaque,
            blur_view: std::ptr::null_mut(),
            gesture_delegate,
            is_active: true,
            scene_observer: std::ptr::null_mut(),
            should_close: None,
            request_frame: None,
            on_input: None,
            on_active_change: None,
            on_hover_change: None,
            on_resize: None,
            on_moved: None,
            on_close: None,
            on_hit_test_window_control: None,
            on_appearance_change: None,
            input_handler: None,
            title: String::new(),
        })));

        // Set the window state ivar on the GPUIView so touch handlers can
        // access it.
        unsafe {
            let state_ptr = Rc::into_raw(window.0.clone()) as *mut c_void;
            (*ui_view).set_ivar::<*mut c_void>(WINDOW_STATE_IVAR, state_ptr);
        }

        // Register for UIScene lifecycle notifications
        window.register_scene_notifications();

        window
    }

    fn register_scene_notifications(&self) {
        unsafe {
            let observer: *mut Object = msg_send![GPUI_SCENE_OBSERVER_CLASS, new];
            let state_ptr = Rc::into_raw(self.0.clone()) as *mut c_void;
            (*observer).set_ivar::<*mut c_void>(WINDOW_STATE_IVAR, state_ptr);

            let center: *mut Object = msg_send![class!(NSNotificationCenter), defaultCenter];

            let did_activate: *mut Object = msg_send![class!(NSString), stringWithUTF8String: UISCENE_DID_ACTIVATE.as_ptr()];
            let _: () = msg_send![center, addObserver: observer
                selector: sel!(sceneDidActivate:)
                name: did_activate
                object: std::ptr::null::<Object>()];

            let will_deactivate: *mut Object = msg_send![class!(NSString), stringWithUTF8String: UISCENE_WILL_DEACTIVATE.as_ptr()];
            let _: () = msg_send![center, addObserver: observer
                selector: sel!(sceneWillDeactivate:)
                name: will_deactivate
                object: std::ptr::null::<Object>()];

            let did_enter_bg: *mut Object = msg_send![class!(NSString), stringWithUTF8String: UISCENE_DID_ENTER_BACKGROUND.as_ptr()];
            let _: () = msg_send![center, addObserver: observer
                selector: sel!(sceneDidEnterBackground:)
                name: did_enter_bg
                object: std::ptr::null::<Object>()];

            let will_enter_fg: *mut Object = msg_send![class!(NSString), stringWithUTF8String: UISCENE_WILL_ENTER_FOREGROUND.as_ptr()];
            let _: () = msg_send![center, addObserver: observer
                selector: sel!(sceneWillEnterForeground:)
                name: will_enter_fg
                object: std::ptr::null::<Object>()];

            self.0.lock().scene_observer = observer;
        }
    }
}

impl Drop for IosWindow {
    fn drop(&mut self) {
        log::info!("iOS window destroyed");
        unsafe {
            let mut state = self.0.lock();

            // Remove scene notification observer
            if !state.scene_observer.is_null() {
                let center: *mut Object =
                    msg_send![class!(NSNotificationCenter), defaultCenter];
                let _: () = msg_send![center, removeObserver: state.scene_observer];

                // Release the Rc held by the observer's ivar
                let ptr: *mut c_void = *(*state.scene_observer).get_ivar(WINDOW_STATE_IVAR);
                if !ptr.is_null() {
                    let _ = Rc::from_raw(ptr as *const Mutex<IosWindowState>);
                }
                let _: () = msg_send![state.scene_observer, release];
                state.scene_observer = std::ptr::null_mut();
            }

            // Release the Rc held by the GPUIView's ivar
            if !state.ui_view.is_null() {
                let ptr: *mut c_void = *(*state.ui_view).get_ivar(WINDOW_STATE_IVAR);
                if !ptr.is_null() {
                    let _ = Rc::from_raw(ptr as *const Mutex<IosWindowState>);
                    (*state.ui_view)
                        .set_ivar::<*mut c_void>(WINDOW_STATE_IVAR, std::ptr::null_mut());
                }
            }

            // Invalidate the CADisplayLink (removes it from the run loop).
            if !state.display_link.is_null() {
                let _: () = msg_send![state.display_link, invalidate];
                state.display_link = std::ptr::null_mut();
            }
            if !state.display_link_target.is_null() {
                let _: () = msg_send![state.display_link_target, release];
                state.display_link_target = std::ptr::null_mut();
            }
            // Free the leaked callback closure.
            if !state.display_link_callback_ptr.is_null() {
                let _ = Box::from_raw(state.display_link_callback_ptr as *mut Box<dyn Fn()>);
                state.display_link_callback_ptr = std::ptr::null_mut();
            }

            // Release blur view if present
            if !state.blur_view.is_null() {
                let _: () = msg_send![state.blur_view, removeFromSuperview];
                let _: () = msg_send![state.blur_view, release];
                state.blur_view = std::ptr::null_mut();
            }

            // Release gesture delegate
            if !state.gesture_delegate.is_null() {
                let _: () = msg_send![state.gesture_delegate, release];
                state.gesture_delegate = std::ptr::null_mut();
            }

            if !state.ui_view.is_null() {
                let _: () = msg_send![state.ui_view, release];
                state.ui_view = std::ptr::null_mut();
            }
            if !state.ui_view_controller.is_null() {
                let _: () = msg_send![state.ui_view_controller, release];
                state.ui_view_controller = std::ptr::null_mut();
            }
            if !state.ui_window.is_null() {
                let _: () = msg_send![state.ui_window, release];
                state.ui_window = std::ptr::null_mut();
            }
            if let Some(callback) = state.on_close.take() {
                callback();
            }
        }
    }
}

impl HasWindowHandle for IosWindow {
    fn window_handle(&self) -> std::result::Result<WindowHandle<'_>, HandleError> {
        let state = self.0.lock();
        let ui_view = NonNull::new(state.ui_view.cast::<c_void>()).ok_or(HandleError::Unavailable)?;
        let mut handle = UiKitWindowHandle::new(ui_view);
        handle.ui_view_controller = NonNull::new(state.ui_view_controller.cast::<c_void>());
        unsafe { Ok(WindowHandle::borrow_raw(handle.into())) }
    }
}

impl HasDisplayHandle for IosWindow {
    fn display_handle(&self) -> std::result::Result<DisplayHandle<'_>, HandleError> {
        Ok(DisplayHandle::uikit())
    }
}

impl PlatformWindow for IosWindow {
    fn bounds(&self) -> Bounds<Pixels> {
        self.0.lock().bounds
    }

    fn is_maximized(&self) -> bool {
        false
    }

    fn window_bounds(&self) -> WindowBounds {
        WindowBounds::Windowed(self.bounds())
    }

    fn content_size(&self) -> crate::Size<Pixels> {
        self.bounds().size
    }

    fn resize(&mut self, size: crate::Size<Pixels>) {
        // iOS manages view layout via UIKit; this just updates cached state
        // as a fallback for callers that set size programmatically.
        log::debug!("resize({:?}) — iOS manages layout via UIKit", size);
        self.0.lock().bounds.size = size;
    }

    fn scale_factor(&self) -> f32 {
        self.0.lock().scale_factor
    }

    fn appearance(&self) -> WindowAppearance {
        detect_system_appearance()
    }

    fn display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        Some(self.0.lock().display.clone())
    }

    fn mouse_position(&self) -> Point<Pixels> {
        self.0
            .lock()
            .last_touch_position
            .unwrap_or_else(Point::default)
    }

    fn modifiers(&self) -> Modifiers {
        Modifiers::default()
    }

    fn capslock(&self) -> crate::Capslock {
        crate::Capslock::default()
    }

    fn set_input_handler(&mut self, input_handler: PlatformInputHandler) {
        let mut lock = self.0.lock();
        let proxy = lock.keyboard_proxy;
        lock.input_handler = Some(input_handler);
        drop(lock);
        // Show keyboard by making the hidden UITextField first responder
        if !proxy.is_null() {
            unsafe {
                let _: () = msg_send![proxy, becomeFirstResponder];
            }
        }
    }

    fn take_input_handler(&mut self) -> Option<PlatformInputHandler> {
        let mut lock = self.0.lock();
        let handler = lock.input_handler.take();
        let proxy = lock.keyboard_proxy;
        drop(lock);
        if handler.is_some() && !proxy.is_null() {
            unsafe {
                let _: () = msg_send![proxy, resignFirstResponder];
            }
        }
        handler
    }

    fn prompt(
        &self,
        _level: crate::PromptLevel,
        _msg: &str,
        _detail: Option<&str>,
        _answers: &[PromptButton],
    ) -> Option<oneshot::Receiver<usize>> {
        None
    }

    fn activate(&self) {
        unsafe {
            let ui_window = self.0.lock().ui_window;
            let _: () = msg_send![ui_window, makeKeyAndVisible];
        }
    }

    fn is_active(&self) -> bool {
        self.0.lock().is_active
    }

    fn is_hovered(&self) -> bool {
        false
    }

    fn background_appearance(&self) -> WindowBackgroundAppearance {
        self.0.lock().background_appearance
    }

    fn set_title(&mut self, title: &str) {
        self.0.lock().title = title.to_string();
    }

    fn set_background_appearance(&self, background_appearance: WindowBackgroundAppearance) {
        let mut lock = self.0.lock();
        lock.background_appearance = background_appearance;

        unsafe {
            // Remove existing blur view if present
            if !lock.blur_view.is_null() {
                let _: () = msg_send![lock.blur_view, removeFromSuperview];
                let _: () = msg_send![lock.blur_view, release];
                lock.blur_view = std::ptr::null_mut();
            }

            match background_appearance {
                WindowBackgroundAppearance::Opaque => {
                    // Metal layer opaque
                    let layer: *mut Object = msg_send![lock.ui_view, layer];
                    let _: () = msg_send![layer, setOpaque: YES];
                }
                WindowBackgroundAppearance::Transparent => {
                    let layer: *mut Object = msg_send![lock.ui_view, layer];
                    let _: () = msg_send![layer, setOpaque: NO];
                }
                WindowBackgroundAppearance::Blurred => {
                    let layer: *mut Object = msg_send![lock.ui_view, layer];
                    let _: () = msg_send![layer, setOpaque: NO];

                    // Create UIVisualEffectView with system material blur
                    let effect: *mut Object = msg_send![class!(UIBlurEffect),
                        effectWithStyle: 6isize]; // UIBlurEffectStyleSystemMaterial
                    let blur_view: *mut Object =
                        msg_send![class!(UIVisualEffectView), alloc];
                    let blur_view: *mut Object =
                        msg_send![blur_view, initWithEffect: effect];

                    let bounds: CGRect = msg_send![lock.ui_view, bounds];
                    let _: () = msg_send![blur_view, setFrame: bounds];
                    // Auto-resize with parent
                    let autoresizing: usize = 0x3F; // FlexibleWidth | FlexibleHeight | all margins
                    let _: () =
                        msg_send![blur_view, setAutoresizingMask: autoresizing];

                    // Insert behind the Metal content
                    let _: () = msg_send![lock.ui_view, insertSubview: blur_view atIndex: 0isize];

                    lock.blur_view = blur_view;
                }
                // Windows-only variants — treat as opaque on iOS
                _ => {
                    let layer: *mut Object = msg_send![lock.ui_view, layer];
                    let _: () = msg_send![layer, setOpaque: YES];
                }
            }
        }
    }

    fn minimize(&self) {}

    fn zoom(&self) {}

    fn toggle_fullscreen(&self) {}

    fn is_fullscreen(&self) -> bool {
        false
    }

    fn on_request_frame(&self, callback: Box<dyn FnMut(RequestFrameOptions)>) {
        self.0.lock().request_frame = Some(callback);

        log::info!("CADisplayLink started");

        let window_state = self.0.clone();
        let first_frame_done = Rc::new(Cell::new(false));
        let first_frame_clone = first_frame_done.clone();

        let step_fn: Box<dyn Fn()> = Box::new(move || {
            // Process scroll momentum
            {
                let mut lock = window_state.lock();
                if let Some(ref mut momentum) = lock.scroll_momentum {
                    let now = Instant::now();
                    let dt_ms = now.duration_since(momentum.last_time).as_millis() as f32;
                    momentum.last_time = now;

                    // Exponential decay: v *= 0.998^dt_ms
                    let decay = 0.998f32.powf(dt_ms);
                    momentum.velocity.x *= decay;
                    momentum.velocity.y *= decay;

                    let vx = momentum.velocity.x;
                    let vy = momentum.velocity.y;
                    let position = momentum.position;
                    let dt_sec = dt_ms / 1000.0;

                    if vx.abs() < 0.5 && vy.abs() < 0.5 {
                        // Momentum exhausted — send final event
                        lock.scroll_momentum = None;
                        if let Some(mut input_cb) = lock.on_input.take() {
                            drop(lock);
                            input_cb(PlatformInput::ScrollWheel(ScrollWheelEvent {
                                position,
                                delta: ScrollDelta::Pixels(point(px(0.0), px(0.0))),
                                modifiers: Modifiers::default(),
                                touch_phase: TouchPhase::Ended,
                            }));
                            window_state.lock().on_input = Some(input_cb);
                        }
                    } else {
                        let dx = vx * dt_sec;
                        let dy = vy * dt_sec;
                        if let Some(mut input_cb) = lock.on_input.take() {
                            drop(lock);
                            input_cb(PlatformInput::ScrollWheel(ScrollWheelEvent {
                                position,
                                delta: ScrollDelta::Pixels(point(px(dx), px(dy))),
                                modifiers: Modifiers::default(),
                                touch_phase: TouchPhase::Moved,
                            }));
                            window_state.lock().on_input = Some(input_cb);
                        }
                    }
                }
            }

            let mut cb = match window_state.lock().request_frame.take() {
                Some(cb) => cb,
                None => return,
            };

            let mut opts = RequestFrameOptions::default();
            if !first_frame_clone.get() {
                first_frame_clone.set(true);
                log::info!("first frame rendered");
                opts.force_render = true;
            }

            cb(opts);
            window_state.lock().request_frame = Some(cb);
        });

        let boxed_fn = Box::new(step_fn);
        let fn_ptr = Box::into_raw(boxed_fn) as *mut c_void;

        unsafe {
            let target: *mut Object = msg_send![DISPLAY_LINK_TARGET_CLASS, new];
            (*target).set_ivar::<*mut c_void>(CALLBACK_IVAR, fn_ptr);

            let display_link: *mut Object = msg_send![
                class!(CADisplayLink),
                displayLinkWithTarget: target
                selector: sel!(step:)
            ];

            let run_loop: *mut Object = msg_send![class!(NSRunLoop), mainRunLoop];
            let _: () = msg_send![display_link, addToRunLoop: run_loop forMode: NSRunLoopCommonModes];

            let mut state = self.0.lock();
            state.display_link = display_link;
            state.display_link_target = target;
            state.display_link_callback_ptr = fn_ptr;
        }
    }

    fn on_input(&self, callback: Box<dyn FnMut(PlatformInput) -> DispatchEventResult>) {
        self.0.lock().on_input = Some(callback);
    }

    fn on_active_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        self.0.lock().on_active_change = Some(callback);
    }

    fn on_hover_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        self.0.lock().on_hover_change = Some(callback);
    }

    fn on_resize(&self, callback: Box<dyn FnMut(crate::Size<Pixels>, f32)>) {
        self.0.lock().on_resize = Some(callback);
    }

    fn on_moved(&self, callback: Box<dyn FnMut()>) {
        self.0.lock().on_moved = Some(callback);
    }

    fn on_should_close(&self, callback: Box<dyn FnMut() -> bool>) {
        self.0.lock().should_close = Some(callback);
    }

    fn on_hit_test_window_control(&self, callback: Box<dyn FnMut() -> Option<WindowControlArea>>) {
        self.0.lock().on_hit_test_window_control = Some(callback);
    }

    fn on_close(&self, callback: Box<dyn FnOnce()>) {
        self.0.lock().on_close = Some(callback);
    }

    fn on_appearance_changed(&self, callback: Box<dyn FnMut()>) {
        self.0.lock().on_appearance_change = Some(callback);
    }

    fn draw(&self, scene: &crate::Scene) {
        self.0.lock().renderer.draw(scene);
    }

    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        self.0.lock().renderer.sprite_atlas().clone()
    }

    fn is_subpixel_rendering_supported(&self) -> bool {
        false
    }

    fn gpu_specs(&self) -> Option<GpuSpecs> {
        None
    }

    fn shared_render_resources(&self) -> Arc<SharedRenderResources> {
        self.0.lock().renderer.shared().clone()
    }

    fn update_ime_position(&self, _bounds: Bounds<Pixels>) {}

    fn raw_native_view_ptr(&self) -> *mut c_void {
        self.0.lock().ui_view.cast::<c_void>()
    }
}

// ---------------------------------------------------------------------------
// Security framework bindings for Keychain access (identical API on iOS/macOS)
// ---------------------------------------------------------------------------

mod ios_security {
    #![allow(non_upper_case_globals)]

    use core_foundation::{
        base::{CFTypeRef, OSStatus},
        dictionary::CFDictionaryRef,
        string::CFStringRef,
    };

    #[link(name = "Security", kind = "framework")]
    unsafe extern "C" {
        pub static kSecClass: CFStringRef;
        pub static kSecClassInternetPassword: CFStringRef;
        pub static kSecAttrServer: CFStringRef;
        pub static kSecAttrAccount: CFStringRef;
        pub static kSecValueData: CFStringRef;
        pub static kSecReturnAttributes: CFStringRef;
        pub static kSecReturnData: CFStringRef;

        pub fn SecItemAdd(attributes: CFDictionaryRef, result: *mut CFTypeRef) -> OSStatus;
        pub fn SecItemUpdate(query: CFDictionaryRef, attributes: CFDictionaryRef) -> OSStatus;
        pub fn SecItemDelete(query: CFDictionaryRef) -> OSStatus;
        pub fn SecItemCopyMatching(query: CFDictionaryRef, result: *mut CFTypeRef) -> OSStatus;
    }

    pub const errSecSuccess: OSStatus = 0;
    pub const errSecUserCanceled: OSStatus = -128;
    pub const errSecItemNotFound: OSStatus = -25300;
}

// ---------------------------------------------------------------------------
// Public entry point for URL scheme handling from Swift
// ---------------------------------------------------------------------------

/// Called from Swift's `application(_:open:options:)` to forward URL opens
/// into the GPUI platform callback system.
///
/// # Safety
/// `url_ptr` must be a valid null-terminated C string pointer.
/// Must be called on the main thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gpui_ios_handle_open_url(url_ptr: *const std::os::raw::c_char) {
    if url_ptr.is_null() {
        return;
    }
    let url = unsafe { std::ffi::CStr::from_ptr(url_ptr) }
        .to_string_lossy()
        .into_owned();

    unsafe {
        let ptr = IOS_PLATFORM_STATE_PTR.load(std::sync::atomic::Ordering::Acquire);
        if !ptr.is_null() {
            let state = &*(ptr as *const Mutex<IosPlatformState>);
            let mut lock = state.lock();
            if let Some(ref mut callback) = lock.open_urls {
                callback(vec![url]);
            }
        }
    }
}

/// Global pointer to the IosPlatformState, set during IosPlatform::new.
/// Used by gpui_ios_handle_open_url to fire the callback from Swift.
static IOS_PLATFORM_STATE_PTR: std::sync::atomic::AtomicPtr<c_void> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
