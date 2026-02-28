#[cfg(feature = "font-kit")]
mod open_type;
#[cfg(feature = "font-kit")]
mod text_system;

use crate::{
    Action, AnyWindowHandle, BackgroundExecutor, Bounds, ClipboardItem, CursorStyle,
    DispatchEventResult, DisplayId, DummyKeyboardMapper, ForegroundExecutor, GLOBAL_THREAD_TIMINGS,
    GpuSpecs, Keymap, Menu, MenuItem, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, NoopTextSystem, OwnedMenu, PathPromptOptions, Pixels, Platform, PlatformAtlas,
    PlatformDispatcher, PlatformDisplay, PlatformInput, PlatformInputHandler,
    PlatformKeyboardLayout, PlatformKeyboardMapper, PlatformTextSystem, PlatformWindow, Point,
    Priority, PromptButton, RequestFrameOptions, ScrollDelta, ScrollWheelEvent, Task, TaskTiming,
    ThermalState, THREAD_TIMINGS, ThreadTaskTimings, TouchPhase, WindowAppearance,
    WindowBackgroundAppearance, WindowBounds, WindowControlArea, WindowParams, point, px, size,
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
use std::{
    cell::Cell,
    ffi::c_void,
    path::{Path, PathBuf},
    ptr::{NonNull, addr_of},
    rc::Rc,
    sync::Arc,
    thread,
    time::Duration,
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

    // UITextFieldDelegate — GPUIView acts as delegate for the hidden
    // UITextField keyboard proxy to intercept typed text.
    decl.add_method(
        sel!(textField:shouldChangeCharactersInRange:replacementString:),
        handle_text_field_change as extern "C" fn(&Object, Sel, *mut Object, NSRange, *mut Object) -> BOOL,
    );
    decl.add_method(
        sel!(textFieldShouldReturn:),
        handle_text_field_return as extern "C" fn(&Object, Sel, *mut Object) -> BOOL,
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

fn dispatch_input(state: &Mutex<IosWindowState>, input: PlatformInput) {
    let mut lock = state.lock();
    if let Some(mut callback) = lock.on_input.take() {
        drop(lock);
        callback(input);
        state.lock().on_input = Some(callback);
    }
}

unsafe extern "C" fn become_first_responder_trampoline(context: *mut c_void) {
    let view = context as *mut Object;
    let _: BOOL = msg_send![view, becomeFirstResponder];
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
    let Some((position, click_count)) = (unsafe { primary_touch_info(touches, this, &state) })
    else {
        return;
    };

    dispatch_input(
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
    text_field: *mut Object,
    _range: NSRange,
    replacement: *mut Object,
) -> BOOL {
    let Some(state) = (unsafe { get_window_state(this) }) else {
        return NO;
    };
    unsafe {
        let utf8: *const std::os::raw::c_char = msg_send![replacement, UTF8String];
        if utf8.is_null() {
            return NO;
        }
        let text = std::ffi::CStr::from_ptr(utf8).to_string_lossy().into_owned();

        if text.is_empty() {
            // Empty replacement = backspace / delete
            dispatch_input(
                &state,
                PlatformInput::KeyDown(crate::KeyDownEvent {
                    keystroke: crate::Keystroke {
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

        // No input handler — dispatch as KeyDown
        dispatch_input(
            &state,
            PlatformInput::KeyDown(crate::KeyDownEvent {
                keystroke: crate::Keystroke {
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
    // Return NO so UITextField stays empty — all text is handled by GPUI
    NO
}

extern "C" fn handle_text_field_return(
    this: &Object,
    _sel: Sel,
    _text_field: *mut Object,
) -> BOOL {
    let Some(state) = (unsafe { get_window_state(this) }) else {
        return NO;
    };
    dispatch_input(
        &state,
        PlatformInput::KeyDown(crate::KeyDownEvent {
            keystroke: crate::Keystroke {
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
pub(crate) struct IosKeyboardLayout;

impl PlatformKeyboardLayout for IosKeyboardLayout {
    fn id(&self) -> &str {
        "ios"
    }

    fn name(&self) -> &str {
        "iOS"
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
        Self {
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
                app_menu_action: None,
                will_open_menu: None,
                validate_app_menu: None,
                keyboard_layout_change: None,
            }),
        }
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

    fn open_url(&self, _url: &str) {}

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

    // TODO: Needs an ObjC observer class for NSProcessInfoThermalStateDidChangeNotification (low priority)
    fn on_thermal_state_change(&self, _callback: Box<dyn FnMut()>) {}

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
        None
    }

    fn write_to_clipboard(&self, _item: ClipboardItem) {}

    fn write_credentials(&self, _url: &str, _username: &str, _password: &[u8]) -> Task<Result<()>> {
        Task::ready(Err(anyhow!("credential storage is not implemented on iOS")))
    }

    fn read_credentials(&self, _url: &str) -> Task<Result<Option<(String, Vec<u8>)>>> {
        Task::ready(Ok(None))
    }

    fn delete_credentials(&self, _url: &str) -> Task<Result<()>> {
        Task::ready(Ok(()))
    }

    fn keyboard_layout(&self) -> Box<dyn PlatformKeyboardLayout> {
        Box::new(IosKeyboardLayout)
    }

    fn keyboard_mapper(&self) -> Rc<dyn PlatformKeyboardMapper> {
        Rc::new(DummyKeyboardMapper)
    }

    fn on_keyboard_layout_change(&self, callback: Box<dyn FnMut()>) {
        self.state.lock().keyboard_layout_change = Some(callback);
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
    renderer: MetalRenderer,
    // CADisplayLink driving the frame loop
    display_link: *mut Object,
    display_link_target: *mut Object,
    display_link_callback_ptr: *mut c_void,
    // Hidden UITextField used as keyboard proxy (UIKeyInput on custom UIView
    // doesn't reliably show the keyboard with the `objc` crate's add_protocol)
    keyboard_proxy: *mut Object,
    // Touch tracking — primary finger only
    tracked_touch: Option<*mut Object>,
    last_touch_position: Option<Point<Pixels>>,
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
        let (ui_window, ui_view_controller, ui_view, keyboard_proxy, bounds, scale_factor) = unsafe {
            let screen: *mut Object = msg_send![class!(UIScreen), mainScreen];
            let screen_bounds: CGRect = msg_send![screen, bounds];
            let scale: f64 = msg_send![screen, scale];

            // On iOS 13+, UIWindow must be associated with a UIWindowScene.
            // Find the first connected UIWindowScene and use initWithWindowScene:.
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

            // Use our custom GPUIView instead of plain UIView.
            // Its layerClass override returns CAMetalLayer, so [view layer]
            // IS the Metal layer — no need to add a sublayer separately.
            let ui_view: *mut Object = msg_send![GPUI_VIEW_CLASS, alloc];
            let ui_view: *mut Object = msg_send![ui_view, initWithFrame: screen_bounds];

            // Enable multi-touch for scroll gestures and future multi-finger input
            let _: () = msg_send![ui_view, setMultipleTouchEnabled: YES];

            // Hidden UITextField as keyboard proxy — shows software keyboard
            // when it becomes first responder. Must have non-zero size and
            // non-zero alpha (hidden views can't become first responder).
            let proxy_frame = CGRect { origin: CGPoint { x: 0.0, y: 0.0 }, size: CGSize { width: 1.0, height: 1.0 } };
            let keyboard_proxy: *mut Object = msg_send![class!(UITextField), alloc];
            let keyboard_proxy: *mut Object = msg_send![keyboard_proxy, initWithFrame: proxy_frame];
            let _: () = msg_send![keyboard_proxy, setAlpha: 0.01f64];
            // Disable autocorrection and autocapitalization
            let _: () = msg_send![keyboard_proxy, setAutocorrectionType: 1isize]; // UITextAutocorrectionTypeNo
            let _: () = msg_send![keyboard_proxy, setAutocapitalizationType: 0isize]; // UITextAutocapitalizationTypeNone
            let _: () = msg_send![keyboard_proxy, setSpellCheckingType: 1isize]; // UITextSpellCheckingTypeNo
            let _: () = msg_send![ui_view, addSubview: keyboard_proxy];

            // Two-finger pan gesture for scroll
            let pan: *mut Object = msg_send![class!(UIPanGestureRecognizer), alloc];
            let pan: *mut Object = msg_send![pan, initWithTarget: ui_view action: sel!(handleScrollPan:)];
            let _: () = msg_send![pan, setMinimumNumberOfTouches: 2usize];
            let _: () = msg_send![pan, setMaximumNumberOfTouches: 2usize];
            let _: () = msg_send![ui_view, addGestureRecognizer: pan];

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
            (ui_window, ui_view_controller, ui_view, keyboard_proxy, bounds, scale as f32)
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
            renderer,
            keyboard_proxy,
            display_link: std::ptr::null_mut(),
            display_link_target: std::ptr::null_mut(),
            display_link_callback_ptr: std::ptr::null_mut(),
            tracked_touch: None,
            last_touch_position: None,
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

            // Set the GPUIView as the delegate for the keyboard proxy UITextField.
            // When the user types, textField:shouldChangeCharactersInRange:replacementString:
            // on the GPUIView intercepts the text and forwards it to GPUI.
            let _: () = msg_send![keyboard_proxy, setDelegate: ui_view];
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
        // Show the software keyboard by making the hidden UITextField first responder
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
        WindowBackgroundAppearance::Opaque
    }

    fn set_title(&mut self, title: &str) {
        self.0.lock().title = title.to_string();
    }

    fn set_background_appearance(&self, _background_appearance: WindowBackgroundAppearance) {}

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
