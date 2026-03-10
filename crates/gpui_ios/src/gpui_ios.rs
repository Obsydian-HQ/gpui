#![cfg(target_os = "ios")]

pub(crate) mod native_controls;
mod ios_native_controls;

#[cfg(feature = "font-kit")]
mod open_type;
#[cfg(feature = "font-kit")]
mod text_system;

use gpui::hash;
use gpui_metal::{InstanceBufferPool, MetalRenderer, SharedRenderResources};
use ios_native_controls::IOS_NATIVE_CONTROLS;
use gpui::{
    Action, AnyWindowHandle, BackgroundExecutor, Bounds, ClipboardEntry, ClipboardItem,
    Capslock, CursorStyle, DevicePixels, DispatchEventResult, DisplayId, Edges, ExternalPaths,
    FileDropEvent, ForegroundExecutor, GLOBAL_THREAD_TIMINGS, GpuSpecs, HostedContentConfig,
    Image, ImageFormat, KeyDownEvent, KeyUpEvent, KeybindingKeystroke, Keymap, Keystroke, Menu,
    MenuItem, Modifiers, ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, OwnedMenu, PathPromptOptions, PinchEvent, Pixels, Platform, PlatformAtlas,
    PlatformDispatcher, PlatformDisplay, PlatformInput, PlatformInputHandler,
    PlatformKeyboardLayout, PlatformKeyboardMapper, PlatformTextSystem, PlatformWindow, Point,
    Priority, PromptButton, PromptLevel, RequestFrameOptions, RotationEvent, RunnableVariant,
    Scene, ScrollDelta, ScrollWheelEvent, Size, THREAD_TIMINGS, Task, TaskTiming, ThermalState,
    ThreadTaskTimings, TouchPhase, WindowAppearance, WindowBackgroundAppearance, WindowBounds,
    WindowControlArea, WindowParams, point, px, size,
};
use anyhow::{Result, anyhow};
use block::ConcreteBlock;
use collections::HashMap;
use core_foundation::{
    base::{CFType, CFTypeRef, TCFType},
    boolean::CFBoolean,
    data::CFData,
    dictionary::{CFDictionary, CFMutableDictionary},
    string::CFString,
};
use ctor::ctor;
use foreign_types::ForeignType as _;
use futures::channel::oneshot;
use metal::{CAMetalLayer, MetalLayer};
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{BOOL, Class, NO, Object, Sel, YES},
    sel, sel_impl,
};
use parking_lot::Mutex;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, UiKitWindowHandle, WindowHandle,
};
use std::{
    cell::Cell,
    ffi::c_void,
    ops::Range,
    path::{Path, PathBuf},
    ptr::{self, NonNull, addr_of},
    rc::Rc,
    sync::{Arc, atomic::Ordering},
    thread,
    time::{Duration, Instant},
};
#[cfg(feature = "font-kit")]
use text_system::IosTextSystem;
#[cfg(not(feature = "font-kit"))]
use gpui::NoopTextSystem;

type DispatchQueue = *mut c_void;
type DispatchTime = u64;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct NSRange {
    location: usize,
    length: usize,
}

impl NSRange {
    fn is_valid(&self) -> bool {
        self.location != usize::MAX
    }

    fn to_range(self) -> Option<Range<usize>> {
        if self.is_valid() {
            Some(self.location..self.location + self.length)
        } else {
            None
        }
    }
}

impl From<Range<usize>> for NSRange {
    fn from(range: Range<usize>) -> Self {
        Self {
            location: range.start,
            length: range.end - range.start,
        }
    }
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

const TEXT_POSITION_INDEX_IVAR: &str = "gpui_index";

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
// GPUITextPosition — UITextPosition subclass storing a UTF-16 character index.
// Required by the UITextInput protocol for position arithmetic.
// ---------------------------------------------------------------------------

static mut GPUI_TEXT_POSITION_CLASS: *const Class = std::ptr::null();

#[ctor]
fn register_text_position_class() {
    unsafe {
        let superclass = class!(UITextPosition);
        let mut decl = ClassDecl::new("GPUITextPosition", superclass)
            .expect("failed to declare GPUITextPosition class");
        decl.add_ivar::<usize>(TEXT_POSITION_INDEX_IVAR);
        GPUI_TEXT_POSITION_CLASS = decl.register();
    }
}

unsafe fn make_text_position(index: usize) -> *mut Object {
    unsafe {
        let obj: *mut Object = msg_send![GPUI_TEXT_POSITION_CLASS, alloc];
        let obj: *mut Object = msg_send![obj, init];
        (*obj).set_ivar::<usize>(TEXT_POSITION_INDEX_IVAR, index);
        obj
    }
}

unsafe fn text_position_index(position: *mut Object) -> usize { unsafe {
    if position.is_null() {
        return 0;
    }
    *(*position).get_ivar::<usize>(TEXT_POSITION_INDEX_IVAR)
}}

// ---------------------------------------------------------------------------
// GPUITextRange — UITextRange subclass storing start/end UTF-16 indices.
// Required by the UITextInput protocol for range operations.
// ---------------------------------------------------------------------------

static mut GPUI_TEXT_RANGE_CLASS: *const Class = std::ptr::null();
const TEXT_RANGE_START_IVAR: &str = "gpui_start";
const TEXT_RANGE_END_IVAR: &str = "gpui_end";

#[ctor]
fn register_text_range_class() {
    unsafe {
        let superclass = class!(UITextRange);
        let mut decl = ClassDecl::new("GPUITextRange", superclass)
            .expect("failed to declare GPUITextRange class");
        decl.add_ivar::<usize>(TEXT_RANGE_START_IVAR);
        decl.add_ivar::<usize>(TEXT_RANGE_END_IVAR);

        decl.add_method(
            sel!(isEmpty),
            text_range_is_empty as extern "C" fn(&Object, Sel) -> BOOL,
        );
        decl.add_method(
            sel!(start),
            text_range_start as extern "C" fn(&Object, Sel) -> *mut Object,
        );
        decl.add_method(
            sel!(end),
            text_range_end as extern "C" fn(&Object, Sel) -> *mut Object,
        );

        GPUI_TEXT_RANGE_CLASS = decl.register();
    }
}

unsafe fn make_text_range(start: usize, end: usize) -> *mut Object {
    unsafe {
        let obj: *mut Object = msg_send![GPUI_TEXT_RANGE_CLASS, alloc];
        let obj: *mut Object = msg_send![obj, init];
        (*obj).set_ivar::<usize>(TEXT_RANGE_START_IVAR, start);
        (*obj).set_ivar::<usize>(TEXT_RANGE_END_IVAR, end);
        obj
    }
}

unsafe fn text_range_to_rust(range: *mut Object) -> Option<Range<usize>> { unsafe {
    if range.is_null() {
        return None;
    }
    let start = *(*range).get_ivar::<usize>(TEXT_RANGE_START_IVAR);
    let end = *(*range).get_ivar::<usize>(TEXT_RANGE_END_IVAR);
    Some(start..end)
}}

extern "C" fn text_range_is_empty(this: &Object, _sel: Sel) -> BOOL {
    unsafe {
        let start = *this.get_ivar::<usize>(TEXT_RANGE_START_IVAR);
        let end = *this.get_ivar::<usize>(TEXT_RANGE_END_IVAR);
        if start == end { YES } else { NO }
    }
}

extern "C" fn text_range_start(this: &Object, _sel: Sel) -> *mut Object {
    unsafe {
        let start = *this.get_ivar::<usize>(TEXT_RANGE_START_IVAR);
        make_text_position(start)
    }
}

extern "C" fn text_range_end(this: &Object, _sel: Sel) -> *mut Object {
    unsafe {
        let end = *this.get_ivar::<usize>(TEXT_RANGE_END_IVAR);
        make_text_position(end)
    }
}

// ---------------------------------------------------------------------------
// CADisplayLink target — an ObjC class whose `step:` method drives the frame
// loop on iOS, equivalent to CVDisplayLink on macOS.
// ---------------------------------------------------------------------------

static mut DISPLAY_LINK_TARGET_CLASS: *const Class = std::ptr::null();

#[ctor]
fn register_display_link_target_class() {
    unsafe {
        let superclass = class!(NSObject);
        let mut decl = ClassDecl::new("GPUIDisplayLinkTarget", superclass)
            .expect("failed to declare GPUIDisplayLinkTarget class");
        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);
        decl.add_method(
            sel!(step:),
            display_link_step as extern "C" fn(&Object, Sel, *mut Object),
        );
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
fn register_gpui_view_class() {
    unsafe {
        let superclass = class!(UIView);
        let mut decl =
            ClassDecl::new("GPUIView", superclass).expect("failed to declare GPUIView class");

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

    // UIKeyInput (base of UITextInput) — first responder + basic text ops
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

    // UITextInput — full text input protocol for IME composition, marked
    // text, cursor positioning, and text selection.
    decl.add_ivar::<*mut c_void>("gpui_input_delegate");
    decl.add_ivar::<*mut c_void>("gpui_tokenizer");

    decl.add_method(
        sel!(textInRange:),
        uitextinput_text_in_range as extern "C" fn(&Object, Sel, *mut Object) -> *mut Object,
    );
    decl.add_method(
        sel!(replaceRange:withText:),
        uitextinput_replace_range as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
    );
    decl.add_method(
        sel!(selectedTextRange),
        uitextinput_selected_text_range as extern "C" fn(&Object, Sel) -> *mut Object,
    );
    decl.add_method(
        sel!(setSelectedTextRange:),
        uitextinput_set_selected_text_range as extern "C" fn(&Object, Sel, *mut Object),
    );
    decl.add_method(
        sel!(markedTextRange),
        uitextinput_marked_text_range as extern "C" fn(&Object, Sel) -> *mut Object,
    );
    decl.add_method(
        sel!(markedTextStyle),
        uitextinput_marked_text_style as extern "C" fn(&Object, Sel) -> *mut Object,
    );
    decl.add_method(
        sel!(setMarkedTextStyle:),
        uitextinput_set_marked_text_style as extern "C" fn(&Object, Sel, *mut Object),
    );
    decl.add_method(
        sel!(setMarkedText:selectedRange:),
        uitextinput_set_marked_text
            as extern "C" fn(&Object, Sel, *mut Object, NSRange),
    );
    decl.add_method(
        sel!(unmarkText),
        uitextinput_unmark_text as extern "C" fn(&Object, Sel),
    );
    decl.add_method(
        sel!(beginningOfDocument),
        uitextinput_beginning_of_document as extern "C" fn(&Object, Sel) -> *mut Object,
    );
    decl.add_method(
        sel!(endOfDocument),
        uitextinput_end_of_document as extern "C" fn(&Object, Sel) -> *mut Object,
    );
    decl.add_method(
        sel!(textRangeFromPosition:toPosition:),
        uitextinput_text_range_from_position
            as extern "C" fn(&Object, Sel, *mut Object, *mut Object) -> *mut Object,
    );
    decl.add_method(
        sel!(positionFromPosition:offset:),
        uitextinput_position_from_position_offset
            as extern "C" fn(&Object, Sel, *mut Object, isize) -> *mut Object,
    );
    decl.add_method(
        sel!(positionFromPosition:inDirection:offset:),
        uitextinput_position_from_position_direction
            as extern "C" fn(&Object, Sel, *mut Object, isize, isize) -> *mut Object,
    );
    decl.add_method(
        sel!(comparePosition:toPosition:),
        uitextinput_compare_position
            as extern "C" fn(&Object, Sel, *mut Object, *mut Object) -> isize,
    );
    decl.add_method(
        sel!(offsetFromPosition:toPosition:),
        uitextinput_offset_from_position
            as extern "C" fn(&Object, Sel, *mut Object, *mut Object) -> isize,
    );
    decl.add_method(
        sel!(inputDelegate),
        uitextinput_input_delegate as extern "C" fn(&Object, Sel) -> *mut Object,
    );
    decl.add_method(
        sel!(setInputDelegate:),
        uitextinput_set_input_delegate as extern "C" fn(&Object, Sel, *mut Object),
    );
    decl.add_method(
        sel!(tokenizer),
        uitextinput_tokenizer as extern "C" fn(&Object, Sel) -> *mut Object,
    );
    decl.add_method(
        sel!(positionWithinRange:farthestInDirection:),
        uitextinput_position_within_range
            as extern "C" fn(&Object, Sel, *mut Object, isize) -> *mut Object,
    );
    decl.add_method(
        sel!(characterRangeByExtendingPosition:inDirection:),
        uitextinput_character_range_by_extending
            as extern "C" fn(&Object, Sel, *mut Object, isize) -> *mut Object,
    );
    decl.add_method(
        sel!(baseWritingDirectionForPosition:inDirection:),
        uitextinput_base_writing_direction
            as extern "C" fn(&Object, Sel, *mut Object, isize) -> isize,
    );
    decl.add_method(
        sel!(setBaseWritingDirection:forRange:),
        uitextinput_set_base_writing_direction
            as extern "C" fn(&Object, Sel, isize, *mut Object),
    );
    decl.add_method(
        sel!(firstRectForRange:),
        uitextinput_first_rect_for_range
            as extern "C" fn(&Object, Sel, *mut Object) -> CGRect,
    );
    decl.add_method(
        sel!(caretRectForPosition:),
        uitextinput_caret_rect_for_position
            as extern "C" fn(&Object, Sel, *mut Object) -> CGRect,
    );
    decl.add_method(
        sel!(selectionRectsForRange:),
        uitextinput_selection_rects_for_range
            as extern "C" fn(&Object, Sel, *mut Object) -> *mut Object,
    );
    decl.add_method(
        sel!(closestPositionToPoint:),
        uitextinput_closest_position_to_point
            as extern "C" fn(&Object, Sel, CGPoint) -> *mut Object,
    );
    decl.add_method(
        sel!(closestPositionToPoint:withinRange:),
        uitextinput_closest_position_to_point_within_range
            as extern "C" fn(&Object, Sel, CGPoint, *mut Object) -> *mut Object,
    );
    decl.add_method(
        sel!(characterRangeAtPoint:),
        uitextinput_character_range_at_point
            as extern "C" fn(&Object, Sel, CGPoint) -> *mut Object,
    );

    // iPadOS hover gesture (pointer support)
    decl.add_method(
        sel!(handleHover:),
        handle_hover as extern "C" fn(&Object, Sel, *mut Object),
    );

    // Long-press gesture (simulates right-click for context menus)
    decl.add_method(
        sel!(handleLongPress:),
        handle_long_press as extern "C" fn(&Object, Sel, *mut Object),
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

        GPUI_VIEW_CLASS = decl.register();
    }
}

extern "C" fn gpui_view_layer_class(_self: &Class, _sel: Sel) -> *const Class {
    class!(CAMetalLayer)
}

/// Recover the `Rc<Mutex<IosWindowState>>` from the view's ivar without
/// consuming the Rc (the ivar still holds its reference).
unsafe fn get_window_state(view: &Object) -> Option<Rc<Mutex<IosWindowState>>> { unsafe {
    let ptr: *mut c_void = *view.get_ivar(WINDOW_STATE_IVAR);
    if ptr.is_null() {
        return None;
    }
    let rc = Rc::from_raw(ptr as *const Mutex<IosWindowState>);
    let clone = rc.clone();
    std::mem::forget(rc); // Don't drop — ivar still holds it
    Some(clone)
}}

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
    log::debug!(
        "[keyboard] become_first_responder_trampoline fired — calling becomeFirstResponder on proxy"
    );
    let result: BOOL = msg_send![view, becomeFirstResponder];
    log::debug!("[keyboard] becomeFirstResponder returned: {}", result != NO);
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

    let modifiers = state.lock().current_modifiers;
    log::trace!(
        "[touch] touchesBegan at ({}, {}), click_count={}",
        position.x.to_f64(),
        position.y.to_f64(),
        click_count
    );
    let result = dispatch_input(
        &state,
        PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers,
            click_count,
            first_mouse: false,
        }),
    );
    log::trace!(
        "[touch] dispatch_input result: propagate={}, default_prevented={}",
        result.propagate,
        result.default_prevented
    );

    // Only show the software keyboard if the touch was handled by an input
    // element (default_prevented means an input field consumed the event).
    let has_input_handler = state.lock().input_handler.is_some();
    if has_input_handler && result.default_prevented {
        unsafe {
            let view_ptr = this as *const Object as *mut Object;
            let is_first: BOOL = msg_send![view_ptr, isFirstResponder];
            log::debug!(
                "[keyboard] requesting first responder (currently_first={})",
                is_first != NO
            );
            if is_first == NO {
                dispatch_async_f(
                    dispatch_get_main_queue_ptr(),
                    view_ptr as *mut c_void,
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

    let modifiers = state.lock().current_modifiers;
    dispatch_input(
        &state,
        PlatformInput::MouseMove(MouseMoveEvent {
            position,
            pressed_button: Some(MouseButton::Left),
            modifiers,
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

    // Clear tracked touch and grab modifiers
    let modifiers = {
        let mut lock = state.lock();
        lock.tracked_touch = None;
        lock.current_modifiers
    };

    dispatch_input(
        &state,
        PlatformInput::MouseUp(MouseUpEvent {
            button: MouseButton::Left,
            position,
            modifiers,
            click_count,
        }),
    );
}

extern "C" fn handle_touches_cancelled(
    this: &Object,
    _sel: Sel,
    _touches: *mut Object,
    _event: *mut Object,
) {
    let Some(state) = (unsafe { get_window_state(this) }) else {
        return;
    };

    // Use last known position or zero, clear tracked touch, grab modifiers
    let (position, modifiers) = {
        let mut lock = state.lock();
        let pos = lock.last_touch_position.unwrap_or_else(Point::default);
        lock.tracked_touch = None;
        (pos, lock.current_modifiers)
    };

    dispatch_input(
        &state,
        PlatformInput::MouseUp(MouseUpEvent {
            button: MouseButton::Left,
            position,
            modifiers,
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

        let new_size = Size {
            width: px(bounds.size.width as f32),
            height: px(bounds.size.height as f32),
        };
        let scale_factor = scale as f32;
        let device_width = f32::from(new_size.width) * scale_factor;
        let device_height = f32::from(new_size.height) * scale_factor;

        // Read safe area insets while we have the view reference — they may change on rotation
        let insets: UIEdgeInsets = msg_send![this, safeAreaInsets];

        let mut lock = state.lock();
        let size_changed = lock.bounds.size != new_size || lock.scale_factor != scale_factor;
        if !size_changed {
            // Still update insets in case they changed without a size change
            lock.safe_area_insets = insets;
            return;
        }

        lock.bounds.size = new_size;
        lock.scale_factor = scale_factor;
        lock.safe_area_insets = insets;

        // The view's layer IS the Metal layer (via replace_layer), so UIKit
        // auto-sizes it. Just update the drawable size for rendering.
        lock.renderer.update_drawable_size(size(
            DevicePixels(device_width as i32),
            DevicePixels(device_height as i32),
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
            let previous_style: isize = msg_send![_previous_trait_collection, userInterfaceStyle];
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
// UIKeyInput — GPUIView is the first responder for hardware keyboard input.
// ---------------------------------------------------------------------------

extern "C" fn can_become_first_responder(_this: &Object, _sel: Sel) -> BOOL {
    log::trace!(
        "[keyboard] GPUIView canBecomeFirstResponder called — returning YES (for hardware keyboard)"
    );
    YES
}

extern "C" fn has_text(this: &Object, _sel: Sel) -> BOOL {
    let Some(state) = (unsafe { get_window_state(this) }) else {
        return NO;
    };
    if state.lock().input_handler.is_some() {
        YES
    } else {
        NO
    }
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
        let text = std::ffi::CStr::from_ptr(utf8)
            .to_string_lossy()
            .into_owned();
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

        // Route through the input handler (UITextInput path) — this also
        // commits any pending marked text (IME composition).
        if with_input_handler(this, |handler| {
            handler.replace_text_in_range(None, &text);
        })
        .is_some()
        {
            return;
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
extern "C" fn keyboard_type(_this: &Object, _sel: Sel) -> isize {
    0
} // UIKeyboardTypeDefault
extern "C" fn autocorrection_type(_this: &Object, _sel: Sel) -> isize {
    1
} // UITextAutocorrectionTypeNo
extern "C" fn autocapitalization_type(_this: &Object, _sel: Sel) -> isize {
    0
} // UITextAutocapitalizationTypeNone
extern "C" fn spell_checking_type(_this: &Object, _sel: Sel) -> isize {
    1
} // UITextSpellCheckingTypeNo

// ---------------------------------------------------------------------------
// UITextInput — full text input protocol implementation.
// Mirrors the macOS NSTextInputClient pattern: each ObjC method delegates
// to PlatformInputHandler through `with_input_handler`.
// ---------------------------------------------------------------------------

/// Takes the input handler out of the window state, calls the closure with it,
/// then puts it back. This avoids holding the lock while the handler runs,
/// which prevents deadlocks when the handler calls back into GPUI.
fn with_input_handler<F, R>(view: &Object, f: F) -> Option<R>
where
    F: FnOnce(&mut PlatformInputHandler) -> R,
{
    let state = unsafe { get_window_state(view)? };
    let mut lock = state.lock();
    if let Some(mut input_handler) = lock.input_handler.take() {
        drop(lock);
        let result = f(&mut input_handler);
        state.lock().input_handler = Some(input_handler);
        Some(result)
    } else {
        None
    }
}

/// Get the total document length in UTF-16 characters. Returns 0 if no
/// input handler is active or the handler doesn't provide text.
fn document_length_utf16(view: &Object) -> usize {
    with_input_handler(view, |handler| {
        // Query text_for_range with a large-but-bounded range. The handler
        // will clamp to the actual document bounds and return the adjusted
        // range, whose end gives us the document length.
        let mut adjusted = None;
        handler.text_for_range(0..1_000_000, &mut adjusted);
        adjusted.map_or_else(
            || {
                // Fallback: use the selection end as a lower bound estimate
                handler
                    .selected_text_range(false)
                    .map_or(0, |sel| sel.range.end)
            },
            |r| r.end,
        )
    })
    .unwrap_or(0)
}

extern "C" fn uitextinput_text_in_range(
    this: &Object,
    _sel: Sel,
    range: *mut Object,
) -> *mut Object {
    let Some(rust_range) = (unsafe { text_range_to_rust(range) }) else {
        return std::ptr::null_mut();
    };
    with_input_handler(this, |handler| {
        let mut adjusted = None;
        handler.text_for_range(rust_range, &mut adjusted)
    })
    .flatten()
    .map(|text| unsafe { ns_string(&text) })
    .unwrap_or(std::ptr::null_mut())
}

extern "C" fn uitextinput_replace_range(
    this: &Object,
    _sel: Sel,
    range: *mut Object,
    text: *mut Object,
) {
    let Some(rust_range) = (unsafe { text_range_to_rust(range) }) else {
        return;
    };
    let text_str = unsafe {
        if text.is_null() {
            return;
        }
        let utf8: *const u8 = msg_send![text, UTF8String];
        if utf8.is_null() {
            return;
        }
        std::ffi::CStr::from_ptr(utf8 as *const std::os::raw::c_char)
            .to_string_lossy()
            .into_owned()
    };
    with_input_handler(this, |handler| {
        handler.replace_text_in_range(Some(rust_range), &text_str);
    });
}

extern "C" fn uitextinput_selected_text_range(this: &Object, _sel: Sel) -> *mut Object {
    with_input_handler(this, |handler| {
        handler.selected_text_range(false).map(|sel| unsafe {
            make_text_range(sel.range.start, sel.range.end)
        })
    })
    .flatten()
    .unwrap_or(std::ptr::null_mut())
}

extern "C" fn uitextinput_set_selected_text_range(
    this: &Object,
    _sel: Sel,
    range: *mut Object,
) {
    let Some(rust_range) = (unsafe { text_range_to_rust(range) }) else {
        return;
    };
    // PlatformInputHandler doesn't expose a direct set_selected_range method.
    // Use replace_text_in_range with an empty string at a zero-width range
    // to position the cursor at the start of the desired selection.
    // Full selection ranges are not supported through this path — GPUI handles
    // selection via touch/mouse events instead.
    with_input_handler(this, |handler| {
        let cursor = rust_range.start;
        handler.replace_text_in_range(Some(cursor..cursor), "");
    });
}

extern "C" fn uitextinput_marked_text_range(this: &Object, _sel: Sel) -> *mut Object {
    with_input_handler(this, |handler| {
        handler.marked_text_range().map(|range| unsafe {
            make_text_range(range.start, range.end)
        })
    })
    .flatten()
    .unwrap_or(std::ptr::null_mut())
}

extern "C" fn uitextinput_marked_text_style(_this: &Object, _sel: Sel) -> *mut Object {
    // Return nil — let UIKit use default marked text styling (underline)
    std::ptr::null_mut()
}

extern "C" fn uitextinput_set_marked_text_style(_this: &Object, _sel: Sel, _style: *mut Object) {
    // No-op — we don't store custom marked text styles
}

extern "C" fn uitextinput_set_marked_text(
    this: &Object,
    _sel: Sel,
    marked_text: *mut Object,
    selected_range: NSRange,
) {
    let text = unsafe {
        if marked_text.is_null() {
            String::new()
        } else {
            // Check if it's an NSAttributedString
            let is_attributed: BOOL =
                msg_send![marked_text, isKindOfClass: class!(NSAttributedString)];
            let ns_str: *mut Object = if is_attributed == YES {
                msg_send![marked_text, string]
            } else {
                marked_text
            };
            let utf8: *const u8 = msg_send![ns_str, UTF8String];
            if utf8.is_null() {
                String::new()
            } else {
                std::ffi::CStr::from_ptr(utf8 as *const std::os::raw::c_char)
                    .to_string_lossy()
                    .into_owned()
            }
        }
    };

    let selected = selected_range.to_range();

    if text.is_empty() {
        // Empty marked text = clear composition
        with_input_handler(this, |handler| handler.unmark_text());
    } else {
        with_input_handler(this, |handler| {
            // replacement_range = None means replace current marked text or selection
            handler.replace_and_mark_text_in_range(None, &text, selected);
        });
    }
}

extern "C" fn uitextinput_unmark_text(this: &Object, _sel: Sel) {
    with_input_handler(this, |handler| handler.unmark_text());
}

extern "C" fn uitextinput_beginning_of_document(_this: &Object, _sel: Sel) -> *mut Object {
    unsafe { make_text_position(0) }
}

extern "C" fn uitextinput_end_of_document(this: &Object, _sel: Sel) -> *mut Object {
    let len = document_length_utf16(this);
    unsafe { make_text_position(len) }
}

extern "C" fn uitextinput_text_range_from_position(
    _this: &Object,
    _sel: Sel,
    from: *mut Object,
    to: *mut Object,
) -> *mut Object {
    if from.is_null() || to.is_null() {
        return std::ptr::null_mut();
    }
    unsafe {
        let start = text_position_index(from);
        let end = text_position_index(to);
        make_text_range(start, end)
    }
}

extern "C" fn uitextinput_position_from_position_offset(
    this: &Object,
    _sel: Sel,
    position: *mut Object,
    offset: isize,
) -> *mut Object {
    if position.is_null() {
        return std::ptr::null_mut();
    }
    let index = unsafe { text_position_index(position) };
    let new_index = index as isize + offset;
    if new_index < 0 {
        return std::ptr::null_mut();
    }
    let new_index = new_index as usize;
    let doc_len = document_length_utf16(this);
    if new_index > doc_len {
        return std::ptr::null_mut();
    }
    unsafe { make_text_position(new_index) }
}

extern "C" fn uitextinput_position_from_position_direction(
    this: &Object,
    _sel: Sel,
    position: *mut Object,
    direction: isize,
    offset: isize,
) -> *mut Object {
    // UITextLayoutDirection: 1=right, 2=left, 3=up, 4=down
    // For a simple text model, right/down = forward, left/up = backward
    let effective_offset = match direction {
        1 | 4 => offset,       // right/down = forward
        2 | 3 => -offset,      // left/up = backward
        _ => offset,
    };
    uitextinput_position_from_position_offset(this, _sel, position, effective_offset)
}

extern "C" fn uitextinput_compare_position(
    _this: &Object,
    _sel: Sel,
    a: *mut Object,
    b: *mut Object,
) -> isize {
    let idx_a = unsafe { text_position_index(a) };
    let idx_b = unsafe { text_position_index(b) };
    match idx_a.cmp(&idx_b) {
        std::cmp::Ordering::Less => -1,    // NSOrderedAscending
        std::cmp::Ordering::Equal => 0,     // NSOrderedSame
        std::cmp::Ordering::Greater => 1,   // NSOrderedDescending
    }
}

extern "C" fn uitextinput_offset_from_position(
    _this: &Object,
    _sel: Sel,
    from: *mut Object,
    to: *mut Object,
) -> isize {
    let idx_from = unsafe { text_position_index(from) };
    let idx_to = unsafe { text_position_index(to) };
    idx_to as isize - idx_from as isize
}

extern "C" fn uitextinput_input_delegate(this: &Object, _sel: Sel) -> *mut Object {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar("gpui_input_delegate");
        ptr as *mut Object
    }
}

extern "C" fn uitextinput_set_input_delegate(this: &Object, _sel: Sel, delegate: *mut Object) {
    unsafe {
        // The text input system sets this — we just store the reference.
        // We do NOT retain/release since the system owns the lifecycle.
        let this_mut = this as *const Object as *mut Object;
        (*this_mut).set_ivar::<*mut c_void>("gpui_input_delegate", delegate as *mut c_void);
    }
}

extern "C" fn uitextinput_tokenizer(this: &Object, _sel: Sel) -> *mut Object {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar("gpui_tokenizer");
        if !ptr.is_null() {
            return ptr as *mut Object;
        }
        // Lazily create a UITextInputStringTokenizer for this view
        let this_id = this as *const Object as *mut Object;
        let tokenizer: *mut Object = msg_send![class!(UITextInputStringTokenizer), alloc];
        let tokenizer: *mut Object = msg_send![tokenizer, initWithTextInput: this_id];
        let this_mut = this as *const Object as *mut Object;
        (*this_mut).set_ivar::<*mut c_void>("gpui_tokenizer", tokenizer as *mut c_void);
        tokenizer
    }
}

extern "C" fn uitextinput_position_within_range(
    _this: &Object,
    _sel: Sel,
    range: *mut Object,
    direction: isize,
) -> *mut Object {
    let Some(rust_range) = (unsafe { text_range_to_rust(range) }) else {
        return std::ptr::null_mut();
    };
    // UITextLayoutDirection: 1=right, 2=left, 3=up, 4=down
    let index = match direction {
        1 | 4 => rust_range.end,   // farthest right/down = end
        2 | 3 => rust_range.start, // farthest left/up = start
        _ => rust_range.end,
    };
    unsafe { make_text_position(index) }
}

extern "C" fn uitextinput_character_range_by_extending(
    this: &Object,
    _sel: Sel,
    position: *mut Object,
    direction: isize,
) -> *mut Object {
    if position.is_null() {
        return std::ptr::null_mut();
    }
    let index = unsafe { text_position_index(position) };
    let doc_len = document_length_utf16(this);
    // Extend one character in the given direction
    let (start, end) = match direction {
        1 | 4 => (index, (index + 1).min(doc_len)),  // right/down
        2 | 3 => (index.saturating_sub(1), index),     // left/up
        _ => (index, (index + 1).min(doc_len)),
    };
    unsafe { make_text_range(start, end) }
}

extern "C" fn uitextinput_base_writing_direction(
    _this: &Object,
    _sel: Sel,
    _position: *mut Object,
    _direction: isize,
) -> isize {
    0 // NSWritingDirectionNatural
}

extern "C" fn uitextinput_set_base_writing_direction(
    _this: &Object,
    _sel: Sel,
    _direction: isize,
    _range: *mut Object,
) {
    // No-op — GPUI doesn't support per-range writing direction changes
}

extern "C" fn uitextinput_first_rect_for_range(
    this: &Object,
    _sel: Sel,
    range: *mut Object,
) -> CGRect {
    let zero_rect = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize { width: 0.0, height: 0.0 },
    };

    let Some(rust_range) = (unsafe { text_range_to_rust(range) }) else {
        return zero_rect;
    };

    with_input_handler(this, |handler| {
        handler.bounds_for_range(rust_range).map(|bounds| {
            // Convert GPUI coordinates to UIKit screen coordinates.
            // On iOS, UIKit uses top-left origin — same as GPUI, no y-flip needed.
            CGRect {
                origin: CGPoint {
                    x: bounds.origin.x.to_f64(),
                    y: bounds.origin.y.to_f64(),
                },
                size: CGSize {
                    width: bounds.size.width.to_f64(),
                    height: bounds.size.height.to_f64(),
                },
            }
        })
    })
    .flatten()
    .unwrap_or(zero_rect)
}

extern "C" fn uitextinput_caret_rect_for_position(
    this: &Object,
    _sel: Sel,
    position: *mut Object,
) -> CGRect {
    if position.is_null() {
        return CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: CGSize { width: 0.0, height: 0.0 },
        };
    }
    let index = unsafe { text_position_index(position) };
    // Query bounds for a zero-width range at this position (caret)
    with_input_handler(this, |handler| {
        handler.bounds_for_range(index..index).map(|bounds| {
            CGRect {
                origin: CGPoint {
                    x: bounds.origin.x.to_f64(),
                    y: bounds.origin.y.to_f64(),
                },
                size: CGSize {
                    width: 2.0, // Standard caret width
                    height: bounds.size.height.to_f64(),
                },
            }
        })
    })
    .flatten()
    .unwrap_or(CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize { width: 2.0, height: 20.0 },
    })
}

extern "C" fn uitextinput_selection_rects_for_range(
    _this: &Object,
    _sel: Sel,
    _range: *mut Object,
) -> *mut Object {
    // Return empty array — UIKit will fall back to firstRectForRange
    unsafe { msg_send![class!(NSArray), array] }
}

extern "C" fn uitextinput_closest_position_to_point(
    this: &Object,
    _sel: Sel,
    point: CGPoint,
) -> *mut Object {
    let gpui_point = gpui::point(px(point.x as f32), px(point.y as f32));
    with_input_handler(this, |handler| {
        handler.character_index_for_point(gpui_point).map(|index| unsafe {
            make_text_position(index)
        })
    })
    .flatten()
    .unwrap_or_else(|| unsafe { make_text_position(0) })
}

extern "C" fn uitextinput_closest_position_to_point_within_range(
    this: &Object,
    _sel: Sel,
    point: CGPoint,
    range: *mut Object,
) -> *mut Object {
    let Some(rust_range) = (unsafe { text_range_to_rust(range) }) else {
        return uitextinput_closest_position_to_point(this, _sel, point);
    };
    let gpui_point = gpui::point(px(point.x as f32), px(point.y as f32));
    with_input_handler(this, |handler| {
        handler.character_index_for_point(gpui_point).map(|index| {
            let clamped = index.clamp(rust_range.start, rust_range.end);
            unsafe { make_text_position(clamped) }
        })
    })
    .flatten()
    .unwrap_or_else(|| unsafe { make_text_position(rust_range.start) })
}

extern "C" fn uitextinput_character_range_at_point(
    this: &Object,
    _sel: Sel,
    point: CGPoint,
) -> *mut Object {
    let gpui_point = gpui::point(px(point.x as f32), px(point.y as f32));
    with_input_handler(this, |handler| {
        handler.character_index_for_point(gpui_point).map(|index| unsafe {
            make_text_range(index, index + 1)
        })
    })
    .flatten()
    .unwrap_or(std::ptr::null_mut())
}

// ---------------------------------------------------------------------------
// Hardware keyboard via UIPresses (iOS 13.4+)
// ---------------------------------------------------------------------------

/// Map a UIKeyboardHIDUsage keyCode to a GPUI key name.
fn keycode_to_key_name(keycode: isize) -> Option<&'static str> {
    match keycode {
        0x28 => Some("enter"),
        0x29 => Some("escape"),
        0x2A => Some("backspace"),
        0x2B => Some("tab"),
        0x2C => Some("space"),
        0x39 => Some("capslock"),

        0x3A => Some("f1"),
        0x3B => Some("f2"),
        0x3C => Some("f3"),
        0x3D => Some("f4"),
        0x3E => Some("f5"),
        0x3F => Some("f6"),
        0x40 => Some("f7"),
        0x41 => Some("f8"),
        0x42 => Some("f9"),
        0x43 => Some("f10"),
        0x44 => Some("f11"),
        0x45 => Some("f12"),
        0x46 => Some("printscreen"),
        0x47 => Some("scrolllock"),
        0x48 => Some("pause"),
        0x49 => Some("insert"),
        0x4A => Some("home"),
        0x4B => Some("pageup"),
        0x4C => Some("delete"),
        0x4D => Some("end"),
        0x4E => Some("pagedown"),
        0x4F => Some("right"),
        0x50 => Some("left"),
        0x51 => Some("down"),
        0x52 => Some("up"),

        0x53 => Some("numlock"),
        0x54 => Some("numpad-divide"),
        0x55 => Some("numpad-multiply"),
        0x56 => Some("numpad-subtract"),
        0x57 => Some("numpad-add"),
        0x58 => Some("numpad-enter"),
        0x59 => Some("numpad-1"),
        0x5A => Some("numpad-2"),
        0x5B => Some("numpad-3"),
        0x5C => Some("numpad-4"),
        0x5D => Some("numpad-5"),
        0x5E => Some("numpad-6"),
        0x5F => Some("numpad-7"),
        0x60 => Some("numpad-8"),
        0x61 => Some("numpad-9"),
        0x62 => Some("numpad-0"),
        0x63 => Some("numpad-decimal"),
        0x67 => Some("numpad-equal"),
        0x68 => Some("f13"),
        0x69 => Some("f14"),
        0x6A => Some("f15"),
        0x6B => Some("f16"),
        0x6C => Some("f17"),
        0x6D => Some("f18"),
        0x6E => Some("f19"),
        0x6F => Some("f20"),
        0x70 => Some("f21"),
        0x71 => Some("f22"),
        0x72 => Some("f23"),
        0x73 => Some("f24"),

        0x76 => Some("menu"),
        0x7F => Some("mute"),
        0x80 => Some("volumeup"),
        0x81 => Some("volumedown"),

        0xE0 => Some("leftctrl"),
        0xE1 => Some("leftshift"),
        0xE2 => Some("leftalt"),
        0xE3 => Some("leftmeta"),
        0xE4 => Some("rightctrl"),
        0xE5 => Some("rightshift"),
        0xE6 => Some("rightalt"),
        0xE7 => Some("rightmeta"),
        _ => None,
    }
}

/// Extract Modifiers from UIKeyModifierFlags bitmask.
fn modifiers_from_flags(flags: isize) -> Modifiers {
    Modifiers {
        control: flags & 0x040000 != 0,  // UIKeyModifierControl
        alt: flags & 0x080000 != 0,      // UIKeyModifierAlternate
        shift: flags & 0x020000 != 0,    // UIKeyModifierShift
        platform: flags & 0x100000 != 0, // UIKeyModifierCommand
        function: flags & 0x800000 != 0, // UIKeyModifierNumericPad (fn key proxy)
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

            // Update the live modifier state
            state.lock().current_modifiers = modifiers;

            if is_modifier_key(keycode) {
                dispatch_input(
                    &state,
                    PlatformInput::ModifiersChanged(ModifiersChangedEvent {
                        modifiers,
                        capslock: Capslock::default(),
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
                        Some(
                            std::ffi::CStr::from_ptr(utf8)
                                .to_string_lossy()
                                .into_owned(),
                        )
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

            // Update the live modifier state
            state.lock().current_modifiers = modifiers;

            if is_modifier_key(keycode) {
                dispatch_input(
                    &state,
                    PlatformInput::ModifiersChanged(ModifiersChangedEvent {
                        modifiers,
                        capslock: Capslock::default(),
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

// ---------------------------------------------------------------------------
// iPadOS hover gesture — fires MouseMove without a pressed button when the
// user hovers a pointer (trackpad, mouse, Apple Pencil hover) over the view.
// ---------------------------------------------------------------------------

extern "C" fn handle_hover(this: &Object, _sel: Sel, gesture: *mut Object) {
    let Some(state) = (unsafe { get_window_state(this) }) else {
        return;
    };
    unsafe {
        let gesture_state: isize = msg_send![gesture, state];
        // UIGestureRecognizerState: 1=Began, 2=Changed, 3=Ended, 4=Cancelled
        match gesture_state {
            1 | 2 => {
                let location: CGPoint =
                    msg_send![gesture, locationInView: this as *const Object as *mut Object];
                let position = point(px(location.x as f32), px(location.y as f32));
                state.lock().last_touch_position = Some(position);

                let modifiers = state.lock().current_modifiers;
                dispatch_input(
                    &state,
                    PlatformInput::MouseMove(MouseMoveEvent {
                        position,
                        pressed_button: None,
                        modifiers,
                    }),
                );
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Long-press gesture — simulates right-click (context menu) on iOS.
// ---------------------------------------------------------------------------

extern "C" fn handle_long_press(this: &Object, _sel: Sel, gesture: *mut Object) {
    let Some(state) = (unsafe { get_window_state(this) }) else {
        return;
    };
    unsafe {
        let gesture_state: isize = msg_send![gesture, state];
        let location: CGPoint =
            msg_send![gesture, locationInView: this as *const Object as *mut Object];
        let position = point(px(location.x as f32), px(location.y as f32));

        let modifiers = state.lock().current_modifiers;
        // UIGestureRecognizerState: 1=Began, 3=Ended, 4=Cancelled
        match gesture_state {
            1 => {
                dispatch_input(
                    &state,
                    PlatformInput::MouseDown(MouseDownEvent {
                        button: MouseButton::Right,
                        position,
                        modifiers,
                        click_count: 1,
                        first_mouse: false,
                    }),
                );
            }
            3 | 4 => {
                dispatch_input(
                    &state,
                    PlatformInput::MouseUp(MouseUpEvent {
                        button: MouseButton::Right,
                        position,
                        modifiers,
                        click_count: 1,
                    }),
                );
            }
            _ => {}
        }
    }
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
        let translation: CGPoint =
            msg_send![gesture, translationInView: this as *const Object as *mut Object];
        let zero = CGPoint { x: 0.0, y: 0.0 };
        let _: () =
            msg_send![gesture, setTranslation: zero inView: this as *const Object as *mut Object];

        // Get position of the gesture centroid
        let location: CGPoint =
            msg_send![gesture, locationInView: this as *const Object as *mut Object];
        let position = point(px(location.x as f32), px(location.y as f32));

        let delta = ScrollDelta::Pixels(point(px(translation.x as f32), px(translation.y as f32)));

        let modifiers = state.lock().current_modifiers;
        dispatch_input(
            &state,
            PlatformInput::ScrollWheel(ScrollWheelEvent {
                position,
                delta,
                modifiers,
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

        let delta = ScrollDelta::Pixels(point(px(translation.x as f32), px(translation.y as f32)));

        let modifiers = state.lock().current_modifiers;
        dispatch_input(
            &state,
            PlatformInput::ScrollWheel(ScrollWheelEvent {
                position,
                delta,
                modifiers,
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

        let modifiers = state.lock().current_modifiers;
        dispatch_input(
            &state,
            PlatformInput::Pinch(PinchEvent {
                center,
                scale: scale as f32,
                modifiers,
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

        let modifiers = state.lock().current_modifiers;
        dispatch_input(
            &state,
            PlatformInput::Rotation(RotationEvent {
                center,
                rotation: rotation as f32,
                modifiers,
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

        let mut lock = state.lock();
        lock.safe_area_insets = insets;

        // Fire on_resize so the window re-evaluates safe_area_insets() and redraws.
        // We pass the same size — the viewport size hasn't changed, but the
        // inset-aware layout must be recomputed.
        let current_size = lock.bounds.size;
        let scale_factor = lock.scale_factor;
        if let Some(mut callback) = lock.on_resize.take() {
            drop(lock);
            callback(current_size, scale_factor);
            state.lock().on_resize = Some(callback);
        }
    }
}

// ---------------------------------------------------------------------------
// GPUIGestureDelegate — allows simultaneous gesture recognition
// ---------------------------------------------------------------------------

static mut GPUI_GESTURE_DELEGATE_CLASS: *const Class = std::ptr::null();

#[ctor]
fn register_gesture_delegate_class() {
    unsafe {
        let superclass = class!(NSObject);
        let mut decl = ClassDecl::new("GPUIGestureDelegate", superclass)
            .expect("failed to declare GPUIGestureDelegate class");

        decl.add_method(
            sel!(gestureRecognizer:shouldRecognizeSimultaneouslyWithGestureRecognizer:),
            gesture_should_recognize_simultaneously
                as extern "C" fn(&Object, Sel, *mut Object, *mut Object) -> BOOL,
        );

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
fn register_thermal_observer_class() {
    unsafe {
        let superclass = class!(NSObject);
        let mut decl = ClassDecl::new("GPUIThermalObserver", superclass)
            .expect("failed to declare GPUIThermalObserver class");

        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);

        decl.add_method(
            sel!(thermalStateChanged:),
            handle_thermal_state_changed as extern "C" fn(&Object, Sel, *mut Object),
        );

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
fn register_input_mode_observer_class() {
    unsafe {
        let superclass = class!(NSObject);
        let mut decl = ClassDecl::new("GPUIInputModeObserver", superclass)
            .expect("failed to declare GPUIInputModeObserver class");

        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);

        decl.add_method(
            sel!(inputModeChanged:),
            handle_input_mode_changed as extern "C" fn(&Object, Sel, *mut Object),
        );

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

unsafe extern "C" fn input_mode_changed_trampoline(context: *mut c_void) { unsafe {
    let callback = &mut *(context as *mut Box<dyn FnMut()>);
    callback();
}}

// ---------------------------------------------------------------------------
// GPUISceneObserver — receives UIScene lifecycle notifications and forwards
// them to the window state callbacks.
// ---------------------------------------------------------------------------

static mut GPUI_SCENE_OBSERVER_CLASS: *const Class = std::ptr::null();

#[ctor]
fn register_scene_observer_class() {
    unsafe {
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

unsafe fn get_scene_observer_state(observer: &Object) -> Option<Rc<Mutex<IosWindowState>>> { unsafe {
    let ptr: *mut c_void = *observer.get_ivar(WINDOW_STATE_IVAR);
    if ptr.is_null() {
        return None;
    }
    let rc = Rc::from_raw(ptr as *const Mutex<IosWindowState>);
    let clone = rc.clone();
    std::mem::forget(rc);
    Some(clone)
}}

// ---------------------------------------------------------------------------
// GPUIDropDelegate — handles external file drag/drop with UIDropInteraction.
// ---------------------------------------------------------------------------

static mut GPUI_DROP_DELEGATE_CLASS: *const Class = std::ptr::null();

#[ctor]
fn register_drop_delegate_class() {
    unsafe {
        let superclass = class!(NSObject);
        let mut decl = ClassDecl::new("GPUIDropDelegate", superclass)
            .expect("failed to declare GPUIDropDelegate");

        decl.add_ivar::<*mut c_void>(WINDOW_STATE_IVAR);

        decl.add_method(
            sel!(dropInteraction:canHandleSession:),
            drop_can_handle_session
                as extern "C" fn(&Object, Sel, *mut Object, *mut Object) -> BOOL,
        );
        decl.add_method(
            sel!(dropInteraction:sessionDidEnter:),
            drop_session_did_enter as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
        );
        decl.add_method(
            sel!(dropInteraction:sessionDidUpdate:),
            drop_session_did_update
                as extern "C" fn(&Object, Sel, *mut Object, *mut Object) -> *mut Object,
        );
        decl.add_method(
            sel!(dropInteraction:sessionDidExit:),
            drop_session_did_exit as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
        );
        decl.add_method(
            sel!(dropInteraction:performDrop:),
            drop_perform_drop as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
        );

        GPUI_DROP_DELEGATE_CLASS = decl.register();
    }
}

unsafe fn drop_delegate_state(delegate: &Object) -> Option<Rc<Mutex<IosWindowState>>> { unsafe {
    let ptr: *mut c_void = *delegate.get_ivar(WINDOW_STATE_IVAR);
    if ptr.is_null() {
        return None;
    }
    let rc = Rc::from_raw(ptr as *const Mutex<IosWindowState>);
    let clone = rc.clone();
    std::mem::forget(rc);
    Some(clone)
}}

fn drop_location(state: &Mutex<IosWindowState>, session: *mut Object) -> Point<Pixels> {
    unsafe {
        let ui_view = state.lock().ui_view;
        let location: CGPoint = msg_send![session, locationInView: ui_view];
        point(px(location.x as f32), px(location.y as f32))
    }
}

extern "C" fn drop_can_handle_session(
    _this: &Object,
    _sel: Sel,
    _interaction: *mut Object,
    session: *mut Object,
) -> BOOL {
    unsafe {
        let types: *mut Object = msg_send![class!(NSMutableArray), array];
        let _: () = msg_send![types, addObject: ns_string("public.file-url")];
        let _: () = msg_send![types, addObject: ns_string("public.url")];
        let has_types: BOOL = msg_send![session, hasItemsConformingToTypeIdentifiers: types];
        let can_load_urls: BOOL = msg_send![session, canLoadObjectsOfClass: class!(NSURL)];
        if has_types == YES || can_load_urls == YES {
            YES
        } else {
            NO
        }
    }
}

extern "C" fn drop_session_did_enter(
    this: &Object,
    _sel: Sel,
    _interaction: *mut Object,
    session: *mut Object,
) {
    let Some(state) = (unsafe { drop_delegate_state(this) }) else {
        return;
    };
    let position = drop_location(&state, session);
    dispatch_input(
        &state,
        PlatformInput::FileDrop(FileDropEvent::Pending { position }),
    );
}

extern "C" fn drop_session_did_update(
    this: &Object,
    _sel: Sel,
    _interaction: *mut Object,
    session: *mut Object,
) -> *mut Object {
    let Some(state) = (unsafe { drop_delegate_state(this) }) else {
        return std::ptr::null_mut();
    };
    let position = drop_location(&state, session);
    dispatch_input(
        &state,
        PlatformInput::FileDrop(FileDropEvent::Pending { position }),
    );

    unsafe {
        let proposal: *mut Object = msg_send![class!(UIDropProposal), alloc];
        msg_send![proposal, initWithDropOperation: 2usize] // UIDropOperationCopy
    }
}

extern "C" fn drop_session_did_exit(
    this: &Object,
    _sel: Sel,
    _interaction: *mut Object,
    _session: *mut Object,
) {
    let Some(state) = (unsafe { drop_delegate_state(this) }) else {
        return;
    };
    dispatch_input(&state, PlatformInput::FileDrop(FileDropEvent::Exited));
}

extern "C" fn drop_perform_drop(
    this: &Object,
    _sel: Sel,
    _interaction: *mut Object,
    session: *mut Object,
) {
    let Some(state) = (unsafe { drop_delegate_state(this) }) else {
        return;
    };
    let position = drop_location(&state, session);

    // Use a Mutex<Option<…>> wrapper so the Rc is always reclaimed when the
    // block is dropped, even if UIKit never invokes the completion handler.
    let state_holder = Arc::new(std::sync::Mutex::new(Some(state.clone())));

    let block = ConcreteBlock::new(move |objects: *mut Object| {
        let Some(state) = state_holder.lock().unwrap().take() else {
            return;
        };

        let mut paths = Vec::<PathBuf>::new();

        unsafe {
            if !objects.is_null() {
                let count: usize = msg_send![objects, count];
                for index in 0..count {
                    let url: *mut Object = msg_send![objects, objectAtIndex: index];
                    if url.is_null() {
                        continue;
                    }

                    let is_file: BOOL = msg_send![url, isFileURL];
                    if is_file != YES {
                        continue;
                    }

                    let started: BOOL = msg_send![url, startAccessingSecurityScopedResource];
                    let path_string: *mut Object = msg_send![url, path];
                    if !path_string.is_null() {
                        let utf8: *const std::os::raw::c_char = msg_send![path_string, UTF8String];
                        if !utf8.is_null() {
                            let path = std::ffi::CStr::from_ptr(utf8)
                                .to_string_lossy()
                                .into_owned();
                            if !path.is_empty() {
                                paths.push(PathBuf::from(path));
                            }
                        }
                    }
                    if started == YES {
                        let _: () = msg_send![url, stopAccessingSecurityScopedResource];
                    }
                }
            }

            if !paths.is_empty() {
                let external_paths = ExternalPaths(paths.into_iter().collect());
                dispatch_input(
                    &state,
                    PlatformInput::FileDrop(FileDropEvent::Entered {
                        position,
                        paths: external_paths,
                    }),
                );
                dispatch_input(
                    &state,
                    PlatformInput::FileDrop(FileDropEvent::Submit { position }),
                );
            }
            dispatch_input(&state, PlatformInput::FileDrop(FileDropEvent::Exited));
        }
    });
    let block = block.copy();

    unsafe {
        let _: *mut Object =
            msg_send![session, loadObjectsOfClass: class!(NSURL) completion: block];
    }
}

// ---------------------------------------------------------------------------
// GPUIDocumentPickerDelegate — bridges UIDocumentPicker callbacks to Rust
// oneshot channels used by prompt_for_paths/prompt_for_new_path.
// ---------------------------------------------------------------------------

enum PickerResultSender {
    Multiple(oneshot::Sender<Result<Option<Vec<PathBuf>>>>),
    Single(oneshot::Sender<Result<Option<PathBuf>>>),
}

struct DocumentPickerCallbackContext {
    sender: PickerResultSender,
    temp_paths: Vec<PathBuf>,
}

static mut GPUI_DOCUMENT_PICKER_DELEGATE_CLASS: *const Class = std::ptr::null();

#[ctor]
fn register_document_picker_delegate_class() {
    unsafe {
        let superclass = class!(NSObject);
        let mut decl = ClassDecl::new("GPUIDocumentPickerDelegate", superclass)
            .expect("failed to declare GPUIDocumentPickerDelegate");

        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);
        decl.add_method(
            sel!(documentPicker:didPickDocumentsAtURLs:),
            document_picker_did_pick as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
        );
        decl.add_method(
            sel!(documentPickerWasCancelled:),
            document_picker_was_cancelled as extern "C" fn(&Object, Sel, *mut Object),
        );

        GPUI_DOCUMENT_PICKER_DELEGATE_CLASS = decl.register();
    }
}

unsafe fn take_document_picker_context(
    delegate: *mut Object,
) -> Option<Box<DocumentPickerCallbackContext>> { unsafe {
    let ptr: *mut c_void = *(*delegate).get_ivar(CALLBACK_IVAR);
    if ptr.is_null() {
        return None;
    }
    (*delegate).set_ivar::<*mut c_void>(CALLBACK_IVAR, std::ptr::null_mut());
    Some(Box::from_raw(ptr as *mut DocumentPickerCallbackContext))
}}

unsafe fn release_document_picker_delegate(delegate: *mut Object) { unsafe {
    let platform_ptr = IOS_PLATFORM_STATE_PTR.load(Ordering::Acquire);
    if !platform_ptr.is_null() {
        let platform_state = &*(platform_ptr as *const Mutex<IosPlatformState>);
        let mut lock = platform_state.lock();
        if let Some(index) = lock
            .document_picker_delegates
            .iter()
            .position(|candidate| *candidate == delegate)
        {
            lock.document_picker_delegates.swap_remove(index);
        }
    }
    let _: () = msg_send![delegate, release];
}}

fn urls_to_paths(urls: *mut Object) -> Vec<PathBuf> {
    let mut result = Vec::new();
    unsafe {
        if urls.is_null() {
            return result;
        }
        let count: usize = msg_send![urls, count];
        for index in 0..count {
            let url: *mut Object = msg_send![urls, objectAtIndex: index];
            if url.is_null() {
                continue;
            }

            let is_file: BOOL = msg_send![url, isFileURL];
            if is_file != YES {
                continue;
            }

            let started: BOOL = msg_send![url, startAccessingSecurityScopedResource];
            let path_obj: *mut Object = msg_send![url, path];
            if !path_obj.is_null() {
                let utf8: *const std::os::raw::c_char = msg_send![path_obj, UTF8String];
                if !utf8.is_null() {
                    let path = std::ffi::CStr::from_ptr(utf8)
                        .to_string_lossy()
                        .into_owned();
                    if !path.is_empty() {
                        result.push(PathBuf::from(path));
                    }
                }
            }
            if started == YES {
                let _: () = msg_send![url, stopAccessingSecurityScopedResource];
            }
        }
    }
    result
}

fn finish_document_picker(delegate: &Object, result: Result<Option<Vec<PathBuf>>>) {
    unsafe {
        let delegate_ptr = delegate as *const Object as *mut Object;
        let Some(mut context) = take_document_picker_context(delegate_ptr) else {
            release_document_picker_delegate(delegate_ptr);
            return;
        };

        for temp_path in context.temp_paths.drain(..) {
            let _ = std::fs::remove_file(temp_path);
        }

        match context.sender {
            PickerResultSender::Multiple(sender) => {
                let _ = sender.send(result);
            }
            PickerResultSender::Single(sender) => {
                let mapped = result.map(|paths| paths.and_then(|p| p.into_iter().next()));
                let _ = sender.send(mapped);
            }
        }

        release_document_picker_delegate(delegate_ptr);
    }
}

extern "C" fn document_picker_did_pick(
    this: &Object,
    _sel: Sel,
    _controller: *mut Object,
    urls: *mut Object,
) {
    let paths = urls_to_paths(urls);
    finish_document_picker(this, Ok(Some(paths)));
}

extern "C" fn document_picker_was_cancelled(this: &Object, _sel: Sel, _controller: *mut Object) {
    finish_document_picker(this, Ok(None));
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
            let input_modes: *mut Object = msg_send![class!(UITextInputMode), activeInputModes];
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

pub(crate) struct IosKeyboardMapper {
    key_equivalents: Option<HashMap<char, char>>,
}

impl IosKeyboardMapper {
    fn new(layout_id: &str) -> Self {
        // Map non-QWERTY physical key positions to their QWERTY equivalents
        // so that keyboard shortcuts (Cmd+Z, Cmd+C, etc.) work on non-QWERTY
        // hardware keyboard layouts attached to iPad/iPhone.
        let mappings: Option<&[(char, char)]> = if layout_id.starts_with("fr") {
            // AZERTY (France)
            Some(&[
                ('a', 'q'), ('q', 'a'), ('z', 'w'), ('w', 'z'),
                ('m', ';'),
            ])
        } else if layout_id.starts_with("de") || layout_id.starts_with("at") {
            // QWERTZ (German/Austrian)
            Some(&[('y', 'z'), ('z', 'y')])
        } else if layout_id.starts_with("cs") || layout_id.starts_with("sk")
            || layout_id.starts_with("hu")
        {
            // QWERTZ (Czech/Slovak/Hungarian)
            Some(&[('y', 'z'), ('z', 'y')])
        } else if layout_id.starts_with("be") {
            // AZERTY (Belgium)
            Some(&[
                ('a', 'q'), ('q', 'a'), ('z', 'w'), ('w', 'z'),
                ('m', ';'),
            ])
        } else if layout_id.starts_with("tr") {
            // Turkish F-layout
            Some(&[
                ('f', 'a'), ('g', 's'), ('j', 'h'), ('k', 'j'),
            ])
        } else {
            // QWERTY (English, Spanish, Portuguese, Italian, etc.)
            None
        };

        let key_equivalents = mappings.map(|pairs| pairs.iter().copied().collect());
        Self { key_equivalents }
    }
}

impl PlatformKeyboardMapper for IosKeyboardMapper {
    fn map_key_equivalent(
        &self,
        mut keystroke: Keystroke,
        use_key_equivalents: bool,
    ) -> KeybindingKeystroke {
        if use_key_equivalents
            && let Some(map) = &self.key_equivalents
            && keystroke.key.chars().count() == 1
            && let Some(mapped) = map.get(&keystroke.key.chars().next().unwrap())
        {
            keystroke.key = mapped.to_string();
        }
        KeybindingKeystroke::from_keystroke(keystroke)
    }

    fn get_key_equivalents(&self) -> Option<&HashMap<char, char>> {
        self.key_equivalents.as_ref()
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

unsafe impl objc::Encode for CGPoint {
    fn encode() -> objc::Encoding {
        let encoding = format!(
            "{{CGPoint={}{}}}",
            f64::encode().as_str(),
            f64::encode().as_str()
        );
        unsafe { objc::Encoding::from_str(&encoding) }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct CGSize {
    width: f64,
    height: f64,
}

unsafe impl objc::Encode for CGSize {
    fn encode() -> objc::Encoding {
        let encoding = format!(
            "{{CGSize={}{}}}",
            f64::encode().as_str(),
            f64::encode().as_str()
        );
        unsafe { objc::Encoding::from_str(&encoding) }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

unsafe impl objc::Encode for CGRect {
    fn encode() -> objc::Encoding {
        let encoding = format!(
            "{{CGRect={}{}}}",
            CGPoint::encode().as_str(),
            CGSize::encode().as_str()
        );
        unsafe { objc::Encoding::from_str(&encoding) }
    }
}

#[derive(Debug)]
pub(crate) struct IosDisplay {
    id: DisplayId,
}

impl IosDisplay {
    fn primary() -> Self {
        Self {
            id: DisplayId::new(1),
        }
    }
}

impl PlatformDisplay for IosDisplay {
    fn id(&self) -> DisplayId {
        self.id
    }

    fn uuid(&self) -> Result<uuid::Uuid> {
        // Generate a stable UUID from the device's identifierForVendor
        let bytes = unsafe {
            let device: *mut Object = msg_send![class!(UIDevice), currentDevice];
            let vendor_id: *mut Object = msg_send![device, identifierForVendor];
            if !vendor_id.is_null() {
                let uuid_string: *mut Object = msg_send![vendor_id, UUIDString];
                if !uuid_string.is_null() {
                    let utf8: *const std::os::raw::c_char = msg_send![uuid_string, UTF8String];
                    if !utf8.is_null() {
                        let s = std::ffi::CStr::from_ptr(utf8).to_string_lossy();
                        if let Ok(parsed) = uuid::Uuid::parse_str(&s) {
                            return Ok(parsed);
                        }
                    }
                }
            }
            [0x01u8; 16]
        };
        Ok(uuid::Uuid::from_bytes(bytes))
    }

    fn bounds(&self) -> Bounds<Pixels> {
        // Query current screen bounds dynamically (handles rotation)
        unsafe {
            let screen: *mut Object = msg_send![class!(UIScreen), mainScreen];
            let bounds: CGRect = msg_send![screen, bounds];
            Bounds::new(
                point(px(0.0), px(0.0)),
                size(px(bounds.size.width as f32), px(bounds.size.height as f32)),
            )
        }
    }
}

pub(crate) struct IosDispatcher;

impl IosDispatcher {
    fn new() -> Self {
        Self
    }

    fn run_runnable(runnable: RunnableVariant) {
        let metadata = runnable.metadata();

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

unsafe fn ns_string(value: &str) -> *mut Object {
    let cstring = std::ffi::CString::new(value).unwrap_or_default();
    msg_send![class!(NSString), stringWithUTF8String: cstring.as_ptr()]
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
    let runnable = unsafe { RunnableVariant::from_raw(NonNull::new_unchecked(context.cast::<()>())) };
    IosDispatcher::run_runnable(runnable);
}

impl PlatformDispatcher for IosDispatcher {
    fn get_all_timings(&self) -> Vec<ThreadTaskTimings> {
        let global_timings = GLOBAL_THREAD_TIMINGS.lock();
        ThreadTaskTimings::convert(&global_timings)
    }

    fn get_current_thread_timings(&self) -> ThreadTaskTimings {
        THREAD_TIMINGS.with(|timings| {
            let timings = timings.lock();
            let raw_timings = &timings.timings;
            let mut vec = Vec::with_capacity(raw_timings.len());
            let (s1, s2) = raw_timings.as_slices();
            vec.extend_from_slice(s1);
            vec.extend_from_slice(s2);
            ThreadTaskTimings {
                thread_name: timings.thread_name.clone(),
                thread_id: timings.thread_id,
                timings: vec,
                total_pushed: timings.total_pushed,
            }
        })
    }

    fn is_main_thread(&self) -> bool {
        unsafe {
            let result: objc::runtime::BOOL = msg_send![class!(NSThread), isMainThread];
            result != objc::runtime::NO
        }
    }

    fn dispatch(&self, runnable: RunnableVariant, _priority: Priority) {
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

    fn dispatch_on_main_thread(&self, runnable: RunnableVariant, _priority: Priority) {
        let context = runnable.into_raw().as_ptr() as *mut c_void;
        unsafe {
            dispatch_async_f(
                Self::dispatch_get_main_queue(),
                context,
                Some(dispatch_trampoline),
            );
        }
    }

    fn dispatch_after(&self, duration: Duration, runnable: RunnableVariant) {
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

pub struct IosPlatform {
    state: Mutex<IosPlatformState>,
}

struct IosPlatformState {
    background_executor: BackgroundExecutor,
    foreground_executor: ForegroundExecutor,
    text_system: Arc<dyn PlatformTextSystem>,
    display: Rc<IosDisplay>,
    active_window: Option<AnyWindowHandle>,
    active_view_controller: *mut Object,
    open_urls: Option<Box<dyn FnMut(Vec<String>)>>,
    on_quit: Option<Box<dyn FnMut()>>,
    on_reopen: Option<Box<dyn FnMut()>>,
    on_thermal_state_change: Option<Box<dyn FnMut()>>,
    thermal_observer: *mut Object,
    input_mode_observer: *mut Object,
    app_menu_action: Option<Box<dyn FnMut(&dyn Action)>>,
    will_open_menu: Option<Box<dyn FnMut()>>,
    validate_app_menu: Option<Box<dyn FnMut(&dyn Action) -> bool>>,
    document_picker_delegates: Vec<*mut Object>,
}

impl IosPlatform {
    pub fn new(_headless: bool) -> Self {
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
                    {
                        Arc::new(IosTextSystem::new())
                    }
                    #[cfg(not(feature = "font-kit"))]
                    {
                        Arc::new(NoopTextSystem::new())
                    }
                },
                display: Rc::new(IosDisplay::primary()),
                active_window: None,
                active_view_controller: std::ptr::null_mut(),
                open_urls: None,
                on_quit: None,
                on_reopen: None,
                on_thermal_state_change: None,
                thermal_observer: std::ptr::null_mut(),
                input_mode_observer: std::ptr::null_mut(),
                app_menu_action: None,
                will_open_menu: None,
                validate_app_menu: None,
                document_picker_delegates: Vec::new(),
            }),
        };

        // Store a global pointer to the platform state for gpui_ios_handle_open_url
        let state_ptr = &platform.state as *const Mutex<IosPlatformState> as *mut c_void;
        IOS_PLATFORM_STATE_PTR.store(state_ptr, std::sync::atomic::Ordering::Release);

        platform
    }

    fn active_presenting_controller(&self) -> Option<*mut Object> {
        let controller = self.state.lock().active_view_controller;
        if !controller.is_null() {
            return Some(controller);
        }

        // Use the modern UIWindowScene API (iOS 13+) instead of the
        // deprecated UIApplication.keyWindow property.
        unsafe {
            let app: *mut Object = msg_send![class!(UIApplication), sharedApplication];
            if app.is_null() {
                return None;
            }
            let scenes: *mut Object = msg_send![app, connectedScenes];
            if scenes.is_null() {
                return None;
            }
            let count: usize = msg_send![scenes, count];
            for i in 0..count {
                let scene: *mut Object = msg_send![scenes, objectAtIndex: i];
                if scene.is_null() {
                    continue;
                }
                // Check if this is a UIWindowScene (responds to `windows`)
                let has_windows: BOOL = msg_send![scene, respondsToSelector: sel!(windows)];
                if has_windows == NO {
                    continue;
                }
                let windows: *mut Object = msg_send![scene, windows];
                if windows.is_null() {
                    continue;
                }
                let win_count: usize = msg_send![windows, count];
                for j in 0..win_count {
                    let window: *mut Object = msg_send![windows, objectAtIndex: j];
                    if window.is_null() {
                        continue;
                    }
                    let is_key: BOOL = msg_send![window, isKeyWindow];
                    if is_key == YES {
                        let root: *mut Object = msg_send![window, rootViewController];
                        if !root.is_null() {
                            return Some(root);
                        }
                    }
                }
            }
            None
        }
    }

    fn create_document_picker_delegate(
        &self,
        sender: PickerResultSender,
        temp_paths: Vec<PathBuf>,
    ) -> *mut Object {
        unsafe {
            let delegate: *mut Object = msg_send![GPUI_DOCUMENT_PICKER_DELEGATE_CLASS, new];
            let context = Box::new(DocumentPickerCallbackContext { sender, temp_paths });
            (*delegate)
                .set_ivar::<*mut c_void>(CALLBACK_IVAR, Box::into_raw(context) as *mut c_void);
            self.state.lock().document_picker_delegates.push(delegate);
            delegate
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
        let mut platform_state = self.state.lock();
        platform_state.active_window = Some(handle);
        platform_state.active_view_controller = window.root_view_controller();
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

    fn register_url_scheme(&self, url: &str) -> Task<Result<()>> {
        let scheme = url
            .trim()
            .trim_end_matches("://")
            .split(':')
            .next()
            .unwrap_or(url)
            .to_string();
        Task::ready({
            unsafe {
                let bundle: *mut Object = msg_send![class!(NSBundle), mainBundle];
                if bundle.is_null() {
                    Err(anyhow!(
                        "main bundle unavailable; cannot validate URL scheme"
                    ))
                } else {
                    let info: *mut Object = msg_send![bundle, infoDictionary];
                    if info.is_null() {
                        Err(anyhow!("Info.plist missing; cannot validate URL scheme"))
                    } else {
                        let key = ns_string("CFBundleURLTypes");
                        let url_types: *mut Object = msg_send![info, objectForKey: key];
                        if url_types.is_null() {
                            Err(anyhow!(
                                "URL scheme '{}' is not declared in CFBundleURLTypes",
                                scheme
                            ))
                        } else {
                            let mut found = false;
                            let type_count: usize = msg_send![url_types, count];
                            for i in 0..type_count {
                                let url_type: *mut Object = msg_send![url_types, objectAtIndex: i];
                                if url_type.is_null() {
                                    continue;
                                }
                                let schemes_key = ns_string("CFBundleURLSchemes");
                                let schemes: *mut Object =
                                    msg_send![url_type, objectForKey: schemes_key];
                                if schemes.is_null() {
                                    continue;
                                }
                                let scheme_count: usize = msg_send![schemes, count];
                                for j in 0..scheme_count {
                                    let item: *mut Object = msg_send![schemes, objectAtIndex: j];
                                    if item.is_null() {
                                        continue;
                                    }
                                    let utf8: *const std::os::raw::c_char =
                                        msg_send![item, UTF8String];
                                    if utf8.is_null() {
                                        continue;
                                    }
                                    let declared = std::ffi::CStr::from_ptr(utf8)
                                        .to_string_lossy()
                                        .into_owned();
                                    if declared.eq_ignore_ascii_case(&scheme) {
                                        found = true;
                                        break;
                                    }
                                }
                                if found {
                                    break;
                                }
                            }

                            if found {
                                Ok(())
                            } else {
                                Err(anyhow!(
                                    "URL scheme '{}' is not declared in CFBundleURLTypes",
                                    scheme
                                ))
                            }
                        }
                    }
                }
            }
        })
    }

    fn prompt_for_paths(
        &self,
        options: PathPromptOptions,
    ) -> oneshot::Receiver<Result<Option<Vec<PathBuf>>>> {
        let (tx, rx) = oneshot::channel();

        let Some(presenter) = self.active_presenting_controller() else {
            let _ = tx.send(Err(anyhow!(
                "no active view controller to present document picker"
            )));
            return rx;
        };

        if !options.files && !options.directories {
            let _ = tx.send(Err(anyhow!(
                "invalid path prompt options: at least one of files/directories must be true"
            )));
            return rx;
        }

        // Use the modern UTType-based API (iOS 14+) instead of the deprecated
        // initWithDocumentTypes:inMode: initializer.
        unsafe {
            let content_types: *mut Object = msg_send![class!(NSMutableArray), array];
            if options.files {
                let data_type: *mut Object = msg_send![class!(UTType), typeWithIdentifier: ns_string("public.data")];
                if !data_type.is_null() {
                    let _: () = msg_send![content_types, addObject: data_type];
                }
            }
            if options.directories {
                let folder_type: *mut Object = msg_send![class!(UTType), typeWithIdentifier: ns_string("public.folder")];
                if !folder_type.is_null() {
                    let _: () = msg_send![content_types, addObject: folder_type];
                }
            }

            let picker: *mut Object = msg_send![class!(UIDocumentPickerViewController), alloc];
            let picker: *mut Object =
                msg_send![picker, initForOpeningContentTypes: content_types];
            if picker.is_null() {
                let _ = tx.send(Err(anyhow!(
                    "failed to create UIDocumentPickerViewController"
                )));
                return rx;
            }

            let _: () = msg_send![picker, setAllowsMultipleSelection: if options.multiple { YES } else { NO }];
            let delegate =
                self.create_document_picker_delegate(PickerResultSender::Multiple(tx), Vec::new());
            let _: () = msg_send![picker, setDelegate: delegate];
            let _: () = msg_send![presenter,
                presentViewController: picker
                animated: YES
                completion: std::ptr::null::<c_void>()
            ];
        }
        rx
    }

    fn prompt_for_new_path(
        &self,
        _directory: &Path,
        suggested_name: Option<&str>,
    ) -> oneshot::Receiver<Result<Option<PathBuf>>> {
        let (tx, rx) = oneshot::channel();

        let Some(presenter) = self.active_presenting_controller() else {
            let _ = tx.send(Err(anyhow!(
                "no active view controller to present document picker"
            )));
            return rx;
        };

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let suggested = suggested_name.unwrap_or("untitled.txt");
        let source_path = std::env::temp_dir().join(format!("gpui-export-{now_ms}-{suggested}"));

        if let Err(error) = std::fs::write(&source_path, []) {
            let _ = tx.send(Err(anyhow!(
                "failed to create temporary export file '{}': {error}",
                source_path.display()
            )));
            return rx;
        }

        unsafe {
            let path_string = ns_string(source_path.to_string_lossy().as_ref());
            let source_url: *mut Object =
                msg_send![class!(NSURL), fileURLWithPath: path_string isDirectory: NO];
            if source_url.is_null() {
                let _ = std::fs::remove_file(&source_path);
                let _ = tx.send(Err(anyhow!(
                    "failed to create file URL for '{}'",
                    source_path.display()
                )));
                return rx;
            }

            // Use the modern initForExportingURLs: API (iOS 14+) instead of
            // the deprecated initWithURL:inMode: initializer.
            let urls: *mut Object = msg_send![class!(NSArray), arrayWithObject: source_url];
            let picker: *mut Object = msg_send![class!(UIDocumentPickerViewController), alloc];
            let picker: *mut Object = msg_send![picker, initForExportingURLs: urls];
            if picker.is_null() {
                let _ = std::fs::remove_file(&source_path);
                let _ = tx.send(Err(anyhow!(
                    "failed to create UIDocumentPickerViewController for export"
                )));
                return rx;
            }

            let delegate = self
                .create_document_picker_delegate(PickerResultSender::Single(tx), vec![source_path]);
            let _: () = msg_send![picker, setDelegate: delegate];
            let _: () = msg_send![presenter,
                presentViewController: picker
                animated: YES
                completion: std::ptr::null::<c_void>()
            ];
        }
        rx
    }

    fn can_select_mixed_files_and_dirs(&self) -> bool {
        true
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

        // Remove previous observer if any
        unsafe {
            if !platform_state.thermal_observer.is_null() {
                let center: *mut Object = msg_send![class!(NSNotificationCenter), defaultCenter];
                let _: () = msg_send![center, removeObserver: platform_state.thermal_observer];
                // Free the old callback stored in the ivar
                let old_ptr: *mut c_void =
                    *(*platform_state.thermal_observer).get_ivar(CALLBACK_IVAR);
                if !old_ptr.is_null() {
                    let _ = Box::from_raw(old_ptr as *mut Box<dyn FnMut()>);
                }
                let _: () = msg_send![platform_state.thermal_observer, release];
                platform_state.thermal_observer = std::ptr::null_mut();
            }
        }

        platform_state.on_thermal_state_change = Some(callback);

        // Heap-allocate the callback pointer so it has a stable address
        // independent of the mutex lock. This avoids a dangling pointer
        // when the lock is dropped.
        let callback_box: Box<Box<dyn FnMut()>> =
            Box::new(platform_state.on_thermal_state_change.take().unwrap());
        let callback_ptr = Box::into_raw(callback_box) as *mut c_void;

        // Store a reference back so we can call it from GPUI too
        // (the Box is now owned by the ivar, we reconstruct a reference)
        unsafe {
            let observer: *mut Object = msg_send![GPUI_THERMAL_OBSERVER_CLASS, new];
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
        Err(anyhow!(
            "auxiliary executable lookup is not implemented on iOS"
        ))
    }

    fn set_cursor_style(&self, _style: CursorStyle) {}

    fn should_auto_hide_scrollbars(&self) -> bool {
        true
    }

    fn read_from_clipboard(&self) -> Option<ClipboardItem> {
        unsafe {
            let pasteboard: *mut Object = msg_send![class!(UIPasteboard), generalPasteboard];
            let metadata_type = ns_string("dev.gpui.clipboard-metadata");

            let has_strings: BOOL = msg_send![pasteboard, hasStrings];
            if has_strings == YES {
                let ns_string: *mut Object = msg_send![pasteboard, string];
                if !ns_string.is_null() {
                    let utf8: *const std::os::raw::c_char = msg_send![ns_string, UTF8String];
                    if !utf8.is_null() {
                        let text = std::ffi::CStr::from_ptr(utf8)
                            .to_string_lossy()
                            .into_owned();

                        let meta_data: *mut Object =
                            msg_send![pasteboard, dataForPasteboardType: metadata_type];
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

            let has_images: BOOL = msg_send![pasteboard, hasImages];
            if has_images == YES {
                let image_obj: *mut Object = msg_send![pasteboard, image];
                if !image_obj.is_null() {
                    let image_data: *mut Object = msg_send![image_obj, pngData];
                    if !image_data.is_null() {
                        let length: usize = msg_send![image_data, length];
                        let bytes: *const u8 = msg_send![image_data, bytes];
                        if !bytes.is_null() && length > 0 {
                            let bytes = std::slice::from_raw_parts(bytes, length).to_vec();
                            let image = Image {
                                format: ImageFormat::Png,
                                id: hash(&bytes),
                                bytes,
                            };
                            return Some(ClipboardItem::new_image(&image));
                        }
                    }
                }
            }

            None
        }
    }

    fn write_to_clipboard(&self, item: ClipboardItem) {
        unsafe {
            let pasteboard: *mut Object = msg_send![class!(UIPasteboard), generalPasteboard];
            if let [ClipboardEntry::Image(image)] = item.entries() {
                let ns_data: *mut Object = msg_send![class!(NSData),
                    dataWithBytes: image.bytes().as_ptr()
                    length: image.bytes().len() as u64
                ];
                if !ns_data.is_null() {
                    let ui_image: *mut Object = msg_send![class!(UIImage), imageWithData: ns_data];
                    if !ui_image.is_null() {
                        let _: () = msg_send![pasteboard, setImage: ui_image];
                        return;
                    }
                }
            }

            if let Some(text) = item.text() {
                let ns_text = ns_string(&text);
                let _: () = msg_send![pasteboard, setString: ns_text];

                if let Some(metadata) = item.metadata() {
                    let metadata_type = ns_string("dev.gpui.clipboard-metadata");
                    let metadata_ns = ns_string(metadata.as_str());
                    let encoding: usize = 4; // NSUTF8StringEncoding
                    let metadata_data: *mut Object =
                        msg_send![metadata_ns, dataUsingEncoding: encoding];
                    if !metadata_data.is_null() {
                        let _: () = msg_send![pasteboard,
                            setData: metadata_data
                            forPasteboardType: metadata_type
                        ];
                    }
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
                        return Ok(None);
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
        let layout = IosKeyboardLayout::current();
        Rc::new(IosKeyboardMapper::new(layout.id()))
    }

    fn on_keyboard_layout_change(&self, callback: Box<dyn FnMut()>) {
        let mut platform_state = self.state.lock();

        // Remove previous observer if any
        unsafe {
            if !platform_state.input_mode_observer.is_null() {
                let center: *mut Object = msg_send![class!(NSNotificationCenter), defaultCenter];
                let _: () =
                    msg_send![center, removeObserver: platform_state.input_mode_observer];
                let old_ptr: *mut c_void =
                    *(*platform_state.input_mode_observer).get_ivar(CALLBACK_IVAR);
                if !old_ptr.is_null() {
                    let _ = Box::from_raw(old_ptr as *mut Box<dyn FnMut()>);
                }
                let _: () = msg_send![platform_state.input_mode_observer, release];
                platform_state.input_mode_observer = std::ptr::null_mut();
            }
        }

        // Heap-allocate callback so the pointer is stable
        let callback_box: Box<Box<dyn FnMut()>> = Box::new(callback);
        let callback_ptr = Box::into_raw(callback_box) as *mut c_void;

        // Register for UITextInputCurrentInputModeDidChangeNotification
        unsafe {
            let observer: *mut Object = msg_send![GPUI_INPUT_MODE_OBSERVER_CLASS, new];
            (*observer).set_ivar::<*mut c_void>(CALLBACK_IVAR, callback_ptr);

            let center: *mut Object = msg_send![class!(NSNotificationCenter), defaultCenter];
            let name_bytes = b"UITextInputCurrentInputModeDidChangeNotification\0";
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
    bounds: Bounds<Pixels>,
    display: Rc<dyn PlatformDisplay>,
    scale_factor: f32,
    ui_window: *mut Object,
    ui_view_controller: *mut Object,
    ui_view: *mut Object,
    // Drag-and-drop integration for external file drops.
    drop_interaction: *mut Object,
    drop_delegate: *mut Object,
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
    // Live modifier state from hardware keyboard presses
    current_modifiers: Modifiers,
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
    on_resize: Option<Box<dyn FnMut(Size<Pixels>, f32)>>,
    on_moved: Option<Box<dyn FnMut()>>,
    on_close: Option<Box<dyn FnOnce()>>,
    on_hit_test_window_control: Option<Box<dyn FnMut() -> Option<WindowControlArea>>>,
    on_appearance_change: Option<Box<dyn FnMut()>>,
    input_handler: Option<PlatformInputHandler>,
    title: String,
}

pub(crate) struct IosWindow(Rc<Mutex<IosWindowState>>);

impl IosWindow {
    fn root_view_controller(&self) -> *mut Object {
        self.0.lock().ui_view_controller
    }

    fn new(
        _handle: AnyWindowHandle,
        options: WindowParams,
        display: Rc<dyn PlatformDisplay>,
    ) -> Self {
        log::debug!("creating iOS window");
        let (
            ui_window,
            ui_view_controller,
            ui_view,
            drop_interaction,
            drop_delegate,
            gesture_delegate,
            bounds,
            scale_factor,
        ) = unsafe {
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

            // Create gesture delegate for simultaneous recognition
            let gesture_delegate: *mut Object = msg_send![GPUI_GESTURE_DELEGATE_CLASS, new];

            // Two-finger pan gesture for scroll
            let pan: *mut Object = msg_send![class!(UIPanGestureRecognizer), alloc];
            let pan: *mut Object =
                msg_send![pan, initWithTarget: ui_view action: sel!(handleScrollPan:)];
            let _: () = msg_send![pan, setMinimumNumberOfTouches: 2usize];
            let _: () = msg_send![pan, setMaximumNumberOfTouches: 2usize];
            let _: () = msg_send![pan, setDelegate: gesture_delegate];
            let _: () = msg_send![ui_view, addGestureRecognizer: pan];

            // Single-finger pan gesture for scroll
            let single_pan: *mut Object = msg_send![class!(UIPanGestureRecognizer), alloc];
            let single_pan: *mut Object = msg_send![single_pan,
                    initWithTarget: ui_view action: sel!(handleSingleFingerPan:)];
            let _: () = msg_send![single_pan, setMinimumNumberOfTouches: 1usize];
            let _: () = msg_send![single_pan, setMaximumNumberOfTouches: 1usize];
            let _: () = msg_send![ui_view, addGestureRecognizer: single_pan];

            // Pinch gesture
            let pinch: *mut Object = msg_send![class!(UIPinchGestureRecognizer), alloc];
            let pinch: *mut Object =
                msg_send![pinch, initWithTarget: ui_view action: sel!(handlePinch:)];
            let _: () = msg_send![pinch, setDelegate: gesture_delegate];
            let _: () = msg_send![ui_view, addGestureRecognizer: pinch];

            // Rotation gesture
            let rotation: *mut Object = msg_send![class!(UIRotationGestureRecognizer), alloc];
            let rotation: *mut Object =
                msg_send![rotation, initWithTarget: ui_view action: sel!(handleRotation:)];
            let _: () = msg_send![rotation, setDelegate: gesture_delegate];
            let _: () = msg_send![ui_view, addGestureRecognizer: rotation];

            // iPadOS hover gesture (pointer/trackpad/Apple Pencil hover)
            let hover: *mut Object = msg_send![class!(UIHoverGestureRecognizer), alloc];
            let hover: *mut Object =
                msg_send![hover, initWithTarget: ui_view action: sel!(handleHover:)];
            let _: () = msg_send![ui_view, addGestureRecognizer: hover];

            // Long-press gesture for simulated right-click (context menu)
            let long_press: *mut Object = msg_send![class!(UILongPressGestureRecognizer), alloc];
            let long_press: *mut Object =
                msg_send![long_press, initWithTarget: ui_view action: sel!(handleLongPress:)];
            let _: () = msg_send![long_press, setDelegate: gesture_delegate];
            let _: () = msg_send![ui_view, addGestureRecognizer: long_press];

            // Native file drag/drop for external files (UIDropInteraction).
            let drop_delegate: *mut Object = msg_send![GPUI_DROP_DELEGATE_CLASS, new];
            let drop_interaction: *mut Object = msg_send![class!(UIDropInteraction), alloc];
            let drop_interaction: *mut Object =
                msg_send![drop_interaction, initWithDelegate: drop_delegate];
            let _: () = msg_send![ui_view, addInteraction: drop_interaction];

            let _: () = msg_send![ui_view_controller, setView: ui_view];
            let _: () = msg_send![ui_window, setRootViewController: ui_view_controller];
            let _: () = msg_send![ui_window, makeKeyAndVisible];

            let bounds = Bounds::new(
                point(px(0.0), px(0.0)),
                size(
                    px(screen_bounds.size.width as f32),
                    px(screen_bounds.size.height as f32),
                ),
            );
            (
                ui_window,
                ui_view_controller,
                ui_view,
                drop_interaction,
                drop_delegate,
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
            let view_metal_layer = MetalLayer::from_ptr(view_layer as *mut CAMetalLayer);
            // from_ptr creates an owning wrapper; retain so the view keeps its layer alive
            let _: () = msg_send![view_layer, retain];
            renderer.replace_layer(view_metal_layer);
            let _: () = msg_send![view_layer, setContentsScale: scale_factor as f64];
        }

        let drawable_size = bounds.size.to_device_pixels(scale_factor);
        renderer.update_drawable_size(drawable_size);

        log::info!(
            "iOS window created ({}x{} @{}x)",
            bounds.size.width.to_f64(),
            bounds.size.height.to_f64(),
            scale_factor,
        );

        let window = Self(Rc::new(Mutex::new(IosWindowState {
            bounds: if options.bounds.size.width > Pixels::ZERO
                && options.bounds.size.height > Pixels::ZERO
            {
                options.bounds
            } else {
                bounds
            },
            display,
            scale_factor,
            ui_window,
            ui_view_controller,
            ui_view,
            drop_interaction,
            drop_delegate,
            renderer,
            display_link: std::ptr::null_mut(),
            display_link_target: std::ptr::null_mut(),
            display_link_callback_ptr: std::ptr::null_mut(),
            tracked_touch: None,
            last_touch_position: None,
            last_press_had_key: false,
            current_modifiers: Modifiers::default(),
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
            let view_state_ptr = Rc::into_raw(window.0.clone()) as *mut c_void;
            (*ui_view).set_ivar::<*mut c_void>(WINDOW_STATE_IVAR, view_state_ptr);
            let drop_state_ptr = Rc::into_raw(window.0.clone()) as *mut c_void;
            (*drop_delegate).set_ivar::<*mut c_void>(WINDOW_STATE_IVAR, drop_state_ptr);
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

            let did_activate: *mut Object =
                msg_send![class!(NSString), stringWithUTF8String: UISCENE_DID_ACTIVATE.as_ptr()];
            let _: () = msg_send![center, addObserver: observer
                selector: sel!(sceneDidActivate:)
                name: did_activate
                object: std::ptr::null::<Object>()];

            let will_deactivate: *mut Object =
                msg_send![class!(NSString), stringWithUTF8String: UISCENE_WILL_DEACTIVATE.as_ptr()];
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
                let center: *mut Object = msg_send![class!(NSNotificationCenter), defaultCenter];
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
            if !state.drop_delegate.is_null() {
                let ptr: *mut c_void = *(*state.drop_delegate).get_ivar(WINDOW_STATE_IVAR);
                if !ptr.is_null() {
                    let _ = Rc::from_raw(ptr as *const Mutex<IosWindowState>);
                    (*state.drop_delegate)
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

            if !state.drop_interaction.is_null() {
                let _: () = msg_send![state.drop_interaction, release];
                state.drop_interaction = std::ptr::null_mut();
            }
            if !state.drop_delegate.is_null() {
                let _: () = msg_send![state.drop_delegate, release];
                state.drop_delegate = std::ptr::null_mut();
            }

            // Release gesture delegate
            if !state.gesture_delegate.is_null() {
                let _: () = msg_send![state.gesture_delegate, release];
                state.gesture_delegate = std::ptr::null_mut();
            }

            // Release the lazily-created UITextInputStringTokenizer
            if !state.ui_view.is_null() {
                let tokenizer_ptr: *mut c_void =
                    *(*state.ui_view).get_ivar("gpui_tokenizer");
                if !tokenizer_ptr.is_null() {
                    let _: () = msg_send![tokenizer_ptr as *mut Object, release];
                    (*state.ui_view)
                        .set_ivar::<*mut c_void>("gpui_tokenizer", std::ptr::null_mut());
                }
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
        let ui_view =
            NonNull::new(state.ui_view.cast::<c_void>()).ok_or(HandleError::Unavailable)?;
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

    fn content_size(&self) -> Size<Pixels> {
        self.bounds().size
    }

    fn safe_area_insets(&self) -> Edges<Pixels> {
        let insets = self.0.lock().safe_area_insets;
        Edges {
            top: px(insets.top as f32),
            right: px(insets.right as f32),
            bottom: px(insets.bottom as f32),
            left: px(insets.left as f32),
        }
    }

    fn resize(&mut self, size: Size<Pixels>) {
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
        self.0.lock().current_modifiers
    }

    fn capslock(&self) -> Capslock {
        Capslock::default()
    }

    fn set_input_handler(&mut self, input_handler: PlatformInputHandler) {
        log::debug!(
            "[keyboard] set_input_handler called — registering handler (keyboard NOT shown)"
        );
        let mut lock = self.0.lock();
        lock.input_handler = Some(input_handler);
        // Don't call becomeFirstResponder here — it would show the keyboard
        // on every app launch. The keyboard is shown via the touch handler
        // when the user actually taps a text input.
    }

    fn take_input_handler(&mut self) -> Option<PlatformInputHandler> {
        let mut lock = self.0.lock();
        let handler = lock.input_handler.take();
        let ui_view = lock.ui_view;
        drop(lock);
        if handler.is_some() && !ui_view.is_null() {
            log::debug!(
                "[keyboard] take_input_handler — resigning first responder (hiding keyboard)"
            );
            unsafe {
                let _: () = msg_send![ui_view, resignFirstResponder];
            }
        }
        handler
    }

    fn prompt(
        &self,
        _level: PromptLevel,
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
                    let blur_view: *mut Object = msg_send![class!(UIVisualEffectView), alloc];
                    let blur_view: *mut Object = msg_send![blur_view, initWithEffect: effect];

                    let bounds: CGRect = msg_send![lock.ui_view, bounds];
                    let _: () = msg_send![blur_view, setFrame: bounds];
                    // Auto-resize with parent
                    let autoresizing: usize = 0x3F; // FlexibleWidth | FlexibleHeight | all margins
                    let _: () = msg_send![blur_view, setAutoresizingMask: autoresizing];

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

                    let modifiers = lock.current_modifiers;
                    if vx.abs() < 0.5 && vy.abs() < 0.5 {
                        // Momentum exhausted — send final event
                        lock.scroll_momentum = None;
                        if let Some(mut input_cb) = lock.on_input.take() {
                            drop(lock);
                            input_cb(PlatformInput::ScrollWheel(ScrollWheelEvent {
                                position,
                                delta: ScrollDelta::Pixels(point(px(0.0), px(0.0))),
                                modifiers,
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
                                modifiers,
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
            let _: () =
                msg_send![display_link, addToRunLoop: run_loop forMode: NSRunLoopCommonModes];

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

    fn on_resize(&self, callback: Box<dyn FnMut(Size<Pixels>, f32)>) {
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

    fn draw(&self, scene: &Scene) {
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

    fn update_ime_position(&self, _bounds: Bounds<Pixels>) {}

    fn raw_native_view_ptr(&self) -> *mut c_void {
        self.0.lock().ui_view.cast::<c_void>()
    }

    fn native_controls(&self) -> Option<&dyn gpui::native_controls::PlatformNativeControls> {
        Some(&IOS_NATIVE_CONTROLS)
    }

    fn configure_hosted_content(
        &self,
        host_view: *mut c_void,
        parent_view: *mut c_void,
        _config: HostedContentConfig,
    ) {
        unsafe {
            crate::native_controls::configure_native_sidebar_window(
                host_view as crate::native_controls::id,
                parent_view,
            );
        }
    }

    fn attach_hosted_surface(
        &self,
        host_view: *mut c_void,
        surface_view: *mut c_void,
    ) {
        unsafe {
            crate::native_controls::embed_surface_view_in_sidebar(
                host_view as crate::native_controls::id,
                surface_view as crate::native_controls::id,
            );
        }
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
