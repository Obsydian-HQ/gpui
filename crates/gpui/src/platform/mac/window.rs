use super::{BoolExt, MacDisplay, NSRange, NSStringExt, ns_string, renderer};
use crate::{
    AnyWindowHandle, BackgroundExecutor, Bounds, Capslock, DisplayLink, ExternalPaths,
    FileDropEvent, ForegroundExecutor, KeyDownEvent, Keystroke, Modifiers, ModifiersChangedEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, PlatformAtlas,
    PlatformDisplay, PlatformInput, PlatformNativeAlert, PlatformNativeAlertStyle,
    PlatformNativePanel, PlatformNativePanelAnchor, PlatformNativePanelLevel,
    PlatformNativePanelMaterial, PlatformNativePanelStyle,
    PlatformNativePopover, PlatformNativePopoverAnchor,
    PlatformNativePopoverContentItem, PlatformNativeToolbar, PlatformNativeToolbarDisplayMode,
    PlatformNativeToolbarItem, PlatformNativeToolbarMenuItemData, PlatformNativeToolbarSizeMode,
    PlatformWindow, Point, PromptButton,
    PromptLevel, RequestFrameOptions, SharedString, Size, SystemWindowTab, WindowAppearance,
    WindowBackgroundAppearance, WindowBounds, WindowControlArea, WindowKind, WindowParams,
    dispatch_get_main_queue, dispatch_sys::dispatch_async_f, platform::PlatformInputHandler, point,
    px, size,
};
#[cfg(any(test, feature = "test-support"))]
use anyhow::Result;
use block::ConcreteBlock;
use cocoa::{
    appkit::{
        NSAppKitVersionNumber, NSAppKitVersionNumber12_0, NSApplication, NSBackingStoreBuffered,
        NSColor, NSEvent, NSEventModifierFlags, NSFilenamesPboardType, NSPasteboard, NSScreen,
        NSView, NSViewHeightSizable, NSViewWidthSizable, NSVisualEffectMaterial,
        NSVisualEffectState, NSVisualEffectView, NSWindow, NSWindowButton,
        NSWindowCollectionBehavior, NSWindowOcclusionState, NSWindowOrderingMode,
        NSWindowStyleMask, NSWindowTitleVisibility,
    },
    base::{id, nil},
    foundation::{
        NSArray, NSAutoreleasePool, NSDictionary, NSFastEnumeration, NSInteger, NSNotFound,
        NSOperatingSystemVersion, NSPoint, NSProcessInfo, NSRect, NSSize, NSString, NSUInteger,
        NSUserDefaults,
    },
};
#[cfg(any(test, feature = "test-support"))]
use image::RgbaImage;

use core_graphics::display::{CGDirectDisplayID, CGPoint, CGRect};
use ctor::ctor;
use futures::channel::oneshot;
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{BOOL, Class, NO, Object, Protocol, Sel, YES},
    sel, sel_impl,
};
use parking_lot::Mutex;
use raw_window_handle as rwh;
use smallvec::SmallVec;
use std::{
    cell::Cell,
    ffi::{CStr, c_void},
    mem,
    ops::Range,
    path::PathBuf,
    ptr::{self, NonNull},
    rc::Rc,
    sync::{Arc, Weak},
    time::Duration,
};
use util::ResultExt;

const WINDOW_STATE_IVAR: &str = "windowState";
const TOOLBAR_STATE_IVAR: &str = "toolbarState";

static mut WINDOW_CLASS: *const Class = ptr::null();
static mut PANEL_CLASS: *const Class = ptr::null();
static mut VIEW_CLASS: *const Class = ptr::null();
static mut BLURRED_VIEW_CLASS: *const Class = ptr::null();
static mut TOOLBAR_DELEGATE_CLASS: *const Class = ptr::null();

#[allow(non_upper_case_globals)]
const NSWindowStyleMaskNonactivatingPanel: NSWindowStyleMask =
    NSWindowStyleMask::from_bits_retain(1 << 7);
// WindowLevel const value ref: https://docs.rs/core-graphics2/0.4.1/src/core_graphics2/window_level.rs.html
#[allow(non_upper_case_globals)]
const NSNormalWindowLevel: NSInteger = 0;
#[allow(non_upper_case_globals)]
const NSFloatingWindowLevel: NSInteger = 3;
#[allow(non_upper_case_globals)]
const NSPopUpWindowLevel: NSInteger = 101;
#[allow(non_upper_case_globals)]
const NSTrackingMouseEnteredAndExited: NSUInteger = 0x01;
#[allow(non_upper_case_globals)]
const NSTrackingMouseMoved: NSUInteger = 0x02;
#[allow(non_upper_case_globals)]
const NSTrackingActiveAlways: NSUInteger = 0x80;
#[allow(non_upper_case_globals)]
const NSTrackingInVisibleRect: NSUInteger = 0x200;
#[allow(non_upper_case_globals)]
const NSWindowAnimationBehaviorUtilityWindow: NSInteger = 4;
#[allow(non_upper_case_globals)]
const NSViewLayerContentsRedrawDuringViewResize: NSInteger = 2;
#[allow(non_upper_case_globals)]
const NSToolbarDisplayModeDefault: NSUInteger = 0;
#[allow(non_upper_case_globals)]
const NSToolbarDisplayModeIconAndLabel: NSUInteger = 1;
#[allow(non_upper_case_globals)]
const NSToolbarDisplayModeIconOnly: NSUInteger = 2;
#[allow(non_upper_case_globals)]
const NSToolbarDisplayModeLabelOnly: NSUInteger = 3;
#[allow(non_upper_case_globals)]
const NSToolbarSizeModeDefault: NSUInteger = 0;
#[allow(non_upper_case_globals)]
const NSToolbarSizeModeRegular: NSUInteger = 1;
#[allow(non_upper_case_globals)]
const NSToolbarSizeModeSmall: NSUInteger = 2;
// https://developer.apple.com/documentation/appkit/nsdragoperation
type NSDragOperation = NSUInteger;
#[allow(non_upper_case_globals)]
const NSDragOperationNone: NSDragOperation = 0;
#[allow(non_upper_case_globals)]
const NSDragOperationCopy: NSDragOperation = 1;
#[derive(PartialEq)]
pub enum UserTabbingPreference {
    Never,
    Always,
    InFullScreen,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    // Widely used private APIs; Apple uses them for their Terminal.app.
    fn CGSMainConnectionID() -> id;
    fn CGSSetWindowBackgroundBlurRadius(
        connection_id: id,
        window_id: NSInteger,
        radius: i64,
    ) -> i32;

    static NSToolbarFlexibleSpaceItemIdentifier: id;
    static NSToolbarSpaceItemIdentifier: id;
    static NSToolbarToggleSidebarItemIdentifier: id;
    static NSToolbarSidebarTrackingSeparatorItemIdentifier: id;
}

#[ctor]
unsafe fn build_classes() {
    unsafe {
        WINDOW_CLASS = build_window_class("GPUIWindow", class!(NSWindow));
        PANEL_CLASS = build_window_class("GPUIPanel", class!(NSPanel));
        VIEW_CLASS = {
            let mut decl = ClassDecl::new("GPUIView", class!(NSView)).unwrap();
            decl.add_ivar::<*mut c_void>(WINDOW_STATE_IVAR);
            unsafe {
                decl.add_method(sel!(dealloc), dealloc_view as extern "C" fn(&Object, Sel));

                decl.add_method(
                    sel!(performKeyEquivalent:),
                    handle_key_equivalent as extern "C" fn(&Object, Sel, id) -> BOOL,
                );
                decl.add_method(
                    sel!(keyDown:),
                    handle_key_down as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(keyUp:),
                    handle_key_up as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(mouseDown:),
                    handle_view_event as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(mouseUp:),
                    handle_view_event as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(rightMouseDown:),
                    handle_view_event as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(rightMouseUp:),
                    handle_view_event as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(otherMouseDown:),
                    handle_view_event as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(otherMouseUp:),
                    handle_view_event as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(mouseMoved:),
                    handle_view_event as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(pressureChangeWithEvent:),
                    handle_view_event as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(mouseExited:),
                    handle_view_event as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(mouseDragged:),
                    handle_view_event as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(scrollWheel:),
                    handle_view_event as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(swipeWithEvent:),
                    handle_view_event as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(flagsChanged:),
                    handle_view_event as extern "C" fn(&Object, Sel, id),
                );

                decl.add_method(
                    sel!(makeBackingLayer),
                    make_backing_layer as extern "C" fn(&Object, Sel) -> id,
                );

                decl.add_protocol(Protocol::get("CALayerDelegate").unwrap());
                decl.add_method(
                    sel!(viewDidChangeBackingProperties),
                    view_did_change_backing_properties as extern "C" fn(&Object, Sel),
                );
                decl.add_method(
                    sel!(setFrameSize:),
                    set_frame_size as extern "C" fn(&Object, Sel, NSSize),
                );
                decl.add_method(
                    sel!(displayLayer:),
                    display_layer as extern "C" fn(&Object, Sel, id),
                );

                decl.add_protocol(Protocol::get("NSTextInputClient").unwrap());
                decl.add_method(
                    sel!(validAttributesForMarkedText),
                    valid_attributes_for_marked_text as extern "C" fn(&Object, Sel) -> id,
                );
                decl.add_method(
                    sel!(hasMarkedText),
                    has_marked_text as extern "C" fn(&Object, Sel) -> BOOL,
                );
                decl.add_method(
                    sel!(markedRange),
                    marked_range as extern "C" fn(&Object, Sel) -> NSRange,
                );
                decl.add_method(
                    sel!(selectedRange),
                    selected_range as extern "C" fn(&Object, Sel) -> NSRange,
                );
                decl.add_method(
                    sel!(firstRectForCharacterRange:actualRange:),
                    first_rect_for_character_range
                        as extern "C" fn(&Object, Sel, NSRange, id) -> NSRect,
                );
                decl.add_method(
                    sel!(insertText:replacementRange:),
                    insert_text as extern "C" fn(&Object, Sel, id, NSRange),
                );
                decl.add_method(
                    sel!(setMarkedText:selectedRange:replacementRange:),
                    set_marked_text as extern "C" fn(&Object, Sel, id, NSRange, NSRange),
                );
                decl.add_method(sel!(unmarkText), unmark_text as extern "C" fn(&Object, Sel));
                decl.add_method(
                    sel!(attributedSubstringForProposedRange:actualRange:),
                    attributed_substring_for_proposed_range
                        as extern "C" fn(&Object, Sel, NSRange, *mut c_void) -> id,
                );
                decl.add_method(
                    sel!(viewDidChangeEffectiveAppearance),
                    view_did_change_effective_appearance as extern "C" fn(&Object, Sel),
                );

                // Suppress beep on keystrokes with modifier keys.
                decl.add_method(
                    sel!(doCommandBySelector:),
                    do_command_by_selector as extern "C" fn(&Object, Sel, Sel),
                );

                decl.add_method(
                    sel!(acceptsFirstMouse:),
                    accepts_first_mouse as extern "C" fn(&Object, Sel, id) -> BOOL,
                );

                decl.add_method(
                    sel!(isFlipped),
                    view_is_flipped as extern "C" fn(&Object, Sel) -> BOOL,
                );

                decl.add_method(
                    sel!(characterIndexForPoint:),
                    character_index_for_point as extern "C" fn(&Object, Sel, NSPoint) -> u64,
                );
            }
            decl.register()
        };
        BLURRED_VIEW_CLASS = {
            let mut decl = ClassDecl::new("BlurredView", class!(NSVisualEffectView)).unwrap();
            unsafe {
                decl.add_method(
                    sel!(initWithFrame:),
                    blurred_view_init_with_frame as extern "C" fn(&Object, Sel, NSRect) -> id,
                );
                decl.add_method(
                    sel!(updateLayer),
                    blurred_view_update_layer as extern "C" fn(&Object, Sel),
                );
                decl.register()
            }
        };
        TOOLBAR_DELEGATE_CLASS = {
            let mut decl = ClassDecl::new("GPUINativeToolbarDelegate", class!(NSObject)).unwrap();
            decl.add_ivar::<*mut c_void>(TOOLBAR_STATE_IVAR);
            decl.add_method(
                sel!(toolbarAllowedItemIdentifiers:),
                toolbar_allowed_item_identifiers as extern "C" fn(&Object, Sel, id) -> id,
            );
            decl.add_method(
                sel!(toolbarDefaultItemIdentifiers:),
                toolbar_default_item_identifiers as extern "C" fn(&Object, Sel, id) -> id,
            );
            decl.add_method(
                sel!(toolbar:itemForItemIdentifier:willBeInsertedIntoToolbar:),
                toolbar_item_for_identifier as extern "C" fn(&Object, Sel, id, id, BOOL) -> id,
            );
            decl.register()
        };
    }
}


unsafe fn build_window_class(name: &'static str, superclass: &Class) -> *const Class {
    unsafe {
        let mut decl = ClassDecl::new(name, superclass).unwrap();
        decl.add_ivar::<*mut c_void>(WINDOW_STATE_IVAR);
        decl.add_method(sel!(dealloc), dealloc_window as extern "C" fn(&Object, Sel));

        decl.add_method(
            sel!(canBecomeMainWindow),
            yes as extern "C" fn(&Object, Sel) -> BOOL,
        );
        decl.add_method(
            sel!(canBecomeKeyWindow),
            yes as extern "C" fn(&Object, Sel) -> BOOL,
        );
        decl.add_method(
            sel!(windowDidResize:),
            window_did_resize as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(windowDidChangeOcclusionState:),
            window_did_change_occlusion_state as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(windowWillEnterFullScreen:),
            window_will_enter_fullscreen as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(windowWillExitFullScreen:),
            window_will_exit_fullscreen as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(windowDidMove:),
            window_did_move as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(windowDidChangeScreen:),
            window_did_change_screen as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(windowDidBecomeKey:),
            window_did_change_key_status as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(windowDidResignKey:),
            window_did_change_key_status as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(windowShouldClose:),
            window_should_close as extern "C" fn(&Object, Sel, id) -> BOOL,
        );

        decl.add_method(sel!(close), close_window as extern "C" fn(&Object, Sel));

        decl.add_method(
            sel!(draggingEntered:),
            dragging_entered as extern "C" fn(&Object, Sel, id) -> NSDragOperation,
        );
        decl.add_method(
            sel!(draggingUpdated:),
            dragging_updated as extern "C" fn(&Object, Sel, id) -> NSDragOperation,
        );
        decl.add_method(
            sel!(draggingExited:),
            dragging_exited as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(performDragOperation:),
            perform_drag_operation as extern "C" fn(&Object, Sel, id) -> BOOL,
        );
        decl.add_method(
            sel!(concludeDragOperation:),
            conclude_drag_operation as extern "C" fn(&Object, Sel, id),
        );

        decl.add_method(
            sel!(addTitlebarAccessoryViewController:),
            add_titlebar_accessory_view_controller as extern "C" fn(&Object, Sel, id),
        );

        decl.add_method(
            sel!(moveTabToNewWindow:),
            move_tab_to_new_window as extern "C" fn(&Object, Sel, id),
        );

        decl.add_method(
            sel!(mergeAllWindows:),
            merge_all_windows as extern "C" fn(&Object, Sel, id),
        );

        decl.add_method(
            sel!(selectNextTab:),
            select_next_tab as extern "C" fn(&Object, Sel, id),
        );

        decl.add_method(
            sel!(selectPreviousTab:),
            select_previous_tab as extern "C" fn(&Object, Sel, id),
        );

        decl.add_method(
            sel!(toggleTabBar:),
            toggle_tab_bar as extern "C" fn(&Object, Sel, id),
        );

        decl.register()
    }
}

enum ToolbarNativeResource {
    Button { button: id, target: *mut c_void },
    SearchField { field: id, delegate: *mut c_void },
    SegmentedControl { control: id, target: *mut c_void },
    PopUpButton { popup: id, target: *mut c_void },
    ComboBox { combo: id, delegate: *mut c_void },
    MenuButton { target: *mut c_void },
}

struct ToolbarState {
    allowed_item_identifiers: Vec<SharedString>,
    default_item_identifiers: Vec<SharedString>,
    items: Vec<PlatformNativeToolbarItem>,
    resources: Vec<ToolbarNativeResource>,
}

impl ToolbarState {
    fn item_for_identifier(&self, identifier: &str) -> Option<&PlatformNativeToolbarItem> {
        self.items.iter().find(|item| match item {
            PlatformNativeToolbarItem::Button(item) => item.id.as_ref() == identifier,
            PlatformNativeToolbarItem::SearchField(item) => item.id.as_ref() == identifier,
            PlatformNativeToolbarItem::SegmentedControl(item) => item.id.as_ref() == identifier,
            PlatformNativeToolbarItem::PopUpButton(item) => item.id.as_ref() == identifier,
            PlatformNativeToolbarItem::ComboBox(item) => item.id.as_ref() == identifier,
            PlatformNativeToolbarItem::MenuButton(item) => item.id.as_ref() == identifier,
            PlatformNativeToolbarItem::Label(item) => item.id.as_ref() == identifier,
            PlatformNativeToolbarItem::Space
            | PlatformNativeToolbarItem::FlexibleSpace
            | PlatformNativeToolbarItem::SidebarToggle
            | PlatformNativeToolbarItem::SidebarTrackingSeparator => false,
        })
    }
}

struct MacToolbarState {
    toolbar: id,
    delegate: id,
    state_ptr: *mut c_void,
}

impl MacToolbarState {
    unsafe fn cleanup(&mut self) {
        unsafe {
            let state = Box::from_raw(self.state_ptr as *mut ToolbarState);
            for resource in state.resources {
                match resource {
                    ToolbarNativeResource::Button { button, target } => {
                        crate::platform::native_controls::release_native_button_target(target);
                        crate::platform::native_controls::release_native_button(button);
                    }
                    ToolbarNativeResource::SearchField { field, delegate } => {
                        crate::platform::native_controls::release_native_text_field_delegate(
                            delegate,
                        );
                        crate::platform::native_controls::release_native_search_field(field);
                    }
                    ToolbarNativeResource::SegmentedControl { control, target } => {
                        crate::platform::native_controls::release_native_segmented_target(target);
                        crate::platform::native_controls::release_native_segmented_control(control);
                    }
                    ToolbarNativeResource::PopUpButton { popup, target } => {
                        crate::platform::native_controls::release_native_popup_target(target);
                        crate::platform::native_controls::release_native_popup_button(popup);
                    }
                    ToolbarNativeResource::ComboBox { combo, delegate } => {
                        crate::platform::native_controls::release_native_combo_box_delegate(
                            delegate,
                        );
                        crate::platform::native_controls::release_native_combo_box(combo);
                    }
                    ToolbarNativeResource::MenuButton { target } => {
                        crate::platform::native_controls::release_native_menu_button_target(
                            target,
                        );
                    }
                }
            }

            let _: () = msg_send![self.toolbar, setDelegate: nil];
            let _: () = msg_send![self.delegate, release];
            let _: () = msg_send![self.toolbar, release];
        }
    }
}

pub(crate) struct MacWindowState {
    handle: AnyWindowHandle,
    foreground_executor: ForegroundExecutor,
    background_executor: BackgroundExecutor,
    native_window: id,
    pub(crate) native_view: NonNull<Object>,
    blurred_view: Option<id>,
    background_appearance: WindowBackgroundAppearance,
    display_link: Option<DisplayLink>,
    renderer: renderer::Renderer,
    request_frame_callback: Option<Box<dyn FnMut(RequestFrameOptions)>>,
    pub(crate) event_callback: Option<Box<dyn FnMut(PlatformInput) -> crate::DispatchEventResult>>,
    pub(crate) surface_event_callback: Option<Box<dyn FnMut(*mut c_void, PlatformInput) -> crate::DispatchEventResult>>,
    activate_callback: Option<Box<dyn FnMut(bool)>>,
    resize_callback: Option<Box<dyn FnMut(Size<Pixels>, f32)>>,
    moved_callback: Option<Box<dyn FnMut()>>,
    should_close_callback: Option<Box<dyn FnMut() -> bool>>,
    close_callback: Option<Box<dyn FnOnce()>>,
    appearance_changed_callback: Option<Box<dyn FnMut()>>,
    input_handler: Option<PlatformInputHandler>,
    last_key_equivalent: Option<KeyDownEvent>,
    synthetic_drag_counter: usize,
    traffic_light_position: Option<Point<Pixels>>,
    transparent_titlebar: bool,
    previous_modifiers_changed_event: Option<PlatformInput>,
    keystroke_for_do_command: Option<Keystroke>,
    do_command_handled: Option<bool>,
    external_files_dragged: bool,
    // Whether the next left-mouse click is also the focusing click.
    first_mouse: bool,
    fullscreen_restore_bounds: Bounds<Pixels>,
    move_tab_to_new_window_callback: Option<Box<dyn FnMut()>>,
    merge_all_windows_callback: Option<Box<dyn FnMut()>>,
    select_next_tab_callback: Option<Box<dyn FnMut()>>,
    select_previous_tab_callback: Option<Box<dyn FnMut()>>,
    toggle_tab_bar_callback: Option<Box<dyn FnMut()>>,
    toolbar: Option<MacToolbarState>,
    popover: Option<MacPopoverState>,
    panel: Option<MacPanelState>,
    activated_least_once: bool,
    // The parent window if this window is a sheet (Dialog kind)
    sheet_parent: Option<id>,
}

struct MacPopoverState {
    popover: id,
    delegate_ptr: *mut c_void,
    button_targets: Vec<*mut c_void>,
    switch_targets: Vec<*mut c_void>,
    checkbox_targets: Vec<*mut c_void>,
    hover_row_targets: Vec<*mut c_void>,
}

struct MacPanelState {
    panel: id,
    delegate_ptr: *mut c_void,
    button_targets: Vec<*mut c_void>,
    switch_targets: Vec<*mut c_void>,
    checkbox_targets: Vec<*mut c_void>,
    hover_row_targets: Vec<*mut c_void>,
}

impl MacWindowState {
    fn move_traffic_light(&self) {
        if let Some(traffic_light_position) = self.traffic_light_position {
            if self.is_fullscreen() {
                // Moving traffic lights while fullscreen doesn't work,
                // see https://github.com/zed-industries/zed/issues/4712
                return;
            }

            // Traffic lights should be anchored to the native titlebar region.
            // The effective overlap height can change when the content view is
            // reparented into split-view panes, which would make button
            // placement drift during sidebar/mode transitions.
            let titlebar_height = self.native_titlebar_height();

            unsafe {
                let close_button: id = msg_send![
                    self.native_window,
                    standardWindowButton: NSWindowButton::NSWindowCloseButton
                ];
                let min_button: id = msg_send![
                    self.native_window,
                    standardWindowButton: NSWindowButton::NSWindowMiniaturizeButton
                ];
                let zoom_button: id = msg_send![
                    self.native_window,
                    standardWindowButton: NSWindowButton::NSWindowZoomButton
                ];

                let mut close_button_frame: CGRect = msg_send![close_button, frame];
                let mut min_button_frame: CGRect = msg_send![min_button, frame];
                let mut zoom_button_frame: CGRect = msg_send![zoom_button, frame];
                let (button_container_height, button_container_is_flipped) = {
                    let superview: id = msg_send![close_button, superview];
                    if superview != nil {
                        let is_flipped: BOOL = msg_send![superview, isFlipped];
                        (
                            NSView::frame(superview).size.height as f32,
                            is_flipped == YES,
                        )
                    } else {
                        (titlebar_height.0, false)
                    }
                };
                let y = if button_container_is_flipped {
                    traffic_light_position.y
                } else {
                    px(button_container_height)
                        - traffic_light_position.y
                        - px(close_button_frame.size.height as f32)
                };
                let mut origin = point(
                    traffic_light_position.x,
                    y,
                );
                let button_spacing =
                    px((min_button_frame.origin.x - close_button_frame.origin.x) as f32);

                close_button_frame.origin = CGPoint::new(origin.x.into(), origin.y.into());
                let _: () = msg_send![close_button, setFrame: close_button_frame];
                origin.x += button_spacing;

                min_button_frame.origin = CGPoint::new(origin.x.into(), origin.y.into());
                let _: () = msg_send![min_button, setFrame: min_button_frame];
                origin.x += button_spacing;

                zoom_button_frame.origin = CGPoint::new(origin.x.into(), origin.y.into());
                let _: () = msg_send![zoom_button, setFrame: zoom_button_frame];
                origin.x += button_spacing;
            }
        }
    }

    fn start_display_link(&mut self) {
        self.stop_display_link();
        unsafe {
            if !self
                .native_window
                .occlusionState()
                .contains(NSWindowOcclusionState::NSWindowOcclusionStateVisible)
            {
                return;
            }
        }
        let display_id = unsafe { display_id_for_screen(self.native_window.screen()) };
        if let Some(mut display_link) =
            DisplayLink::new(display_id, self.native_view.as_ptr() as *mut c_void, step).log_err()
        {
            display_link.start().log_err();
            self.display_link = Some(display_link);
        }
    }

    fn stop_display_link(&mut self) {
        self.display_link = None;
    }

    fn is_maximized(&self) -> bool {
        unsafe {
            let bounds = self.bounds();
            let screen_size = self.native_window.screen().visibleFrame().into();
            bounds.size == screen_size
        }
    }

    fn is_fullscreen(&self) -> bool {
        unsafe {
            let style_mask = self.native_window.styleMask();
            style_mask.contains(NSWindowStyleMask::NSFullScreenWindowMask)
        }
    }

    fn bounds(&self) -> Bounds<Pixels> {
        let mut window_frame = unsafe { NSWindow::frame(self.native_window) };
        let screen = unsafe { NSWindow::screen(self.native_window) };
        if screen == nil {
            return Bounds::new(point(px(0.), px(0.)), crate::DEFAULT_WINDOW_SIZE);
        }
        let screen_frame = unsafe { NSScreen::frame(screen) };

        // Flip the y coordinate to be top-left origin
        window_frame.origin.y =
            screen_frame.size.height - window_frame.origin.y - window_frame.size.height;

        Bounds::new(
            point(
                px((window_frame.origin.x - screen_frame.origin.x) as f32),
                px((window_frame.origin.y + screen_frame.origin.y) as f32),
            ),
            size(
                px(window_frame.size.width as f32),
                px(window_frame.size.height as f32),
            ),
        )
    }

    fn content_size(&self) -> Size<Pixels> {
        // Use the native_view's frame rather than the window's contentView frame.
        // When a native sidebar reparents the native_view into a split-view pane,
        // the native_view's frame reflects the actual rendering area.
        let NSSize { width, height, .. } =
            unsafe { NSView::frame(self.native_view.as_ptr() as id) }.size;
        size(px(width as f32), px(height as f32))
    }

    fn scale_factor(&self) -> f32 {
        get_scale_factor(self.native_window)
    }

    fn native_titlebar_height(&self) -> Pixels {
        unsafe {
            let frame = NSWindow::frame(self.native_window);
            let content_layout_rect: CGRect = msg_send![self.native_window, contentLayoutRect];
            px((frame.size.height - content_layout_rect.size.height) as f32)
        }
    }

    fn titlebar_height(&self) -> Pixels {
        unsafe {
            let native_titlebar = self.native_titlebar_height().0 as f64;

            // When the native_view is reparented into a split-view pane
            // that doesn't extend behind the titlebar (e.g. the
            // NSSplitViewController adjusts for safe area), the view's
            // height is less than the contentView's height. The difference
            // is the pane's offset from the full content area, which
            // reduces the effective titlebar overlap.
            let view_height =
                NSView::frame(self.native_view.as_ptr() as id).size.height;
            let content_view: id = msg_send![self.native_window, contentView];
            let content_view_height = NSView::frame(content_view).size.height;
            let pane_offset = content_view_height - view_height;
            let effective = (native_titlebar - pane_offset).max(0.0);

            px(effective as f32)
        }
    }

    fn window_bounds(&self) -> WindowBounds {
        if self.is_fullscreen() {
            WindowBounds::Fullscreen(self.fullscreen_restore_bounds)
        } else {
            WindowBounds::Windowed(self.bounds())
        }
    }
}

unsafe impl Send for MacWindowState {}

pub(crate) struct MacWindow(Arc<Mutex<MacWindowState>>);

impl MacWindow {
    pub fn open(
        handle: AnyWindowHandle,
        WindowParams {
            bounds,
            titlebar,
            kind,
            is_movable,
            is_resizable,
            is_minimizable,
            focus,
            show,
            display_id,
            window_min_size,
            tabbing_identifier,
        }: WindowParams,
        foreground_executor: ForegroundExecutor,
        background_executor: BackgroundExecutor,
        renderer_context: renderer::Context,
    ) -> Self {
        unsafe {
            let pool = NSAutoreleasePool::new(nil);

            let allows_automatic_window_tabbing = tabbing_identifier.is_some();
            if allows_automatic_window_tabbing {
                let () = msg_send![class!(NSWindow), setAllowsAutomaticWindowTabbing: YES];
            } else {
                let () = msg_send![class!(NSWindow), setAllowsAutomaticWindowTabbing: NO];
            }

            let mut style_mask;
            if let Some(titlebar) = titlebar.as_ref() {
                style_mask =
                    NSWindowStyleMask::NSClosableWindowMask | NSWindowStyleMask::NSTitledWindowMask;

                if is_resizable {
                    style_mask |= NSWindowStyleMask::NSResizableWindowMask;
                }

                if is_minimizable {
                    style_mask |= NSWindowStyleMask::NSMiniaturizableWindowMask;
                }

                if titlebar.appears_transparent {
                    style_mask |= NSWindowStyleMask::NSFullSizeContentViewWindowMask;
                }
            } else {
                style_mask = NSWindowStyleMask::NSTitledWindowMask
                    | NSWindowStyleMask::NSFullSizeContentViewWindowMask;
            }

            let native_window: id = match kind {
                WindowKind::Normal => {
                    msg_send![WINDOW_CLASS, alloc]
                }
                WindowKind::PopUp => {
                    style_mask |= NSWindowStyleMaskNonactivatingPanel;
                    msg_send![PANEL_CLASS, alloc]
                }
                WindowKind::Floating | WindowKind::Dialog => {
                    msg_send![PANEL_CLASS, alloc]
                }
            };

            let display = display_id
                .and_then(MacDisplay::find_by_id)
                .unwrap_or_else(MacDisplay::primary);

            let mut target_screen = nil;
            let mut screen_frame = None;

            let screens = NSScreen::screens(nil);
            let count: u64 = cocoa::foundation::NSArray::count(screens);
            for i in 0..count {
                let screen = cocoa::foundation::NSArray::objectAtIndex(screens, i);
                let frame = NSScreen::frame(screen);
                let display_id = display_id_for_screen(screen);
                if display_id == display.0 {
                    screen_frame = Some(frame);
                    target_screen = screen;
                }
            }

            let screen_frame = screen_frame.unwrap_or_else(|| {
                let screen = NSScreen::mainScreen(nil);
                target_screen = screen;
                NSScreen::frame(screen)
            });

            let window_rect = NSRect::new(
                NSPoint::new(
                    screen_frame.origin.x + bounds.origin.x.0 as f64,
                    screen_frame.origin.y
                        + (display.bounds().size.height - bounds.origin.y).0 as f64,
                ),
                NSSize::new(bounds.size.width.0 as f64, bounds.size.height.0 as f64),
            );

            let native_window = native_window.initWithContentRect_styleMask_backing_defer_screen_(
                window_rect,
                style_mask,
                NSBackingStoreBuffered,
                NO,
                target_screen,
            );
            assert!(!native_window.is_null());
            let () = msg_send![
                native_window,
                registerForDraggedTypes:
                    NSArray::arrayWithObject(nil, NSFilenamesPboardType)
            ];
            let () = msg_send![
                native_window,
                setReleasedWhenClosed: NO
            ];

            let content_view = native_window.contentView();
            let native_view: id = msg_send![VIEW_CLASS, alloc];
            let native_view = NSView::initWithFrame_(native_view, NSView::bounds(content_view));
            assert!(!native_view.is_null());

            let mut window = Self(Arc::new(Mutex::new(MacWindowState {
                handle,
                foreground_executor,
                background_executor,
                native_window,
                native_view: NonNull::new_unchecked(native_view),
                blurred_view: None,
                background_appearance: WindowBackgroundAppearance::Opaque,
                display_link: None,
                renderer: renderer::new_renderer(
                    renderer_context,
                    native_window as *mut _,
                    native_view as *mut _,
                    bounds.size.map(|pixels| pixels.0),
                    false,
                ),
                request_frame_callback: None,
                event_callback: None,
                surface_event_callback: None,
                activate_callback: None,
                resize_callback: None,
                moved_callback: None,
                should_close_callback: None,
                close_callback: None,
                appearance_changed_callback: None,
                input_handler: None,
                last_key_equivalent: None,
                synthetic_drag_counter: 0,
                traffic_light_position: titlebar
                    .as_ref()
                    .and_then(|titlebar| titlebar.traffic_light_position),
                transparent_titlebar: titlebar
                    .as_ref()
                    .is_none_or(|titlebar| titlebar.appears_transparent),
                previous_modifiers_changed_event: None,
                keystroke_for_do_command: None,
                do_command_handled: None,
                external_files_dragged: false,
                first_mouse: false,
                fullscreen_restore_bounds: Bounds::default(),
                move_tab_to_new_window_callback: None,
                merge_all_windows_callback: None,
                select_next_tab_callback: None,
                select_previous_tab_callback: None,
                toggle_tab_bar_callback: None,
                toolbar: None,
                popover: None,
                panel: None,
                activated_least_once: false,
                sheet_parent: None,
            })));

            (*native_window).set_ivar(
                WINDOW_STATE_IVAR,
                Arc::into_raw(window.0.clone()) as *const c_void,
            );
            native_window.setDelegate_(native_window);
            (*native_view).set_ivar(
                WINDOW_STATE_IVAR,
                Arc::into_raw(window.0.clone()) as *const c_void,
            );

            if let Some(title) = titlebar
                .as_ref()
                .and_then(|t| t.title.as_ref().map(AsRef::as_ref))
            {
                window.set_title(title);
            }

            native_window.setMovable_(is_movable as BOOL);

            if let Some(window_min_size) = window_min_size {
                native_window.setContentMinSize_(NSSize {
                    width: window_min_size.width.to_f64(),
                    height: window_min_size.height.to_f64(),
                });
            }

            if titlebar.is_none_or(|titlebar| titlebar.appears_transparent) {
                native_window.setTitlebarAppearsTransparent_(YES);
                native_window.setTitleVisibility_(NSWindowTitleVisibility::NSWindowTitleHidden);
            }

            native_view.setAutoresizingMask_(NSViewWidthSizable | NSViewHeightSizable);
            native_view.setWantsBestResolutionOpenGLSurface_(YES);

            // From winit crate: On Mojave, views automatically become layer-backed shortly after
            // being added to a native_window. Changing the layer-backedness of a view breaks the
            // association between the view and its associated OpenGL context. To work around this,
            // on we explicitly make the view layer-backed up front so that AppKit doesn't do it
            // itself and break the association with its context.
            native_view.setWantsLayer(YES);
            let _: () = msg_send![
            native_view,
            setLayerContentsRedrawPolicy: NSViewLayerContentsRedrawDuringViewResize
            ];

            content_view.addSubview_(native_view.autorelease());
            native_window.makeFirstResponder_(native_view);

            let app: id = NSApplication::sharedApplication(nil);
            let main_window: id = msg_send![app, mainWindow];
            let mut sheet_parent = None;

            match kind {
                WindowKind::Normal | WindowKind::Floating => {
                    if kind == WindowKind::Floating {
                        // Let the window float keep above normal windows.
                        native_window.setLevel_(NSFloatingWindowLevel);
                    } else {
                        native_window.setLevel_(NSNormalWindowLevel);
                    }
                    native_window.setAcceptsMouseMovedEvents_(YES);

                    // Add a tracking area so the view receives mouseMoved:
                    // events based on mouse position rather than first-responder
                    // status. When a native sidebar reparents the view into an
                    // NSSplitView, removeFromSuperview causes it to resign first
                    // responder, and mouseMoved: would stop being delivered
                    // without a tracking area.
                    let tracking_area: id = msg_send![class!(NSTrackingArea), alloc];
                    let _: () = msg_send![
                        tracking_area,
                        initWithRect: NSRect::new(NSPoint::new(0., 0.), NSSize::new(0., 0.))
                        options: NSTrackingMouseMoved | NSTrackingActiveAlways | NSTrackingInVisibleRect
                        owner: native_view
                        userInfo: nil
                    ];
                    let _: () =
                        msg_send![native_view, addTrackingArea: tracking_area.autorelease()];

                    if let Some(tabbing_identifier) = tabbing_identifier {
                        let tabbing_id = ns_string(tabbing_identifier.as_str());
                        let _: () = msg_send![native_window, setTabbingIdentifier: tabbing_id];
                    } else {
                        let _: () = msg_send![native_window, setTabbingIdentifier:nil];
                    }
                }
                WindowKind::PopUp => {
                    // Use a tracking area to allow receiving MouseMoved events even when
                    // the window or application aren't active, which is often the case
                    // e.g. for notification windows.
                    let tracking_area: id = msg_send![class!(NSTrackingArea), alloc];
                    let _: () = msg_send![
                        tracking_area,
                        initWithRect: NSRect::new(NSPoint::new(0., 0.), NSSize::new(0., 0.))
                        options: NSTrackingMouseEnteredAndExited | NSTrackingMouseMoved | NSTrackingActiveAlways | NSTrackingInVisibleRect
                        owner: native_view
                        userInfo: nil
                    ];
                    let _: () =
                        msg_send![native_view, addTrackingArea: tracking_area.autorelease()];

                    native_window.setLevel_(NSPopUpWindowLevel);
                    let _: () = msg_send![
                        native_window,
                        setAnimationBehavior: NSWindowAnimationBehaviorUtilityWindow
                    ];
                    native_window.setCollectionBehavior_(
                        NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces |
                        NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary
                    );
                }
                WindowKind::Dialog => {
                    if !main_window.is_null() {
                        let parent = {
                            let active_sheet: id = msg_send![main_window, attachedSheet];
                            if active_sheet.is_null() {
                                main_window
                            } else {
                                active_sheet
                            }
                        };
                        let _: () =
                            msg_send![parent, beginSheet: native_window completionHandler: nil];
                        sheet_parent = Some(parent);
                    }
                }
            }

            if allows_automatic_window_tabbing
                && !main_window.is_null()
                && main_window != native_window
            {
                let main_window_is_fullscreen = main_window
                    .styleMask()
                    .contains(NSWindowStyleMask::NSFullScreenWindowMask);
                let user_tabbing_preference = Self::get_user_tabbing_preference()
                    .unwrap_or(UserTabbingPreference::InFullScreen);
                let should_add_as_tab = user_tabbing_preference == UserTabbingPreference::Always
                    || user_tabbing_preference == UserTabbingPreference::InFullScreen
                        && main_window_is_fullscreen;

                if should_add_as_tab {
                    let main_window_can_tab: BOOL =
                        msg_send![main_window, respondsToSelector: sel!(addTabbedWindow:ordered:)];
                    let main_window_visible: BOOL = msg_send![main_window, isVisible];

                    if main_window_can_tab == YES && main_window_visible == YES {
                        let _: () = msg_send![main_window, addTabbedWindow: native_window ordered: NSWindowOrderingMode::NSWindowAbove];

                        // Ensure the window is visible immediately after adding the tab, since the tab bar is updated with a new entry at this point.
                        // Note: Calling orderFront here can break fullscreen mode (makes fullscreen windows exit fullscreen), so only do this if the main window is not fullscreen.
                        if !main_window_is_fullscreen {
                            let _: () = msg_send![native_window, orderFront: nil];
                        }
                    }
                }
            }

            if focus && show {
                native_window.makeKeyAndOrderFront_(nil);
            } else if show {
                native_window.orderFront_(nil);
            }

            // Set the initial position of the window to the specified origin.
            // Although we already specified the position using `initWithContentRect_styleMask_backing_defer_screen_`,
            // the window position might be incorrect if the main screen (the screen that contains the window that has focus)
            //  is different from the primary screen.
            NSWindow::setFrameTopLeftPoint_(native_window, window_rect.origin);
            {
                let mut window_state = window.0.lock();
                window_state.move_traffic_light();
                window_state.sheet_parent = sheet_parent;
            }

            pool.drain();

            window
        }
    }

    pub fn active_window() -> Option<AnyWindowHandle> {
        unsafe {
            let app = NSApplication::sharedApplication(nil);
            let main_window: id = msg_send![app, mainWindow];
            if main_window.is_null() {
                return None;
            }

            if msg_send![main_window, isKindOfClass: WINDOW_CLASS] {
                let handle = get_window_state(&*main_window).lock().handle;
                Some(handle)
            } else {
                None
            }
        }
    }

    pub fn ordered_windows() -> Vec<AnyWindowHandle> {
        unsafe {
            let app = NSApplication::sharedApplication(nil);
            let windows: id = msg_send![app, orderedWindows];
            let count: NSUInteger = msg_send![windows, count];

            let mut window_handles = Vec::new();
            for i in 0..count {
                let window: id = msg_send![windows, objectAtIndex:i];
                if msg_send![window, isKindOfClass: WINDOW_CLASS] {
                    let handle = get_window_state(&*window).lock().handle;
                    window_handles.push(handle);
                }
            }

            window_handles
        }
    }

    pub fn get_user_tabbing_preference() -> Option<UserTabbingPreference> {
        unsafe {
            let defaults: id = NSUserDefaults::standardUserDefaults();
            let domain = ns_string("NSGlobalDomain");
            let key = ns_string("AppleWindowTabbingMode");

            let dict: id = msg_send![defaults, persistentDomainForName: domain];
            let value: id = if !dict.is_null() {
                msg_send![dict, objectForKey: key]
            } else {
                nil
            };

            let value_str = if !value.is_null() {
                CStr::from_ptr(NSString::UTF8String(value)).to_string_lossy()
            } else {
                "".into()
            };

            match value_str.as_ref() {
                "manual" => Some(UserTabbingPreference::Never),
                "always" => Some(UserTabbingPreference::Always),
                _ => Some(UserTabbingPreference::InFullScreen),
            }
        }
    }
}

impl Drop for MacWindow {
    fn drop(&mut self) {
        let (window, sheet_parent, foreground_executor, toolbar) = {
            let mut this = self.0.lock();
            this.renderer.destroy();
            let window = this.native_window;
            let sheet_parent = this.sheet_parent.take();
            let foreground_executor = this.foreground_executor.clone();
            let toolbar = this.toolbar.take();
            cleanup_popover_state(&mut this.popover);
            cleanup_panel_state(&mut this.panel);
            this.display_link.take();
            unsafe {
                this.native_window.setDelegate_(nil);
            }
            this.input_handler.take();
            (window, sheet_parent, foreground_executor, toolbar)
        };

        unsafe {
            clear_native_toolbar_for_window(window, toolbar);
        }

        foreground_executor
            .spawn(async move {
                unsafe {
                    if let Some(parent) = sheet_parent {
                        let _: () = msg_send![parent, endSheet: window];
                    }
                    window.close();
                    window.autorelease();
                }
            })
            .detach();
    }
}

impl PlatformWindow for MacWindow {
    fn bounds(&self) -> Bounds<Pixels> {
        self.0.as_ref().lock().bounds()
    }

    fn window_bounds(&self) -> WindowBounds {
        self.0.as_ref().lock().window_bounds()
    }

    fn is_maximized(&self) -> bool {
        self.0.as_ref().lock().is_maximized()
    }

    fn content_size(&self) -> Size<Pixels> {
        self.0.as_ref().lock().content_size()
    }

    fn titlebar_height(&self) -> Pixels {
        self.0.as_ref().lock().titlebar_height()
    }

    fn resize(&mut self, size: Size<Pixels>) {
        let this = self.0.lock();
        let window = this.native_window;
        this.foreground_executor
            .spawn(async move {
                unsafe {
                    window.setContentSize_(NSSize {
                        width: size.width.0 as f64,
                        height: size.height.0 as f64,
                    });
                }
            })
            .detach();
    }

    fn merge_all_windows(&self) {
        let native_window = self.0.lock().native_window;
        unsafe extern "C" fn merge_windows_async(context: *mut std::ffi::c_void) {
            let native_window = context as id;
            let _: () = msg_send![native_window, mergeAllWindows:nil];
        }

        unsafe {
            dispatch_async_f(
                dispatch_get_main_queue(),
                native_window as *mut std::ffi::c_void,
                Some(merge_windows_async),
            );
        }
    }

    fn move_tab_to_new_window(&self) {
        let native_window = self.0.lock().native_window;
        unsafe extern "C" fn move_tab_async(context: *mut std::ffi::c_void) {
            let native_window = context as id;
            let _: () = msg_send![native_window, moveTabToNewWindow:nil];
            let _: () = msg_send![native_window, makeKeyAndOrderFront: nil];
        }

        unsafe {
            dispatch_async_f(
                dispatch_get_main_queue(),
                native_window as *mut std::ffi::c_void,
                Some(move_tab_async),
            );
        }
    }

    fn toggle_window_tab_overview(&self) {
        let native_window = self.0.lock().native_window;
        unsafe {
            let _: () = msg_send![native_window, toggleTabOverview:nil];
        }
    }

    fn set_tabbing_identifier(&self, tabbing_identifier: Option<String>) {
        let native_window = self.0.lock().native_window;
        unsafe {
            let allows_automatic_window_tabbing = tabbing_identifier.is_some();
            if allows_automatic_window_tabbing {
                let () = msg_send![class!(NSWindow), setAllowsAutomaticWindowTabbing: YES];
            } else {
                let () = msg_send![class!(NSWindow), setAllowsAutomaticWindowTabbing: NO];
            }

            if let Some(tabbing_identifier) = tabbing_identifier {
                let tabbing_id = ns_string(tabbing_identifier.as_str());
                let _: () = msg_send![native_window, setTabbingIdentifier: tabbing_id];
            } else {
                let _: () = msg_send![native_window, setTabbingIdentifier:nil];
            }
        }
    }

    fn set_native_toolbar(&self, toolbar: Option<PlatformNativeToolbar>) {
        let (native_window, old_toolbar) = {
            let mut this = self.0.lock();
            (this.native_window, this.toolbar.take())
        };

        let new_toolbar = unsafe {
            clear_native_toolbar_for_window(native_window, old_toolbar);
            toolbar.map(|toolbar| build_native_toolbar_for_window(native_window, toolbar))
        };

        self.0.lock().toolbar = new_toolbar;
    }

    fn show_native_popover(
        &self,
        popover_config: PlatformNativePopover,
        anchor: PlatformNativePopoverAnchor,
    ) {
        let mut this = self.0.lock();

        // Dismiss any existing popover first
        cleanup_popover_state(&mut this.popover);

        let behavior = popover_config.behavior.to_raw();
        let (popover, delegate_ptr) = unsafe {
            crate::platform::native_controls::create_native_popover(
                popover_config.content_width,
                popover_config.content_height,
                behavior,
                popover_config.on_close,
                popover_config.on_show,
            )
        };

        let mut button_targets = Vec::new();
        let mut switch_targets = Vec::new();
        let mut checkbox_targets = Vec::new();
        let mut hover_row_targets = Vec::new();

        // Populate content view with items
        unsafe {
            let content_view =
                crate::platform::native_controls::get_native_popover_content_view(popover);
            let padding = 16.0;
            let content_width = popover_config.content_width - padding * 2.0;

            // First pass: calculate heights for each item
            let mut item_heights: Vec<f64> = Vec::new();
            for item in &popover_config.content_items {
                let height = match item {
                    PlatformNativePopoverContentItem::Label { bold, .. } => {
                        if *bold { 28.0 } else { 22.0 }
                    }
                    PlatformNativePopoverContentItem::SmallLabel { .. } => 18.0,
                    PlatformNativePopoverContentItem::IconLabel { .. } => 24.0,
                    PlatformNativePopoverContentItem::Button { .. } => 32.0,
                    PlatformNativePopoverContentItem::Toggle {
                        description, ..
                    } => {
                        if description.is_some() { 44.0 } else { 30.0 }
                    }
                    PlatformNativePopoverContentItem::Checkbox { .. } => 24.0,
                    PlatformNativePopoverContentItem::ProgressBar { label, .. } => {
                        if label.is_some() { 36.0 } else { 20.0 }
                    }
                    PlatformNativePopoverContentItem::ColorDot { detail, .. } => {
                        if detail.is_some() { 38.0 } else { 24.0 }
                    }
                    PlatformNativePopoverContentItem::ClickableRow { detail, .. } => {
                        if detail.is_some() { 36.0 } else { 28.0 }
                    }
                    PlatformNativePopoverContentItem::Separator => 12.0,
                };
                item_heights.push(height);
            }

            // Second pass: create views, consuming items for ownership of callbacks
            let mut top_y = padding;
            for (item, height) in popover_config.content_items.into_iter().zip(item_heights) {
                let flipped_y = popover_config.content_height - top_y - height;

                match item {
                    PlatformNativePopoverContentItem::Label { text, bold } => {
                        let font_size = if bold { 15.0 } else { 13.0 };
                        let label_height = if bold { 22.0 } else { 18.0 };
                        crate::platform::native_controls::add_native_popover_label(
                            content_view,
                            text.as_ref(),
                            padding,
                            flipped_y,
                            content_width,
                            label_height,
                            font_size,
                            bold,
                        );
                    }
                    PlatformNativePopoverContentItem::SmallLabel { text } => {
                        crate::platform::native_controls::add_native_popover_small_label(
                            content_view,
                            text.as_ref(),
                            padding,
                            flipped_y,
                            content_width,
                        );
                    }
                    PlatformNativePopoverContentItem::IconLabel { icon, text } => {
                        crate::platform::native_controls::add_native_popover_icon_label(
                            content_view,
                            icon.as_ref(),
                            text.as_ref(),
                            padding,
                            flipped_y,
                            content_width,
                        );
                    }
                    PlatformNativePopoverContentItem::Button { title, on_click } => {
                        let button = crate::platform::native_controls::add_native_popover_button(
                            content_view,
                            title.as_ref(),
                            padding,
                            flipped_y,
                            content_width,
                            28.0,
                        );
                        if let Some(callback) = on_click {
                            let target =
                                crate::platform::native_controls::set_native_button_action(
                                    button, callback,
                                );
                            button_targets.push(target);
                        }
                    }
                    PlatformNativePopoverContentItem::Toggle {
                        text,
                        checked,
                        on_change,
                        enabled,
                        description,
                    } => {
                        let target =
                            crate::platform::native_controls::add_native_popover_toggle(
                                content_view,
                                text.as_ref(),
                                checked,
                                padding,
                                flipped_y,
                                content_width,
                                enabled,
                                description.as_ref().map(|s| AsRef::<str>::as_ref(s)),
                                on_change,
                            );
                        if !target.is_null() {
                            switch_targets.push(target);
                        }
                    }
                    PlatformNativePopoverContentItem::Checkbox {
                        text,
                        checked,
                        on_change,
                        enabled,
                    } => {
                        let target =
                            crate::platform::native_controls::add_native_popover_checkbox(
                                content_view,
                                text.as_ref(),
                                checked,
                                padding,
                                flipped_y,
                                content_width,
                                enabled,
                                on_change,
                            );
                        if !target.is_null() {
                            checkbox_targets.push(target);
                        }
                    }
                    PlatformNativePopoverContentItem::ProgressBar { value, max, label } => {
                        crate::platform::native_controls::add_native_popover_progress(
                            content_view,
                            value,
                            max,
                            label.as_ref().map(|s| AsRef::<str>::as_ref(s)),
                            padding,
                            flipped_y,
                            content_width,
                        );
                    }
                    PlatformNativePopoverContentItem::ColorDot {
                        color,
                        text,
                        detail,
                        on_click,
                    } => {
                        let target =
                            crate::platform::native_controls::add_native_popover_color_dot(
                                content_view,
                                color,
                                text.as_ref(),
                                detail.as_ref().map(|s| AsRef::<str>::as_ref(s)),
                                padding,
                                flipped_y,
                                content_width,
                                on_click,
                            );
                        if !target.is_null() {
                            button_targets.push(target);
                        }
                    }
                    PlatformNativePopoverContentItem::ClickableRow {
                        icon,
                        text,
                        detail,
                        on_click,
                        enabled,
                        selected,
                    } => {
                        let target =
                            crate::platform::native_controls::add_native_popover_clickable_row(
                                content_view,
                                icon.as_ref().map(|s| AsRef::<str>::as_ref(s)),
                                text.as_ref(),
                                detail.as_ref().map(|s| AsRef::<str>::as_ref(s)),
                                padding,
                                flipped_y,
                                content_width,
                                enabled,
                                selected,
                                on_click,
                            );
                        if !target.is_null() {
                            hover_row_targets.push(target);
                        }
                    }
                    PlatformNativePopoverContentItem::Separator => {
                        crate::platform::native_controls::add_native_popover_separator(
                            content_view,
                            padding,
                            flipped_y + 5.0,
                            content_width,
                        );
                    }
                }

                top_y += height;
            }
        }

        // Show the popover
        match anchor {
            PlatformNativePopoverAnchor::ToolbarItem(item_id) => unsafe {
                let native_window = this.native_window;
                let toolbar: id = msg_send![native_window, toolbar];
                if toolbar != nil {
                    let items: id = msg_send![toolbar, items];
                    let count: usize = msg_send![items, count];
                    for i in 0..count {
                        let item: id = msg_send![items, objectAtIndex: i];
                        let item_identifier: id = msg_send![item, itemIdentifier];
                        let identifier_str = ns_string_to_owned(item_identifier);
                        if identifier_str == item_id.as_ref() {
                            crate::platform::native_controls::show_native_popover_relative_to_toolbar_item(
                                popover, item,
                            );
                            break;
                        }
                    }
                }
            },
        }

        this.popover = Some(MacPopoverState {
            popover,
            delegate_ptr,
            button_targets,
            switch_targets,
            checkbox_targets,
            hover_row_targets,
        });
    }

    fn dismiss_native_popover(&self) {
        let mut this = self.0.lock();
        cleanup_popover_state(&mut this.popover);
    }

    fn show_native_panel(
        &self,
        panel_config: PlatformNativePanel,
        anchor: PlatformNativePanelAnchor,
    ) {
        let mut this = self.0.lock();

        // Dismiss any existing panel first
        cleanup_panel_state(&mut this.panel);

        let style = match panel_config.style {
            PlatformNativePanelStyle::Titled => {
                crate::platform::native_controls::NativePanelStyle::Titled
            }
            PlatformNativePanelStyle::Borderless => {
                crate::platform::native_controls::NativePanelStyle::Borderless
            }
            PlatformNativePanelStyle::Hud => {
                crate::platform::native_controls::NativePanelStyle::Hud
            }
            PlatformNativePanelStyle::Utility => {
                crate::platform::native_controls::NativePanelStyle::Utility
            }
        };

        let level = match panel_config.level {
            PlatformNativePanelLevel::Normal => {
                crate::platform::native_controls::NativePanelLevel::Normal
            }
            PlatformNativePanelLevel::Floating => {
                crate::platform::native_controls::NativePanelLevel::Floating
            }
            PlatformNativePanelLevel::ModalPanel => {
                crate::platform::native_controls::NativePanelLevel::ModalPanel
            }
            PlatformNativePanelLevel::PopUpMenu => {
                crate::platform::native_controls::NativePanelLevel::PopUpMenu
            }
            PlatformNativePanelLevel::Custom(v) => {
                crate::platform::native_controls::NativePanelLevel::Custom(v)
            }
        };

        let material = panel_config.material.map(|m| match m {
            PlatformNativePanelMaterial::HudWindow => {
                crate::platform::native_controls::NativePanelMaterial::HudWindow
            }
            PlatformNativePanelMaterial::Popover => {
                crate::platform::native_controls::NativePanelMaterial::Popover
            }
            PlatformNativePanelMaterial::Sidebar => {
                crate::platform::native_controls::NativePanelMaterial::Sidebar
            }
            PlatformNativePanelMaterial::UnderWindow => {
                crate::platform::native_controls::NativePanelMaterial::UnderWindow
            }
        });

        let (panel, delegate_ptr) = unsafe {
            crate::platform::native_controls::create_native_panel(
                panel_config.width,
                panel_config.height,
                style,
                level,
                panel_config.non_activating,
                panel_config.has_shadow,
                panel_config.corner_radius,
                material,
                panel_config.on_close,
            )
        };

        let mut button_targets = Vec::new();
        let mut switch_targets = Vec::new();
        let mut checkbox_targets = Vec::new();
        let mut hover_row_targets = Vec::new();

        // Populate panel content inside an NSScrollView for scrollability
        unsafe {
            let content_view =
                crate::platform::native_controls::get_native_panel_content_view(panel);
            let padding = 16.0;
            let content_width = panel_config.width - padding * 2.0;

            // Calculate total content height
            let mut item_heights: Vec<f64> = Vec::new();
            for item in &panel_config.content_items {
                let height = match item {
                    PlatformNativePopoverContentItem::Label { bold, .. } => {
                        if *bold { 28.0 } else { 22.0 }
                    }
                    PlatformNativePopoverContentItem::SmallLabel { .. } => 18.0,
                    PlatformNativePopoverContentItem::IconLabel { .. } => 24.0,
                    PlatformNativePopoverContentItem::Button { .. } => 32.0,
                    PlatformNativePopoverContentItem::Toggle {
                        description, ..
                    } => {
                        if description.is_some() { 44.0 } else { 30.0 }
                    }
                    PlatformNativePopoverContentItem::Checkbox { .. } => 24.0,
                    PlatformNativePopoverContentItem::ProgressBar { label, .. } => {
                        if label.is_some() { 36.0 } else { 20.0 }
                    }
                    PlatformNativePopoverContentItem::ColorDot { detail, .. } => {
                        if detail.is_some() { 38.0 } else { 24.0 }
                    }
                    PlatformNativePopoverContentItem::ClickableRow { detail, .. } => {
                        if detail.is_some() { 36.0 } else { 28.0 }
                    }
                    PlatformNativePopoverContentItem::Separator => 12.0,
                };
                item_heights.push(height);
            }

            let total_content_height: f64 =
                item_heights.iter().sum::<f64>() + padding * 2.0;

            // Set up NSScrollView wrapping the content
            let content_bounds: NSRect = msg_send![content_view, bounds];
            let scroll_view: id = msg_send![class!(NSScrollView), alloc];
            let scroll_view: id = msg_send![scroll_view, initWithFrame: content_bounds];
            let _: () = msg_send![scroll_view, setHasVerticalScroller: YES];
            let _: () = msg_send![scroll_view, setHasHorizontalScroller: NO];
            let _: () = msg_send![scroll_view, setDrawsBackground: NO];
            // NSViewWidthSizable | NSViewHeightSizable = 18
            let _: () = msg_send![scroll_view, setAutoresizingMask: 18u64];

            // Create the document view (flipped so top-down coordinates work naturally)
            let doc_frame = NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(panel_config.width, total_content_height),
            );
            let doc_view: id = msg_send![class!(NSView), alloc];
            let doc_view: id = msg_send![doc_view, initWithFrame: doc_frame];
            let _: () = msg_send![scroll_view, setDocumentView: doc_view];
            let _: () = msg_send![doc_view, release];

            let _: () = msg_send![content_view, addSubview: scroll_view];
            let _: () = msg_send![scroll_view, release];

            // Place items into the document view using bottom-up coordinates
            let mut top_y = padding;
            for (item, height) in panel_config.content_items.into_iter().zip(item_heights) {
                let flipped_y = total_content_height - top_y - height;

                match item {
                    PlatformNativePopoverContentItem::Label { text, bold } => {
                        let font_size = if bold { 15.0 } else { 13.0 };
                        let label_height = if bold { 22.0 } else { 18.0 };
                        crate::platform::native_controls::add_native_popover_label(
                            doc_view,
                            text.as_ref(),
                            padding,
                            flipped_y,
                            content_width,
                            label_height,
                            font_size,
                            bold,
                        );
                    }
                    PlatformNativePopoverContentItem::SmallLabel { text } => {
                        crate::platform::native_controls::add_native_popover_small_label(
                            doc_view,
                            text.as_ref(),
                            padding,
                            flipped_y,
                            content_width,
                        );
                    }
                    PlatformNativePopoverContentItem::IconLabel { icon, text } => {
                        crate::platform::native_controls::add_native_popover_icon_label(
                            doc_view,
                            icon.as_ref(),
                            text.as_ref(),
                            padding,
                            flipped_y,
                            content_width,
                        );
                    }
                    PlatformNativePopoverContentItem::Button { title, on_click } => {
                        let button = crate::platform::native_controls::add_native_popover_button(
                            doc_view,
                            title.as_ref(),
                            padding,
                            flipped_y,
                            content_width,
                            28.0,
                        );
                        if let Some(callback) = on_click {
                            let target =
                                crate::platform::native_controls::set_native_button_action(
                                    button, callback,
                                );
                            button_targets.push(target);
                        }
                    }
                    PlatformNativePopoverContentItem::Toggle {
                        text,
                        checked,
                        on_change,
                        enabled,
                        description,
                    } => {
                        let target =
                            crate::platform::native_controls::add_native_popover_toggle(
                                doc_view,
                                text.as_ref(),
                                checked,
                                padding,
                                flipped_y,
                                content_width,
                                enabled,
                                description.as_ref().map(|s| AsRef::<str>::as_ref(s)),
                                on_change,
                            );
                        if !target.is_null() {
                            switch_targets.push(target);
                        }
                    }
                    PlatformNativePopoverContentItem::Checkbox {
                        text,
                        checked,
                        on_change,
                        enabled,
                    } => {
                        let target =
                            crate::platform::native_controls::add_native_popover_checkbox(
                                doc_view,
                                text.as_ref(),
                                checked,
                                padding,
                                flipped_y,
                                content_width,
                                enabled,
                                on_change,
                            );
                        if !target.is_null() {
                            checkbox_targets.push(target);
                        }
                    }
                    PlatformNativePopoverContentItem::ProgressBar { value, max, label } => {
                        crate::platform::native_controls::add_native_popover_progress(
                            doc_view,
                            value,
                            max,
                            label.as_ref().map(|s| AsRef::<str>::as_ref(s)),
                            padding,
                            flipped_y,
                            content_width,
                        );
                    }
                    PlatformNativePopoverContentItem::ColorDot {
                        color,
                        text,
                        detail,
                        on_click,
                    } => {
                        let target =
                            crate::platform::native_controls::add_native_popover_color_dot(
                                doc_view,
                                color,
                                text.as_ref(),
                                detail.as_ref().map(|s| AsRef::<str>::as_ref(s)),
                                padding,
                                flipped_y,
                                content_width,
                                on_click,
                            );
                        if !target.is_null() {
                            button_targets.push(target);
                        }
                    }
                    PlatformNativePopoverContentItem::ClickableRow {
                        icon,
                        text,
                        detail,
                        on_click,
                        enabled,
                        selected,
                    } => {
                        let target =
                            crate::platform::native_controls::add_native_popover_clickable_row(
                                doc_view,
                                icon.as_ref().map(|s| AsRef::<str>::as_ref(s)),
                                text.as_ref(),
                                detail.as_ref().map(|s| AsRef::<str>::as_ref(s)),
                                padding,
                                flipped_y,
                                content_width,
                                enabled,
                                selected,
                                on_click,
                            );
                        if !target.is_null() {
                            hover_row_targets.push(target);
                        }
                    }
                    PlatformNativePopoverContentItem::Separator => {
                        crate::platform::native_controls::add_native_popover_separator(
                            doc_view,
                            padding,
                            flipped_y + 5.0,
                            content_width,
                        );
                    }
                }

                top_y += height;
            }

            // Scroll to top of content. In Cocoa's bottom-up coordinate system,
            // the "top" of the content is at the highest y value.
            if total_content_height > content_bounds.size.height {
                let clip_view: id = msg_send![scroll_view, contentView];
                let scroll_point =
                    NSPoint::new(0.0, total_content_height - content_bounds.size.height);
                let _: () = msg_send![clip_view, scrollToPoint: scroll_point];
                let _: () = msg_send![scroll_view, reflectScrolledClipView: clip_view];
            }
        }

        // Position and show the panel based on anchor
        match anchor {
            PlatformNativePanelAnchor::ToolbarItem(item_id) => unsafe {
                let native_window = this.native_window;
                if let Some(screen_frame) =
                    crate::platform::native_controls::get_toolbar_item_screen_frame(
                        native_window,
                        item_id.as_ref(),
                    )
                {
                    // Center the panel horizontally under the toolbar item
                    let item_center_x =
                        screen_frame.origin.x + screen_frame.size.width / 2.0;
                    let x = item_center_x - panel_config.width / 2.0;
                    // Place the panel directly below the toolbar item
                    let y = screen_frame.origin.y - panel_config.height;
                    crate::platform::native_controls::set_native_panel_frame_origin(panel, x, y);
                    crate::platform::native_controls::show_native_panel(panel);
                } else {
                    crate::platform::native_controls::show_native_panel_centered(panel);
                }
            },
            PlatformNativePanelAnchor::Point { x, y } => unsafe {
                // x, y are in GPUI screen coordinates (top-left origin).
                // Convert to Cocoa screen coordinates (bottom-left origin) for
                // setFrameTopLeftPoint:.
                let screen = NSWindow::screen(this.native_window);
                let (cocoa_x, cocoa_y) = if screen != nil {
                    let screen_frame = NSScreen::frame(screen);
                    (
                        x + screen_frame.origin.x,
                        screen_frame.size.height - y + screen_frame.origin.y,
                    )
                } else {
                    (x, y)
                };
                crate::platform::native_controls::set_native_panel_frame_top_left(
                    panel, cocoa_x, cocoa_y,
                );
                crate::platform::native_controls::show_native_panel(panel);
            },
            PlatformNativePanelAnchor::Centered => unsafe {
                crate::platform::native_controls::show_native_panel_centered(panel);
            },
        }

        this.panel = Some(MacPanelState {
            panel,
            delegate_ptr,
            button_targets,
            switch_targets,
            checkbox_targets,
            hover_row_targets,
        });
    }

    fn dismiss_native_panel(&self) {
        let mut this = self.0.lock();
        cleanup_panel_state(&mut this.panel);
    }

    fn show_native_alert_sheet(
        &self,
        alert_config: PlatformNativeAlert,
    ) -> Option<oneshot::Receiver<usize>> {
        let style = match alert_config.style {
            PlatformNativeAlertStyle::Warning => {
                crate::platform::native_controls::NativeAlertStyleRaw::Warning
            }
            PlatformNativeAlertStyle::Informational => {
                crate::platform::native_controls::NativeAlertStyleRaw::Informational
            }
            PlatformNativeAlertStyle::Critical => {
                crate::platform::native_controls::NativeAlertStyleRaw::Critical
            }
        };

        unsafe {
            let alert: id = msg_send![class!(NSAlert), alloc];
            let alert: id = msg_send![alert, init];
            let alert_style: u64 = match style {
                crate::platform::native_controls::NativeAlertStyleRaw::Warning => 0,
                crate::platform::native_controls::NativeAlertStyleRaw::Informational => 1,
                crate::platform::native_controls::NativeAlertStyleRaw::Critical => 2,
            };
            let _: () = msg_send![alert, setAlertStyle: alert_style];
            let _: () = msg_send![alert, setMessageText: ns_string(alert_config.message.as_ref())];

            if let Some(info) = &alert_config.informative_text {
                let _: () = msg_send![alert, setInformativeText: ns_string(info.as_ref())];
            }

            for title in &alert_config.button_titles {
                let _: () = msg_send![alert, addButtonWithTitle: ns_string(title.as_ref())];
            }

            if alert_config.shows_suppression_button {
                let _: () = msg_send![alert, setShowsSuppressionButton: YES];
            }

            let (done_tx, done_rx) = oneshot::channel();
            let done_tx = Cell::new(Some(done_tx));
            let block = ConcreteBlock::new(move |answer: NSInteger| {
                let _: () = msg_send![alert, release];
                if let Some(done_tx) = done_tx.take() {
                    // Convert NSModalResponse (1000, 1001, ...) to button index (0, 1, ...)
                    let button_index = (answer
                        - crate::platform::native_controls::NS_ALERT_FIRST_BUTTON_RETURN as NSInteger)
                        as usize;
                    let _ = done_tx.send(button_index);
                }
            });
            let block = block.copy();
            let native_window = self.0.lock().native_window;
            let executor = self.0.lock().foreground_executor.clone();
            executor
                .spawn(async move {
                    let _: () = msg_send![
                        alert,
                        beginSheetModalForWindow: native_window
                        completionHandler: block
                    ];
                })
                .detach();

            Some(done_rx)
        }
    }

    fn present_as_sheet(&self, child_window: &dyn PlatformWindow) {
        let this = self.0.lock();
        let parent_window = this.native_window;
        if let Ok(rwh::RawWindowHandle::AppKit(handle)) =
            child_window.window_handle().map(|h| h.as_raw())
        {
            let ns_view = handle.ns_view.as_ptr() as id;
            unsafe {
                let child_ns_window: id = msg_send![ns_view, window];
                if child_ns_window != nil {
                    let _: () = msg_send![parent_window, beginSheet: child_ns_window completionHandler: nil];
                }
            }
        }
    }

    fn dismiss_sheet(&self) {
        let this = self.0.lock();
        if let Some(sheet_parent) = this.sheet_parent {
            let native_window = this.native_window;
            unsafe {
                let _: () = msg_send![sheet_parent, endSheet: native_window];
            }
        }
    }

    fn scale_factor(&self) -> f32 {
        self.0.as_ref().lock().scale_factor()
    }

    fn appearance(&self) -> WindowAppearance {
        unsafe {
            let appearance: id = msg_send![self.0.lock().native_window, effectiveAppearance];
            WindowAppearance::from_native(appearance)
        }
    }

    fn display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        unsafe {
            let screen = self.0.lock().native_window.screen();
            if screen.is_null() {
                return None;
            }
            let device_description: id = msg_send![screen, deviceDescription];
            let screen_number: id =
                NSDictionary::valueForKey_(device_description, ns_string("NSScreenNumber"));

            let screen_number: u32 = msg_send![screen_number, unsignedIntValue];

            Some(Rc::new(MacDisplay(screen_number)))
        }
    }

    fn mouse_position(&self) -> Point<Pixels> {
        let lock = self.0.lock();
        let window_point =
            unsafe { lock.native_window.mouseLocationOutsideOfEventStream() };
        let native_view = lock.native_view.as_ptr() as id;
        unsafe {
            let local_point: NSPoint =
                msg_send![native_view, convertPoint:window_point fromView:nil];
            let bounds: NSRect = msg_send![native_view, bounds];
            point(
                px(local_point.x as f32),
                px((bounds.size.height - local_point.y) as f32),
            )
        }
    }

    fn modifiers(&self) -> Modifiers {
        unsafe {
            let modifiers: NSEventModifierFlags = msg_send![class!(NSEvent), modifierFlags];

            let control = modifiers.contains(NSEventModifierFlags::NSControlKeyMask);
            let alt = modifiers.contains(NSEventModifierFlags::NSAlternateKeyMask);
            let shift = modifiers.contains(NSEventModifierFlags::NSShiftKeyMask);
            let command = modifiers.contains(NSEventModifierFlags::NSCommandKeyMask);
            let function = modifiers.contains(NSEventModifierFlags::NSFunctionKeyMask);

            Modifiers {
                control,
                alt,
                shift,
                platform: command,
                function,
            }
        }
    }

    fn capslock(&self) -> Capslock {
        unsafe {
            let modifiers: NSEventModifierFlags = msg_send![class!(NSEvent), modifierFlags];

            Capslock {
                on: modifiers.contains(NSEventModifierFlags::NSAlphaShiftKeyMask),
            }
        }
    }

    fn set_input_handler(&mut self, input_handler: PlatformInputHandler) {
        self.0.as_ref().lock().input_handler = Some(input_handler);
    }

    fn take_input_handler(&mut self) -> Option<PlatformInputHandler> {
        self.0.as_ref().lock().input_handler.take()
    }

    fn prompt(
        &self,
        level: PromptLevel,
        msg: &str,
        detail: Option<&str>,
        answers: &[PromptButton],
    ) -> Option<oneshot::Receiver<usize>> {
        // macOs applies overrides to modal window buttons after they are added.
        // Two most important for this logic are:
        // * Buttons with "Cancel" title will be displayed as the last buttons in the modal
        // * Last button added to the modal via `addButtonWithTitle` stays focused
        // * Focused buttons react on "space"/" " keypresses
        // * Usage of `keyEquivalent`, `makeFirstResponder` or `setInitialFirstResponder` does not change the focus
        //
        // See also https://developer.apple.com/documentation/appkit/nsalert/1524532-addbuttonwithtitle#discussion
        // ```
        // By default, the first button has a key equivalent of Return,
        // any button with a title of “Cancel” has a key equivalent of Escape,
        // and any button with the title “Don’t Save” has a key equivalent of Command-D (but only if it’s not the first button).
        // ```
        //
        // To avoid situations when the last element added is "Cancel" and it gets the focus
        // (hence stealing both ESC and Space shortcuts), we find and add one non-Cancel button
        // last, so it gets focus and a Space shortcut.
        // This way, "Save this file? Yes/No/Cancel"-ish modals will get all three buttons mapped with a key.
        let latest_non_cancel_label = answers
            .iter()
            .enumerate()
            .rev()
            .find(|(_, label)| !label.is_cancel())
            .filter(|&(label_index, _)| label_index > 0);

        unsafe {
            let alert: id = msg_send![class!(NSAlert), alloc];
            let alert: id = msg_send![alert, init];
            let alert_style = match level {
                PromptLevel::Info => 1,
                PromptLevel::Warning => 0,
                PromptLevel::Critical => 2,
            };
            let _: () = msg_send![alert, setAlertStyle: alert_style];
            let _: () = msg_send![alert, setMessageText: ns_string(msg)];
            if let Some(detail) = detail {
                let _: () = msg_send![alert, setInformativeText: ns_string(detail)];
            }

            for (ix, answer) in answers
                .iter()
                .enumerate()
                .filter(|&(ix, _)| Some(ix) != latest_non_cancel_label.map(|(ix, _)| ix))
            {
                let button: id = msg_send![alert, addButtonWithTitle: ns_string(answer.label())];
                let _: () = msg_send![button, setTag: ix as NSInteger];

                if answer.is_cancel() {
                    // Bind Escape Key to Cancel Button
                    if let Some(key) = std::char::from_u32(super::events::ESCAPE_KEY as u32) {
                        let _: () =
                            msg_send![button, setKeyEquivalent: ns_string(&key.to_string())];
                    }
                }
            }
            if let Some((ix, answer)) = latest_non_cancel_label {
                let button: id = msg_send![alert, addButtonWithTitle: ns_string(answer.label())];
                let _: () = msg_send![button, setTag: ix as NSInteger];
            }

            let (done_tx, done_rx) = oneshot::channel();
            let done_tx = Cell::new(Some(done_tx));
            let block = ConcreteBlock::new(move |answer: NSInteger| {
                let _: () = msg_send![alert, release];
                if let Some(done_tx) = done_tx.take() {
                    let _ = done_tx.send(answer.try_into().unwrap());
                }
            });
            let block = block.copy();
            let native_window = self.0.lock().native_window;
            let executor = self.0.lock().foreground_executor.clone();
            executor
                .spawn(async move {
                    let _: () = msg_send![
                        alert,
                        beginSheetModalForWindow: native_window
                        completionHandler: block
                    ];
                })
                .detach();

            Some(done_rx)
        }
    }

    fn activate(&self) {
        let window = self.0.lock().native_window;
        let executor = self.0.lock().foreground_executor.clone();
        executor
            .spawn(async move {
                unsafe {
                    let _: () = msg_send![window, makeKeyAndOrderFront: nil];
                }
            })
            .detach();
    }

    fn is_active(&self) -> bool {
        unsafe { self.0.lock().native_window.isKeyWindow() == YES }
    }

    // is_hovered is unused on macOS. See Window::is_window_hovered.
    fn is_hovered(&self) -> bool {
        false
    }

    fn set_title(&mut self, title: &str) {
        unsafe {
            let app = NSApplication::sharedApplication(nil);
            let window = self.0.lock().native_window;
            let title = ns_string(title);
            let _: () = msg_send![app, changeWindowsItem:window title:title filename:false];
            let _: () = msg_send![window, setTitle: title];
            self.0.lock().move_traffic_light();
        }
    }

    fn get_title(&self) -> String {
        unsafe {
            let title: id = msg_send![self.0.lock().native_window, title];
            if title.is_null() {
                "".to_string()
            } else {
                title.to_str().to_string()
            }
        }
    }

    fn set_app_id(&mut self, _app_id: &str) {}

    fn set_background_appearance(&self, background_appearance: WindowBackgroundAppearance) {
        let mut this = self.0.as_ref().lock();
        this.background_appearance = background_appearance;

        let opaque = background_appearance == WindowBackgroundAppearance::Opaque;
        this.renderer.update_transparency(!opaque);

        unsafe {
            this.native_window.setOpaque_(opaque as BOOL);
            let background_color = if opaque {
                NSColor::colorWithSRGBRed_green_blue_alpha_(nil, 0f64, 0f64, 0f64, 1f64)
            } else {
                // Not using `+[NSColor clearColor]` to avoid broken shadow.
                NSColor::colorWithSRGBRed_green_blue_alpha_(nil, 0f64, 0f64, 0f64, 0.0001)
            };
            this.native_window.setBackgroundColor_(background_color);

            if NSAppKitVersionNumber < NSAppKitVersionNumber12_0 {
                // Whether `-[NSVisualEffectView respondsToSelector:@selector(_updateProxyLayer)]`.
                // On macOS Catalina/Big Sur `NSVisualEffectView` doesn’t own concrete sublayers
                // but uses a `CAProxyLayer`. Use the legacy WindowServer API.
                let blur_radius = if background_appearance == WindowBackgroundAppearance::Blurred {
                    80
                } else {
                    0
                };

                let window_number = this.native_window.windowNumber();
                CGSSetWindowBackgroundBlurRadius(CGSMainConnectionID(), window_number, blur_radius);
            } else {
                // On newer macOS `NSVisualEffectView` manages the effect layer directly. Using it
                // could have a better performance (it downsamples the backdrop) and more control
                // over the effect layer.
                if background_appearance != WindowBackgroundAppearance::Blurred {
                    if let Some(blur_view) = this.blurred_view {
                        NSView::removeFromSuperview(blur_view);
                        this.blurred_view = None;
                    }
                } else if this.blurred_view.is_none() {
                    let content_view = this.native_window.contentView();
                    let frame = NSView::bounds(content_view);
                    let mut blur_view: id = msg_send![BLURRED_VIEW_CLASS, alloc];
                    blur_view = NSView::initWithFrame_(blur_view, frame);
                    blur_view.setAutoresizingMask_(NSViewWidthSizable | NSViewHeightSizable);

                    let _: () = msg_send![
                        content_view,
                        addSubview: blur_view
                        positioned: NSWindowOrderingMode::NSWindowBelow
                        relativeTo: nil
                    ];
                    this.blurred_view = Some(blur_view.autorelease());
                }
            }
        }
    }

    fn background_appearance(&self) -> WindowBackgroundAppearance {
        self.0.as_ref().lock().background_appearance
    }

    fn is_subpixel_rendering_supported(&self) -> bool {
        false
    }

    fn set_edited(&mut self, edited: bool) {
        unsafe {
            let window = self.0.lock().native_window;
            msg_send![window, setDocumentEdited: edited as BOOL]
        }

        // Changing the document edited state resets the traffic light position,
        // so we have to move it again.
        self.0.lock().move_traffic_light();
    }

    fn show_character_palette(&self) {
        let this = self.0.lock();
        let window = this.native_window;
        this.foreground_executor
            .spawn(async move {
                unsafe {
                    let app = NSApplication::sharedApplication(nil);
                    let _: () = msg_send![app, orderFrontCharacterPalette: window];
                }
            })
            .detach();
    }

    fn minimize(&self) {
        let window = self.0.lock().native_window;
        unsafe {
            window.miniaturize_(nil);
        }
    }

    fn zoom(&self) {
        let this = self.0.lock();
        let window = this.native_window;
        this.foreground_executor
            .spawn(async move {
                unsafe {
                    window.zoom_(nil);
                }
            })
            .detach();
    }

    fn toggle_fullscreen(&self) {
        let this = self.0.lock();
        let window = this.native_window;
        this.foreground_executor
            .spawn(async move {
                unsafe {
                    window.toggleFullScreen_(nil);
                }
            })
            .detach();
    }

    fn is_fullscreen(&self) -> bool {
        let this = self.0.lock();
        let window = this.native_window;

        unsafe {
            window
                .styleMask()
                .contains(NSWindowStyleMask::NSFullScreenWindowMask)
        }
    }

    fn on_request_frame(&self, callback: Box<dyn FnMut(RequestFrameOptions)>) {
        self.0.as_ref().lock().request_frame_callback = Some(callback);
    }

    fn on_input(&self, callback: Box<dyn FnMut(PlatformInput) -> crate::DispatchEventResult>) {
        self.0.as_ref().lock().event_callback = Some(callback);
    }

    fn on_surface_input(
        &self,
        callback: Box<dyn FnMut(*mut c_void, PlatformInput) -> crate::DispatchEventResult>,
    ) {
        self.0.as_ref().lock().surface_event_callback = Some(callback);
    }

    fn on_active_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        self.0.as_ref().lock().activate_callback = Some(callback);
    }

    fn on_hover_status_change(&self, _: Box<dyn FnMut(bool)>) {}

    fn on_resize(&self, callback: Box<dyn FnMut(Size<Pixels>, f32)>) {
        self.0.as_ref().lock().resize_callback = Some(callback);
    }

    fn on_moved(&self, callback: Box<dyn FnMut()>) {
        self.0.as_ref().lock().moved_callback = Some(callback);
    }

    fn on_should_close(&self, callback: Box<dyn FnMut() -> bool>) {
        self.0.as_ref().lock().should_close_callback = Some(callback);
    }

    fn on_close(&self, callback: Box<dyn FnOnce()>) {
        self.0.as_ref().lock().close_callback = Some(callback);
    }

    fn on_hit_test_window_control(&self, _callback: Box<dyn FnMut() -> Option<WindowControlArea>>) {
    }

    fn on_appearance_changed(&self, callback: Box<dyn FnMut()>) {
        self.0.lock().appearance_changed_callback = Some(callback);
    }

    fn tabbed_windows(&self) -> Option<Vec<SystemWindowTab>> {
        unsafe {
            let windows: id = msg_send![self.0.lock().native_window, tabbedWindows];
            if windows.is_null() {
                return None;
            }

            let count: NSUInteger = msg_send![windows, count];
            let mut result = Vec::new();
            for i in 0..count {
                let window: id = msg_send![windows, objectAtIndex:i];
                if msg_send![window, isKindOfClass: WINDOW_CLASS] {
                    let handle = get_window_state(&*window).lock().handle;
                    let title: id = msg_send![window, title];
                    let title = SharedString::from(title.to_str().to_string());

                    result.push(SystemWindowTab::new(title, handle));
                }
            }

            Some(result)
        }
    }

    fn tab_bar_visible(&self) -> bool {
        unsafe {
            let tab_group: id = msg_send![self.0.lock().native_window, tabGroup];
            if tab_group.is_null() {
                false
            } else {
                let tab_bar_visible: BOOL = msg_send![tab_group, isTabBarVisible];
                tab_bar_visible == YES
            }
        }
    }

    fn on_move_tab_to_new_window(&self, callback: Box<dyn FnMut()>) {
        self.0.as_ref().lock().move_tab_to_new_window_callback = Some(callback);
    }

    fn on_merge_all_windows(&self, callback: Box<dyn FnMut()>) {
        self.0.as_ref().lock().merge_all_windows_callback = Some(callback);
    }

    fn on_select_next_tab(&self, callback: Box<dyn FnMut()>) {
        self.0.as_ref().lock().select_next_tab_callback = Some(callback);
    }

    fn on_select_previous_tab(&self, callback: Box<dyn FnMut()>) {
        self.0.as_ref().lock().select_previous_tab_callback = Some(callback);
    }

    fn on_toggle_tab_bar(&self, callback: Box<dyn FnMut()>) {
        self.0.as_ref().lock().toggle_tab_bar_callback = Some(callback);
    }

    fn draw(&self, scene: &crate::Scene) {
        let mut this = self.0.lock();
        this.renderer.draw(scene);
    }

    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        self.0.lock().renderer.sprite_atlas().clone()
    }

    fn gpu_specs(&self) -> Option<crate::GpuSpecs> {
        None
    }

    fn update_ime_position(&self, _bounds: Bounds<Pixels>) {
        let executor = self.0.lock().foreground_executor.clone();
        executor
            .spawn(async move {
                unsafe {
                    let input_context: id =
                        msg_send![class!(NSTextInputContext), currentInputContext];
                    if input_context.is_null() {
                        return;
                    }
                    let _: () = msg_send![input_context, invalidateCharacterCoordinates];
                }
            })
            .detach()
    }

    fn raw_native_view_ptr(&self) -> *mut c_void {
        self.0.lock().native_view.as_ptr() as *mut c_void
    }

    fn shared_render_resources(
        &self,
    ) -> std::sync::Arc<super::metal_renderer::SharedRenderResources> {
        self.0.lock().renderer.shared().clone()
    }

    fn window_state_ptr(&self) -> *const c_void {
        Arc::into_raw(self.0.clone()) as *const c_void
    }

    fn titlebar_double_click(&self) {
        let this = self.0.lock();
        let window = this.native_window;
        this.foreground_executor
            .spawn(async move {
                unsafe {
                    let defaults: id = NSUserDefaults::standardUserDefaults();
                    let domain = ns_string("NSGlobalDomain");
                    let key = ns_string("AppleActionOnDoubleClick");

                    let dict: id = msg_send![defaults, persistentDomainForName: domain];
                    let action: id = if !dict.is_null() {
                        msg_send![dict, objectForKey: key]
                    } else {
                        nil
                    };

                    let action_str = if !action.is_null() {
                        CStr::from_ptr(NSString::UTF8String(action)).to_string_lossy()
                    } else {
                        "".into()
                    };

                    match action_str.as_ref() {
                        "None" => {
                            // "Do Nothing" selected, so do no action
                        }
                        "Minimize" => {
                            window.miniaturize_(nil);
                        }
                        "Maximize" => {
                            window.zoom_(nil);
                        }
                        "Fill" => {
                            // There is no documented API for "Fill" action, so we'll just zoom the window
                            window.zoom_(nil);
                        }
                        _ => {
                            window.zoom_(nil);
                        }
                    }
                }
            })
            .detach();
    }

    fn start_window_move(&self) {
        let this = self.0.lock();
        let window = this.native_window;

        unsafe {
            let app = NSApplication::sharedApplication(nil);
            let mut event: id = msg_send![app, currentEvent];
            let _: () = msg_send![window, performWindowDragWithEvent: event];
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    fn render_to_image(&self, scene: &crate::Scene) -> Result<RgbaImage> {
        let mut this = self.0.lock();
        this.renderer.render_to_image(scene)
    }
}

impl rwh::HasWindowHandle for MacWindow {
    fn window_handle(&self) -> Result<rwh::WindowHandle<'_>, rwh::HandleError> {
        // SAFETY: The AppKitWindowHandle is a wrapper around a pointer to an NSView
        unsafe {
            Ok(rwh::WindowHandle::borrow_raw(rwh::RawWindowHandle::AppKit(
                rwh::AppKitWindowHandle::new(self.0.lock().native_view.cast()),
            )))
        }
    }
}

impl rwh::HasDisplayHandle for MacWindow {
    fn display_handle(&self) -> Result<rwh::DisplayHandle<'_>, rwh::HandleError> {
        // SAFETY: This is a no-op on macOS
        unsafe {
            Ok(rwh::DisplayHandle::borrow_raw(
                rwh::AppKitDisplayHandle::new().into(),
            ))
        }
    }
}

fn get_scale_factor(native_window: id) -> f32 {
    let factor = unsafe {
        let screen: id = msg_send![native_window, screen];
        if screen.is_null() {
            return 2.0;
        }
        NSScreen::backingScaleFactor(screen) as f32
    };

    // We are not certain what triggers this, but it seems that sometimes
    // this method would return 0 (https://github.com/zed-industries/zed/issues/6412)
    // It seems most likely that this would happen if the window has no screen
    // (if it is off-screen), though we'd expect to see viewDidChangeBackingProperties before
    // it was rendered for real.
    // Regardless, attempt to avoid the issue here.
    if factor == 0.0 { 2. } else { factor }
}

unsafe fn get_window_state(object: &Object) -> Arc<Mutex<MacWindowState>> {
    unsafe {
        let raw: *mut c_void = *object.get_ivar(WINDOW_STATE_IVAR);
        let rc1 = Arc::from_raw(raw as *mut Mutex<MacWindowState>);
        let rc2 = rc1.clone();
        mem::forget(rc1);
        rc2
    }
}

unsafe fn drop_window_state(object: &Object) {
    unsafe {
        let raw: *mut c_void = *object.get_ivar(WINDOW_STATE_IVAR);
        Arc::from_raw(raw as *mut Mutex<MacWindowState>);
    }
}

extern "C" fn yes(_: &Object, _: Sel) -> BOOL {
    YES
}

extern "C" fn dealloc_window(this: &Object, _: Sel) {
    unsafe {
        drop_window_state(this);
        let _: () = msg_send![super(this, class!(NSWindow)), dealloc];
    }
}

extern "C" fn dealloc_view(this: &Object, _: Sel) {
    unsafe {
        drop_window_state(this);
        let _: () = msg_send![super(this, class!(NSView)), dealloc];
    }
}

/// Returns true when the window's first responder is a field editor for a native
/// control (e.g. NSSearchField in the toolbar). In that case, GPUI should not
/// intercept key events so AppKit can dispatch them through the field editor's
/// normal command handling chain (doCommandBySelector:).
fn is_native_field_editor_active(view: &Object) -> bool {
    unsafe {
        let window: id = msg_send![view, window];
        if window.is_null() {
            return false;
        }
        let first_responder: id = msg_send![window, firstResponder];
        if first_responder.is_null() {
            return false;
        }
        let is_text_view: BOOL = msg_send![first_responder, isKindOfClass: class!(NSTextView)];
        if is_text_view == NO {
            return false;
        }
        let is_field_editor: BOOL = msg_send![first_responder, isFieldEditor];
        is_field_editor != NO
    }
}

extern "C" fn handle_key_equivalent(this: &Object, _: Sel, native_event: id) -> BOOL {
    if is_native_field_editor_active(this) {
        return NO;
    }
    handle_key_event(this, native_event, true)
}

extern "C" fn handle_key_down(this: &Object, _: Sel, native_event: id) {
    if is_native_field_editor_active(this) {
        return;
    }
    handle_key_event(this, native_event, false);
}

extern "C" fn handle_key_up(this: &Object, _: Sel, native_event: id) {
    handle_key_event(this, native_event, false);
}

// Things to test if you're modifying this method:
//  U.S. layout:
//   - The IME consumes characters like 'j' and 'k', which makes paging through `less` in
//     the terminal behave incorrectly by default. This behavior should be patched by our
//     IME integration
//   - `alt-t` should open the tasks menu
//   - Keybinding chords involving letter keys (e.g. `j j`) should still work without the IME
//     consuming the individual characters unexpectedly.
//  Brazilian layout:
//   - `" space` should create an unmarked quote
//   - `" backspace` should delete the marked quote
//   - `" "`should create an unmarked quote and a second marked quote
//   - `" up` should insert a quote, unmark it, and move up one line
//   - `" cmd-down` should insert a quote, unmark it, and move to the end of the file
//   - `cmd-ctrl-space` and clicking on an emoji should type it
//  Czech (QWERTY) layout:
//   - `option-4` should go to end of line (same as $) when that binding is configured
//  Japanese (Romaji) layout:
//   - type `a i left down up enter enter` should create an unmarked text "愛"
extern "C" fn handle_key_event(this: &Object, native_event: id, key_equivalent: bool) -> BOOL {
    let window_state = unsafe { get_window_state(this) };
    let mut lock = window_state.as_ref().lock();

    let event = unsafe {
        PlatformInput::from_native(native_event, None, Some(this as *const _ as id))
    };

    let Some(event) = event else {
        return NO;
    };

    let run_callback = |event: PlatformInput| -> BOOL {
        let mut callback = window_state.as_ref().lock().event_callback.take();
        let handled: BOOL = if let Some(callback) = callback.as_mut() {
            !callback(event).propagate as BOOL
        } else {
            NO
        };
        window_state.as_ref().lock().event_callback = callback;
        handled
    };

    match event {
        PlatformInput::KeyDown(mut key_down_event) => {
            // For certain keystrokes, macOS will first dispatch a "key equivalent" event.
            // If that event isn't handled, it will then dispatch a "key down" event. GPUI
            // makes no distinction between these two types of events, so we need to ignore
            // the "key down" event if we've already just processed its "key equivalent" version.
            if key_equivalent {
                lock.last_key_equivalent = Some(key_down_event.clone());
            } else if lock.last_key_equivalent.take().as_ref() == Some(&key_down_event) {
                return NO;
            }

            drop(lock);

            let is_composing =
                with_input_handler(this, |input_handler| input_handler.marked_text_range())
                    .flatten()
                    .is_some();

            // If we're composing, send the key to the input handler first;
            // otherwise we only send to the input handler if we don't have a matching binding.
            // The input handler may call `do_command_by_selector` if it doesn't know how to handle
            // a key. If it does so, it will return YES so we won't send the key twice.
            // We also do this for non-printing keys (like arrow keys and escape) as the IME menu
            // may need them even if there is no marked text;
            // however we skip keys with control or the input handler adds control-characters to the buffer.
            // and keys with function, as the input handler swallows them.
            if is_composing
                || (key_down_event.keystroke.key_char.is_none()
                    && !key_down_event.keystroke.modifiers.control
                    && !key_down_event.keystroke.modifiers.function)
            {
                {
                    let mut lock = window_state.as_ref().lock();
                    lock.keystroke_for_do_command = Some(key_down_event.keystroke.clone());
                    lock.do_command_handled.take();
                    drop(lock);
                }

                let handled: BOOL = unsafe {
                    let input_context: id = msg_send![this, inputContext];
                    msg_send![input_context, handleEvent: native_event]
                };
                window_state.as_ref().lock().keystroke_for_do_command.take();
                if let Some(handled) = window_state.as_ref().lock().do_command_handled.take() {
                    return handled as BOOL;
                } else if handled == YES {
                    return YES;
                }

                let handled = run_callback(PlatformInput::KeyDown(key_down_event));
                return handled;
            }

            let handled = run_callback(PlatformInput::KeyDown(key_down_event.clone()));
            if handled == YES {
                return YES;
            }

            if key_down_event.is_held
                && let Some(key_char) = key_down_event.keystroke.key_char.as_ref()
            {
                let handled = with_input_handler(this, |input_handler| {
                    if !input_handler.apple_press_and_hold_enabled() {
                        input_handler.replace_text_in_range(None, key_char);
                        return YES;
                    }
                    NO
                });
                if handled == Some(YES) {
                    return YES;
                }
            }

            // Don't send key equivalents to the input handler if there are key modifiers other
            // than Function key, or macOS shortcuts like cmd-` will stop working.
            if key_equivalent && key_down_event.keystroke.modifiers != Modifiers::function() {
                return NO;
            }

            unsafe {
                let input_context: id = msg_send![this, inputContext];
                msg_send![input_context, handleEvent: native_event]
            }
        }

        PlatformInput::KeyUp(_) => {
            drop(lock);
            run_callback(event)
        }

        _ => NO,
    }
}

extern "C" fn handle_view_event(this: &Object, _: Sel, native_event: id) {
    unsafe {
        let event_type: u64 = msg_send![native_event, type];
        // NSEventTypeLeftMouseDown = 1, NSEventTypeRightMouseDown = 3
        if event_type == 1 || event_type == 3 {
            if is_native_field_editor_active(this) {
                let window: id = msg_send![this, window];
                let nobody: id = std::ptr::null_mut();
                let _: BOOL = msg_send![window, makeFirstResponder: nobody];
            }
        }
    }

    let window_state = unsafe { get_window_state(this) };
    let weak_window_state = Arc::downgrade(&window_state);
    let mut lock = window_state.as_ref().lock();
    let event = unsafe {
        PlatformInput::from_native(native_event, None, Some(this as *const _ as id))
    };

    if let Some(mut event) = event {
        match &mut event {
            PlatformInput::MouseDown(
                event @ MouseDownEvent {
                    button: MouseButton::Left,
                    modifiers: Modifiers { control: true, .. },
                    ..
                },
            ) => {
                // On mac, a ctrl-left click should be handled as a right click.
                *event = MouseDownEvent {
                    button: MouseButton::Right,
                    modifiers: Modifiers {
                        control: false,
                        ..event.modifiers
                    },
                    click_count: 1,
                    ..*event
                };
            }

            // Handles focusing click.
            PlatformInput::MouseDown(
                event @ MouseDownEvent {
                    button: MouseButton::Left,
                    ..
                },
            ) if (lock.first_mouse) => {
                *event = MouseDownEvent {
                    first_mouse: true,
                    ..*event
                };
                lock.first_mouse = false;
            }

            // Because we map a ctrl-left_down to a right_down -> right_up let's ignore
            // the ctrl-left_up to avoid having a mismatch in button down/up events if the
            // user is still holding ctrl when releasing the left mouse button
            PlatformInput::MouseUp(
                event @ MouseUpEvent {
                    button: MouseButton::Left,
                    modifiers: Modifiers { control: true, .. },
                    ..
                },
            ) => {
                *event = MouseUpEvent {
                    button: MouseButton::Right,
                    modifiers: Modifiers {
                        control: false,
                        ..event.modifiers
                    },
                    click_count: 1,
                    ..*event
                };
            }

            _ => {}
        };

        match &event {
            PlatformInput::MouseDown(_) => {
                drop(lock);
                unsafe {
                    let input_context: id = msg_send![this, inputContext];
                    msg_send![input_context, handleEvent: native_event]
                }
                lock = window_state.as_ref().lock();
            }
            PlatformInput::MouseMove(
                event @ MouseMoveEvent {
                    pressed_button: Some(_),
                    ..
                },
            ) => {
                // Synthetic drag is used for selecting long buffer contents while buffer is being scrolled.
                // External file drag and drop is able to emit its own synthetic mouse events which will conflict
                // with these ones.
                if !lock.external_files_dragged {
                    lock.synthetic_drag_counter += 1;
                    let executor = lock.foreground_executor.clone();
                    executor
                        .spawn(synthetic_drag(
                            weak_window_state,
                            lock.synthetic_drag_counter,
                            event.clone(),
                            lock.background_executor.clone(),
                        ))
                        .detach();
                }
            }

            PlatformInput::MouseUp(MouseUpEvent { .. }) => {
                lock.synthetic_drag_counter += 1;
            }

            PlatformInput::ModifiersChanged(ModifiersChangedEvent {
                modifiers,
                capslock,
            }) => {
                // Only raise modifiers changed event when they have actually changed
                if let Some(PlatformInput::ModifiersChanged(ModifiersChangedEvent {
                    modifiers: prev_modifiers,
                    capslock: prev_capslock,
                })) = &lock.previous_modifiers_changed_event
                    && prev_modifiers == modifiers
                    && prev_capslock == capslock
                {
                    return;
                }

                lock.previous_modifiers_changed_event = Some(event.clone());
            }

            _ => {}
        }

        if let Some(mut callback) = lock.event_callback.take() {
            drop(lock);
            callback(event);
            window_state.lock().event_callback = Some(callback);
        }
    }
}

extern "C" fn window_did_change_occlusion_state(this: &Object, _: Sel, _: id) {
    let window_state = unsafe { get_window_state(this) };
    let lock = &mut *window_state.lock();
    unsafe {
        if lock
            .native_window
            .occlusionState()
            .contains(NSWindowOcclusionState::NSWindowOcclusionStateVisible)
        {
            lock.move_traffic_light();
            lock.start_display_link();
        } else {
            lock.stop_display_link();
        }
    }
}

extern "C" fn window_did_resize(this: &Object, _: Sel, _: id) {
    let window_state = unsafe { get_window_state(this) };
    window_state.as_ref().lock().move_traffic_light();
}

extern "C" fn window_will_enter_fullscreen(this: &Object, _: Sel, _: id) {
    let window_state = unsafe { get_window_state(this) };
    let mut lock = window_state.as_ref().lock();
    lock.fullscreen_restore_bounds = lock.bounds();

    let min_version = NSOperatingSystemVersion::new(15, 3, 0);

    if is_macos_version_at_least(min_version) {
        unsafe {
            lock.native_window.setTitlebarAppearsTransparent_(NO);
        }
    }
}

extern "C" fn window_will_exit_fullscreen(this: &Object, _: Sel, _: id) {
    let window_state = unsafe { get_window_state(this) };
    let mut lock = window_state.as_ref().lock();

    let min_version = NSOperatingSystemVersion::new(15, 3, 0);

    if is_macos_version_at_least(min_version) && lock.transparent_titlebar {
        unsafe {
            lock.native_window.setTitlebarAppearsTransparent_(YES);
        }
    }
}

pub(crate) fn is_macos_version_at_least(version: NSOperatingSystemVersion) -> bool {
    unsafe { NSProcessInfo::processInfo(nil).isOperatingSystemAtLeastVersion(version) }
}

extern "C" fn window_did_move(this: &Object, _: Sel, _: id) {
    let window_state = unsafe { get_window_state(this) };
    let mut lock = window_state.as_ref().lock();
    if let Some(mut callback) = lock.moved_callback.take() {
        drop(lock);
        callback();
        window_state.lock().moved_callback = Some(callback);
    }
}

// Update the window scale factor and drawable size, and call the resize callback if any.
fn update_window_scale_factor(window_state: &Arc<Mutex<MacWindowState>>) {
    let mut lock = window_state.as_ref().lock();
    let scale_factor = lock.scale_factor();
    let size = lock.content_size();
    let drawable_size = size.to_device_pixels(scale_factor);
    unsafe {
        let _: () = msg_send![
            lock.renderer.layer(),
            setContentsScale: scale_factor as f64
        ];
    }

    lock.renderer.update_drawable_size(drawable_size);

    if let Some(mut callback) = lock.resize_callback.take() {
        let content_size = lock.content_size();
        let scale_factor = lock.scale_factor();
        drop(lock);
        callback(content_size, scale_factor);
        window_state.as_ref().lock().resize_callback = Some(callback);
    };
}

extern "C" fn window_did_change_screen(this: &Object, _: Sel, _: id) {
    let window_state = unsafe { get_window_state(this) };
    let mut lock = window_state.as_ref().lock();
    lock.start_display_link();
    drop(lock);
    update_window_scale_factor(&window_state);
}

extern "C" fn window_did_change_key_status(this: &Object, selector: Sel, _: id) {
    let window_state = unsafe { get_window_state(this) };
    let mut lock = window_state.lock();
    let is_active = unsafe { lock.native_window.isKeyWindow() == YES };

    // When opening a pop-up while the application isn't active, Cocoa sends a spurious
    // `windowDidBecomeKey` message to the previous key window even though that window
    // isn't actually key. This causes a bug if the application is later activated while
    // the pop-up is still open, making it impossible to activate the previous key window
    // even if the pop-up gets closed. The only way to activate it again is to de-activate
    // the app and re-activate it, which is a pretty bad UX.
    // The following code detects the spurious event and invokes `resignKeyWindow`:
    // in theory, we're not supposed to invoke this method manually but it balances out
    // the spurious `becomeKeyWindow` event and helps us work around that bug.
    if selector == sel!(windowDidBecomeKey:) && !is_active {
        unsafe {
            let _: () = msg_send![lock.native_window, resignKeyWindow];
            return;
        }
    }

    lock.move_traffic_light();

    let executor = lock.foreground_executor.clone();
    drop(lock);

    // When a window becomes active, trigger an immediate synchronous frame request to prevent
    // tab flicker when switching between windows in native tabs mode.
    //
    // This is only done on subsequent activations (not the first) to ensure the initial focus
    // path is properly established. Without this guard, the focus state would remain unset until
    // the first mouse click, causing keybindings to be non-functional.
    if selector == sel!(windowDidBecomeKey:) && is_active {
        let window_state = unsafe { get_window_state(this) };
        let mut lock = window_state.lock();

        if lock.activated_least_once {
            if let Some(mut callback) = lock.request_frame_callback.take() {
                lock.renderer.set_presents_with_transaction(true);
                lock.stop_display_link();
                drop(lock);
                callback(Default::default());

                let mut lock = window_state.lock();
                lock.request_frame_callback = Some(callback);
                lock.renderer.set_presents_with_transaction(false);
                sync_renderer_drawable_size(&mut *lock);
                lock.start_display_link();
            }
        } else {
            lock.activated_least_once = true;
        }
    }

    executor
        .spawn(async move {
            let mut lock = window_state.as_ref().lock();
            lock.move_traffic_light();
            if let Some(mut callback) = lock.activate_callback.take() {
                drop(lock);
                callback(is_active);
                let mut lock = window_state.lock();
                lock.activate_callback = Some(callback);
                lock.move_traffic_light();
            };
        })
        .detach();
}

extern "C" fn window_should_close(this: &Object, _: Sel, _: id) -> BOOL {
    let window_state = unsafe { get_window_state(this) };
    let mut lock = window_state.as_ref().lock();
    if let Some(mut callback) = lock.should_close_callback.take() {
        drop(lock);
        let should_close = callback();
        window_state.lock().should_close_callback = Some(callback);
        should_close as BOOL
    } else {
        YES
    }
}

extern "C" fn close_window(this: &Object, _: Sel) {
    unsafe {
        let close_callback = {
            let window_state = get_window_state(this);
            let mut lock = window_state.as_ref().lock();
            lock.close_callback.take()
        };

        if let Some(callback) = close_callback {
            callback();
        }

        let _: () = msg_send![super(this, class!(NSWindow)), close];
    }
}

extern "C" fn make_backing_layer(this: &Object, _: Sel) -> id {
    let window_state = unsafe { get_window_state(this) };
    let window_state = window_state.as_ref().lock();
    window_state.renderer.layer_ptr() as id
}

extern "C" fn view_did_change_backing_properties(this: &Object, _: Sel) {
    let window_state = unsafe { get_window_state(this) };
    update_window_scale_factor(&window_state);
}

extern "C" fn set_frame_size(this: &Object, _: Sel, size: NSSize) {
    let window_state = unsafe { get_window_state(this) };
    let mut lock = window_state.as_ref().lock();

    let new_size = Size::<Pixels>::from(size);
    let old_size = unsafe {
        let old_frame: NSRect = msg_send![this, frame];
        Size::<Pixels>::from(old_frame.size)
    };

    if old_size == new_size {
        return;
    }

    unsafe {
        let _: () = msg_send![super(this, class!(NSView)), setFrameSize: size];
    }

    // When request_frame_callback is None, we're inside a frame rendering
    // cycle (display_layer/step took the callback). Updating the renderer's
    // drawable size mid-render and calling the resize callback would cause
    // re-entrant entity updates (RefCell already borrowed) and potentially
    // corrupt Metal state. Defer both — the next frame will pick up the
    // correct size via content_size() and bounds_changed().
    let mid_frame = lock.request_frame_callback.is_none();

    if !mid_frame {
        let scale_factor = lock.scale_factor();
        let drawable_size = new_size.to_device_pixels(scale_factor);
        lock.renderer.update_drawable_size(drawable_size);
    }

    if !mid_frame {
        if let Some(mut callback) = lock.resize_callback.take() {
            let content_size = lock.content_size();
            let scale_factor = lock.scale_factor();
            drop(lock);
            callback(content_size, scale_factor);
            window_state.lock().resize_callback = Some(callback);
        };
    }
}

extern "C" fn display_layer(this: &Object, _: Sel, _: id) {
    let window_state = unsafe { get_window_state(this) };
    let mut lock = window_state.lock();
    if let Some(mut callback) = lock.request_frame_callback.take() {
        lock.renderer.set_presents_with_transaction(true);
        lock.stop_display_link();
        drop(lock);
        callback(Default::default());

        let mut lock = window_state.lock();
        lock.request_frame_callback = Some(callback);
        lock.renderer.set_presents_with_transaction(false);
        // Sync renderer drawable size in case setFrameSize: was deferred
        // during the frame (e.g. native sidebar view reparenting).
        sync_renderer_drawable_size(&mut *lock);
        lock.start_display_link();
    }
}

unsafe extern "C" fn step(view: *mut c_void) {
    let view = view as id;
    let window_state = unsafe { get_window_state(&*view) };
    let mut lock = window_state.lock();

    if let Some(mut callback) = lock.request_frame_callback.take() {
        drop(lock);
        callback(Default::default());
        let mut lock = window_state.lock();
        // Sync renderer drawable size in case setFrameSize: was deferred
        sync_renderer_drawable_size(&mut *lock);
        lock.request_frame_callback = Some(callback);
    }
}

/// Re-syncs the renderer's drawable size with the native view's current frame.
/// Called after a frame rendering cycle to pick up any deferred size changes
/// from setFrameSize: calls that occurred mid-render.
fn sync_renderer_drawable_size(state: &mut MacWindowState) {
    let current_size: Size<Pixels> = unsafe {
        let view = state.native_view.as_ptr();
        let frame: NSRect = msg_send![view, frame];
        Size::<Pixels>::from(frame.size)
    };
    let scale_factor = state.scale_factor();
    let drawable_size = current_size.to_device_pixels(scale_factor);
    state.renderer.update_drawable_size(drawable_size);
}

extern "C" fn valid_attributes_for_marked_text(_: &Object, _: Sel) -> id {
    unsafe { msg_send![class!(NSArray), array] }
}

extern "C" fn has_marked_text(this: &Object, _: Sel) -> BOOL {
    let has_marked_text_result =
        with_input_handler(this, |input_handler| input_handler.marked_text_range()).flatten();

    has_marked_text_result.is_some() as BOOL
}

extern "C" fn marked_range(this: &Object, _: Sel) -> NSRange {
    let marked_range_result =
        with_input_handler(this, |input_handler| input_handler.marked_text_range()).flatten();

    marked_range_result.map_or(NSRange::invalid(), |range| range.into())
}

extern "C" fn selected_range(this: &Object, _: Sel) -> NSRange {
    let selected_range_result = with_input_handler(this, |input_handler| {
        input_handler.selected_text_range(false)
    })
    .flatten();

    selected_range_result.map_or(NSRange::invalid(), |selection| selection.range.into())
}

extern "C" fn first_rect_for_character_range(
    this: &Object,
    _: Sel,
    range: NSRange,
    _: id,
) -> NSRect {
    let frame = get_frame(this);
    with_input_handler(this, |input_handler| {
        input_handler.bounds_for_range(range.to_range()?)
    })
    .flatten()
    .map_or(
        NSRect::new(NSPoint::new(0., 0.), NSSize::new(0., 0.)),
        |bounds| {
            NSRect::new(
                NSPoint::new(
                    frame.origin.x + bounds.origin.x.0 as f64,
                    frame.origin.y + frame.size.height
                        - bounds.origin.y.0 as f64
                        - bounds.size.height.0 as f64,
                ),
                NSSize::new(bounds.size.width.0 as f64, bounds.size.height.0 as f64),
            )
        },
    )
}

fn get_frame(this: &Object) -> NSRect {
    unsafe {
        let state = get_window_state(this);
        let lock = state.lock();
        let mut frame = NSWindow::frame(lock.native_window);
        let content_layout_rect: CGRect = msg_send![lock.native_window, contentLayoutRect];
        let style_mask: NSWindowStyleMask = msg_send![lock.native_window, styleMask];
        if !style_mask.contains(NSWindowStyleMask::NSFullSizeContentViewWindowMask) {
            frame.origin.y -= frame.size.height - content_layout_rect.size.height;
        }
        frame
    }
}

extern "C" fn insert_text(this: &Object, _: Sel, text: id, replacement_range: NSRange) {
    unsafe {
        let is_attributed_string: BOOL =
            msg_send![text, isKindOfClass: [class!(NSAttributedString)]];
        let text: id = if is_attributed_string == YES {
            msg_send![text, string]
        } else {
            text
        };

        let text = text.to_str();
        let replacement_range = replacement_range.to_range();
        with_input_handler(this, |input_handler| {
            input_handler.replace_text_in_range(replacement_range, text)
        });
    }
}

extern "C" fn set_marked_text(
    this: &Object,
    _: Sel,
    text: id,
    selected_range: NSRange,
    replacement_range: NSRange,
) {
    unsafe {
        let is_attributed_string: BOOL =
            msg_send![text, isKindOfClass: [class!(NSAttributedString)]];
        let text: id = if is_attributed_string == YES {
            msg_send![text, string]
        } else {
            text
        };
        let selected_range = selected_range.to_range();
        let replacement_range = replacement_range.to_range();
        let text = text.to_str();
        with_input_handler(this, |input_handler| {
            input_handler.replace_and_mark_text_in_range(replacement_range, text, selected_range)
        });
    }
}
extern "C" fn unmark_text(this: &Object, _: Sel) {
    with_input_handler(this, |input_handler| input_handler.unmark_text());
}

extern "C" fn attributed_substring_for_proposed_range(
    this: &Object,
    _: Sel,
    range: NSRange,
    actual_range: *mut c_void,
) -> id {
    with_input_handler(this, |input_handler| {
        let range = range.to_range()?;
        if range.is_empty() {
            return None;
        }
        let mut adjusted: Option<Range<usize>> = None;

        let selected_text = input_handler.text_for_range(range.clone(), &mut adjusted)?;
        if let Some(adjusted) = adjusted
            && adjusted != range
        {
            unsafe { (actual_range as *mut NSRange).write(NSRange::from(adjusted)) };
        }
        unsafe {
            let string: id = msg_send![class!(NSAttributedString), alloc];
            let string: id = msg_send![string, initWithString: ns_string(&selected_text)];
            Some(string)
        }
    })
    .flatten()
    .unwrap_or(nil)
}

// We ignore which selector it asks us to do because the user may have
// bound the shortcut to something else.
extern "C" fn do_command_by_selector(this: &Object, _: Sel, _: Sel) {
    let state = unsafe { get_window_state(this) };
    let mut lock = state.as_ref().lock();
    let keystroke = lock.keystroke_for_do_command.take();
    let mut event_callback = lock.event_callback.take();
    drop(lock);

    if let Some((keystroke, mut callback)) = keystroke.zip(event_callback.as_mut()) {
        // AppKit calls this Objective-C method directly. If user callback code
        // panics, we must not unwind across the FFI boundary.
        let handled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (callback)(PlatformInput::KeyDown(KeyDownEvent {
                keystroke,
                is_held: false,
                prefer_character_input: false,
            }))
        }));
        match handled {
            Ok(handled) => {
                state.as_ref().lock().do_command_handled = Some(!handled.propagate);
            }
            Err(_) => {
                log::error!("panic while handling do_command_by_selector");
                state.as_ref().lock().do_command_handled = Some(false);
            }
        }
    }

    state.as_ref().lock().event_callback = event_callback;
}

extern "C" fn view_did_change_effective_appearance(this: &Object, _: Sel) {
    unsafe {
        let state = get_window_state(this);
        let mut lock = state.as_ref().lock();
        if let Some(mut callback) = lock.appearance_changed_callback.take() {
            drop(lock);
            callback();
            state.lock().appearance_changed_callback = Some(callback);
        }
    }
}

extern "C" fn accepts_first_mouse(this: &Object, _: Sel, _: id) -> BOOL {
    let window_state = unsafe { get_window_state(this) };
    let mut lock = window_state.as_ref().lock();
    lock.first_mouse = true;
    YES
}

extern "C" fn view_is_flipped(_: &Object, _: Sel) -> BOOL {
    NO
}

extern "C" fn character_index_for_point(this: &Object, _: Sel, position: NSPoint) -> u64 {
    let position = screen_point_to_gpui_point(this, position);
    with_input_handler(this, |input_handler| {
        input_handler.character_index_for_point(position)
    })
    .flatten()
    .map(|index| index as u64)
    .unwrap_or(NSNotFound as u64)
}

fn screen_point_to_gpui_point(this: &Object, position: NSPoint) -> Point<Pixels> {
    let frame = get_frame(this);
    let window_x = position.x - frame.origin.x;
    let window_y = frame.size.height - (position.y - frame.origin.y);

    point(px(window_x as f32), px(window_y as f32))
}

extern "C" fn dragging_entered(this: &Object, _: Sel, dragging_info: id) -> NSDragOperation {
    let window_state = unsafe { get_window_state(this) };
    let position = drag_event_position(&window_state, dragging_info);
    let paths = external_paths_from_event(dragging_info);
    if let Some(event) = paths.map(|paths| FileDropEvent::Entered { position, paths })
        && send_file_drop_event(window_state, event)
    {
        return NSDragOperationCopy;
    }
    NSDragOperationNone
}

extern "C" fn dragging_updated(this: &Object, _: Sel, dragging_info: id) -> NSDragOperation {
    let window_state = unsafe { get_window_state(this) };
    let position = drag_event_position(&window_state, dragging_info);
    if send_file_drop_event(window_state, FileDropEvent::Pending { position }) {
        NSDragOperationCopy
    } else {
        NSDragOperationNone
    }
}

extern "C" fn dragging_exited(this: &Object, _: Sel, _: id) {
    let window_state = unsafe { get_window_state(this) };
    send_file_drop_event(window_state, FileDropEvent::Exited);
}

extern "C" fn perform_drag_operation(this: &Object, _: Sel, dragging_info: id) -> BOOL {
    let window_state = unsafe { get_window_state(this) };
    let position = drag_event_position(&window_state, dragging_info);
    send_file_drop_event(window_state, FileDropEvent::Submit { position }).to_objc()
}

fn external_paths_from_event(dragging_info: *mut Object) -> Option<ExternalPaths> {
    let mut paths = SmallVec::new();
    let pasteboard: id = unsafe { msg_send![dragging_info, draggingPasteboard] };
    let filenames = unsafe { NSPasteboard::propertyListForType(pasteboard, NSFilenamesPboardType) };
    if filenames == nil {
        return None;
    }
    for file in unsafe { filenames.iter() } {
        let path = unsafe {
            let f = NSString::UTF8String(file);
            CStr::from_ptr(f).to_string_lossy().into_owned()
        };
        paths.push(PathBuf::from(path))
    }
    Some(ExternalPaths(paths))
}

extern "C" fn conclude_drag_operation(this: &Object, _: Sel, _: id) {
    let window_state = unsafe { get_window_state(this) };
    send_file_drop_event(window_state, FileDropEvent::Exited);
}

async fn synthetic_drag(
    window_state: Weak<Mutex<MacWindowState>>,
    drag_id: usize,
    event: MouseMoveEvent,
    executor: BackgroundExecutor,
) {
    loop {
        executor.timer(Duration::from_millis(16)).await;
        if let Some(window_state) = window_state.upgrade() {
            let mut lock = window_state.lock();
            if lock.synthetic_drag_counter == drag_id {
                if let Some(mut callback) = lock.event_callback.take() {
                    drop(lock);
                    callback(PlatformInput::MouseMove(event.clone()));
                    window_state.lock().event_callback = Some(callback);
                }
            } else {
                break;
            }
        }
    }
}

/// Sends the specified FileDropEvent using `PlatformInput::FileDrop` to the window
/// state and updates the window state according to the event passed.
fn send_file_drop_event(
    window_state: Arc<Mutex<MacWindowState>>,
    file_drop_event: FileDropEvent,
) -> bool {
    let mut window_state = window_state.lock();
    let window_event_callback = window_state.event_callback.as_mut();
    if let Some(mut callback) = window_event_callback {
        let external_files_dragged = match file_drop_event {
            FileDropEvent::Entered { .. } => Some(true),
            FileDropEvent::Exited => Some(false),
            _ => None,
        };

        callback(PlatformInput::FileDrop(file_drop_event));

        if let Some(external_files_dragged) = external_files_dragged {
            window_state.external_files_dragged = external_files_dragged;
        }
        true
    } else {
        false
    }
}

fn drag_event_position(window_state: &Mutex<MacWindowState>, dragging_info: id) -> Point<Pixels> {
    let drag_location: NSPoint = unsafe { msg_send![dragging_info, draggingLocation] };
    let lock = window_state.lock();
    let native_view = lock.native_view.as_ptr() as id;
    unsafe {
        let local_point: NSPoint =
            msg_send![native_view, convertPoint:drag_location fromView:nil];
        let bounds: NSRect = msg_send![native_view, bounds];
        point(
            px(local_point.x as f32),
            px((bounds.size.height - local_point.y) as f32),
        )
    }
}

fn with_input_handler<F, R>(window: &Object, f: F) -> Option<R>
where
    F: FnOnce(&mut PlatformInputHandler) -> R,
{
    let window_state = unsafe { get_window_state(window) };
    let mut lock = window_state.as_ref().lock();
    if let Some(mut input_handler) = lock.input_handler.take() {
        drop(lock);
        let result = f(&mut input_handler);
        window_state.lock().input_handler = Some(input_handler);
        Some(result)
    } else {
        None
    }
}

unsafe fn display_id_for_screen(screen: id) -> CGDirectDisplayID {
    unsafe {
        let device_description = NSScreen::deviceDescription(screen);
        let screen_number_key: id = ns_string("NSScreenNumber");
        let screen_number = device_description.objectForKey_(screen_number_key);
        let screen_number: NSUInteger = msg_send![screen_number, unsignedIntegerValue];
        screen_number as CGDirectDisplayID
    }
}

fn toolbar_display_mode_to_native(mode: PlatformNativeToolbarDisplayMode) -> NSUInteger {
    match mode {
        PlatformNativeToolbarDisplayMode::Default => NSToolbarDisplayModeDefault,
        PlatformNativeToolbarDisplayMode::IconAndLabel => NSToolbarDisplayModeIconAndLabel,
        PlatformNativeToolbarDisplayMode::IconOnly => NSToolbarDisplayModeIconOnly,
        PlatformNativeToolbarDisplayMode::LabelOnly => NSToolbarDisplayModeLabelOnly,
    }
}

fn toolbar_size_mode_to_native(mode: PlatformNativeToolbarSizeMode) -> NSUInteger {
    match mode {
        PlatformNativeToolbarSizeMode::Default => NSToolbarSizeModeDefault,
        PlatformNativeToolbarSizeMode::Regular => NSToolbarSizeModeRegular,
        PlatformNativeToolbarSizeMode::Small => NSToolbarSizeModeSmall,
    }
}

fn cleanup_popover_state(popover_state: &mut Option<MacPopoverState>) {
    if let Some(state) = popover_state.take() {
        unsafe {
            crate::platform::native_controls::dismiss_native_popover(state.popover);
            for target in &state.button_targets {
                crate::platform::native_controls::release_native_button_target(*target);
            }
            for target in &state.switch_targets {
                crate::platform::native_controls::release_native_popover_switch_target(*target);
            }
            for target in &state.checkbox_targets {
                crate::platform::native_controls::release_native_popover_checkbox_target(*target);
            }
            for target in &state.hover_row_targets {
                crate::platform::native_controls::release_native_hover_row_target(*target);
            }
            crate::platform::native_controls::release_native_popover(
                state.popover,
                state.delegate_ptr,
            );
        }
    }
}

fn cleanup_panel_state(panel_state: &mut Option<MacPanelState>) {
    if let Some(state) = panel_state.take() {
        unsafe {
            for target in &state.button_targets {
                crate::platform::native_controls::release_native_button_target(*target);
            }
            for target in &state.switch_targets {
                crate::platform::native_controls::release_native_popover_switch_target(*target);
            }
            for target in &state.checkbox_targets {
                crate::platform::native_controls::release_native_popover_checkbox_target(*target);
            }
            for target in &state.hover_row_targets {
                crate::platform::native_controls::release_native_hover_row_target(*target);
            }
            crate::platform::native_controls::release_native_panel(
                state.panel,
                state.delegate_ptr,
            );
        }
    }
}

unsafe fn clear_native_toolbar_for_window(native_window: id, toolbar: Option<MacToolbarState>) {
    unsafe {
        let _: () = msg_send![native_window, setToolbar: nil];
        if let Some(mut toolbar) = toolbar {
            toolbar.cleanup();
        }
    }
}

unsafe fn build_native_toolbar_for_window(
    native_window: id,
    toolbar: PlatformNativeToolbar,
) -> MacToolbarState {
    unsafe {
        if let Some(title) = toolbar.title.as_ref() {
            let _: () = msg_send![native_window, setTitle: ns_string(title.as_ref())];
        }

        let has_sidebar_items = toolbar.items.iter().any(|item| {
            matches!(
                item,
                PlatformNativeToolbarItem::SidebarToggle
                    | PlatformNativeToolbarItem::SidebarTrackingSeparator
            )
        });

        let mut allowed_item_identifiers = Vec::with_capacity(toolbar.items.len());
        let mut default_item_identifiers = Vec::with_capacity(toolbar.items.len());
        for item in &toolbar.items {
            match item {
                PlatformNativeToolbarItem::Button(button) => {
                    allowed_item_identifiers.push(button.id.clone());
                    default_item_identifiers.push(button.id.clone());
                }
                PlatformNativeToolbarItem::SearchField(search) => {
                    allowed_item_identifiers.push(search.id.clone());
                    default_item_identifiers.push(search.id.clone());
                }
                PlatformNativeToolbarItem::Space => {
                    let identifier = SharedString::from("NSToolbarSpaceItem");
                    allowed_item_identifiers.push(identifier.clone());
                    default_item_identifiers.push(identifier);
                }
                PlatformNativeToolbarItem::FlexibleSpace => {
                    let identifier = SharedString::from("NSToolbarFlexibleSpaceItem");
                    allowed_item_identifiers.push(identifier.clone());
                    default_item_identifiers.push(identifier);
                }
                PlatformNativeToolbarItem::SegmentedControl(segmented) => {
                    allowed_item_identifiers.push(segmented.id.clone());
                    default_item_identifiers.push(segmented.id.clone());
                }
                PlatformNativeToolbarItem::PopUpButton(popup) => {
                    allowed_item_identifiers.push(popup.id.clone());
                    default_item_identifiers.push(popup.id.clone());
                }
                PlatformNativeToolbarItem::ComboBox(combo) => {
                    allowed_item_identifiers.push(combo.id.clone());
                    default_item_identifiers.push(combo.id.clone());
                }
                PlatformNativeToolbarItem::MenuButton(menu_button) => {
                    allowed_item_identifiers.push(menu_button.id.clone());
                    default_item_identifiers.push(menu_button.id.clone());
                }
                PlatformNativeToolbarItem::Label(label) => {
                    allowed_item_identifiers.push(label.id.clone());
                    default_item_identifiers.push(label.id.clone());
                }
                PlatformNativeToolbarItem::SidebarToggle => {
                    let identifier = SharedString::from(ns_string_to_owned(
                        NSToolbarToggleSidebarItemIdentifier,
                    ));
                    allowed_item_identifiers.push(identifier.clone());
                    default_item_identifiers.push(identifier);
                }
                PlatformNativeToolbarItem::SidebarTrackingSeparator => {
                    let identifier = SharedString::from(ns_string_to_owned(
                        NSToolbarSidebarTrackingSeparatorItemIdentifier,
                    ));
                    allowed_item_identifiers.push(identifier.clone());
                    default_item_identifiers.push(identifier);
                }
            }
        }

        let state = ToolbarState {
            allowed_item_identifiers,
            default_item_identifiers,
            items: toolbar.items,
            resources: Vec::new(),
        };
        let state_ptr = Box::into_raw(Box::new(state)) as *mut c_void;

        let delegate: id = msg_send![TOOLBAR_DELEGATE_CLASS, alloc];
        let delegate: id = msg_send![delegate, init];
        (*delegate).set_ivar(TOOLBAR_STATE_IVAR, state_ptr);

        let toolbar_obj: id = msg_send![class!(NSToolbar), alloc];
        let toolbar_obj: id =
            msg_send![toolbar_obj, initWithIdentifier: ns_string(toolbar.identifier.as_ref())];
        let _: () = msg_send![toolbar_obj, setDelegate: delegate];
        let _: () = msg_send![toolbar_obj, setShowsBaselineSeparator: toolbar.shows_baseline_separator.to_objc()];
        let _: () = msg_send![
            toolbar_obj,
            setDisplayMode: toolbar_display_mode_to_native(toolbar.display_mode)
        ];
        let _: () =
            msg_send![toolbar_obj, setSizeMode: toolbar_size_mode_to_native(toolbar.size_mode)];

        let _: () = msg_send![native_window, setToolbar: toolbar_obj];

        if has_sidebar_items {
            let supports_toolbar_style: bool =
                msg_send![native_window, respondsToSelector: sel!(setToolbarStyle:)];
            if supports_toolbar_style {
                let _: () = msg_send![native_window, setToolbarStyle: 3i64];
            }
        }

        MacToolbarState {
            toolbar: toolbar_obj,
            delegate,
            state_ptr,
        }
    }
}

unsafe fn get_toolbar_state(this: &Object) -> &'static mut ToolbarState {
    unsafe {
        let raw: *mut c_void = *this.get_ivar(TOOLBAR_STATE_IVAR);
        &mut *(raw as *mut ToolbarState)
    }
}

unsafe fn ns_string_to_owned(ns_string: id) -> String {
    unsafe {
        let cstr: *const std::os::raw::c_char = msg_send![ns_string, UTF8String];
        if cstr.is_null() {
            String::new()
        } else {
            CStr::from_ptr(cstr).to_string_lossy().into_owned()
        }
    }
}

unsafe fn toolbar_identifiers_to_ns_array(identifiers: &[SharedString]) -> id {
    unsafe {
        let array: id = msg_send![class!(NSMutableArray), array];
        for identifier in identifiers {
            let _: () = msg_send![array, addObject: ns_string(identifier.as_ref())];
        }
        array
    }
}

extern "C" fn toolbar_allowed_item_identifiers(this: &Object, _: Sel, _: id) -> id {
    unsafe {
        let state = get_toolbar_state(this);
        toolbar_identifiers_to_ns_array(&state.allowed_item_identifiers)
    }
}

extern "C" fn toolbar_default_item_identifiers(this: &Object, _: Sel, _: id) -> id {
    unsafe {
        let state = get_toolbar_state(this);
        toolbar_identifiers_to_ns_array(&state.default_item_identifiers)
    }
}

extern "C" fn toolbar_item_for_identifier(
    this: &Object,
    _: Sel,
    _: id,
    identifier: id,
    _: BOOL,
) -> id {
    unsafe {
        let is_space: BOOL = msg_send![identifier, isEqual: NSToolbarSpaceItemIdentifier];
        let is_flexible_space: BOOL =
            msg_send![identifier, isEqual: NSToolbarFlexibleSpaceItemIdentifier];
        let is_sidebar_toggle: BOOL =
            msg_send![identifier, isEqual: NSToolbarToggleSidebarItemIdentifier];
        let is_sidebar_separator: BOOL =
            msg_send![identifier, isEqual: NSToolbarSidebarTrackingSeparatorItemIdentifier];
        if is_space == YES
            || is_flexible_space == YES
            || is_sidebar_toggle == YES
            || is_sidebar_separator == YES
        {
            return nil;
        }

        let identifier_string = ns_string_to_owned(identifier);
        let state = get_toolbar_state(this);

        match state.item_for_identifier(&identifier_string) {
            Some(PlatformNativeToolbarItem::Button(_)) => {
                create_toolbar_button_item(this, state, identifier, &identifier_string)
            }
            Some(PlatformNativeToolbarItem::SearchField(_)) => {
                create_toolbar_search_item(this, state, identifier, &identifier_string)
            }
            Some(PlatformNativeToolbarItem::SegmentedControl(_)) => {
                create_toolbar_segmented_item(this, state, identifier, &identifier_string)
            }
            Some(PlatformNativeToolbarItem::PopUpButton(_)) => {
                create_toolbar_popup_item(this, state, identifier, &identifier_string)
            }
            Some(PlatformNativeToolbarItem::ComboBox(_)) => {
                create_toolbar_combo_box_item(this, state, identifier, &identifier_string)
            }
            Some(PlatformNativeToolbarItem::MenuButton(_)) => {
                create_toolbar_menu_button_item(this, state, identifier, &identifier_string)
            }
            Some(PlatformNativeToolbarItem::Label(_)) => {
                create_toolbar_label_item(state, identifier, &identifier_string)
            }
            Some(PlatformNativeToolbarItem::Space)
            | Some(PlatformNativeToolbarItem::FlexibleSpace)
            | Some(PlatformNativeToolbarItem::SidebarToggle)
            | Some(PlatformNativeToolbarItem::SidebarTrackingSeparator)
            | None => nil,
        }
    }
}

struct ToolbarImageContext {
    toolbar_item: id,
    image: id,
}

unsafe extern "C" fn set_toolbar_image_on_main(context: *mut c_void) {
    unsafe {
        let ctx = Box::from_raw(context as *mut ToolbarImageContext);
        let _: () = msg_send![ctx.toolbar_item, setImage: ctx.image];
        let _: () = msg_send![ctx.image, release];
        let _: () = msg_send![ctx.toolbar_item, release];
    }
}

unsafe extern "C" fn release_toolbar_item_on_main(context: *mut c_void) {
    unsafe {
        let item = context as id;
        let _: () = msg_send![item, release];
    }
}

/// Asynchronously loads an image from a URL and sets it on an NSToolbarItem.
/// If `circular` is true, the image is clipped to a circle (for avatars).
unsafe fn load_toolbar_image_from_url(toolbar_item: id, url_str: &str, circular: bool) {
    unsafe {
        let ns_url_string = ns_string(url_str);
        let url: id = msg_send![class!(NSURL), URLWithString: ns_url_string];
        if url == nil {
            return;
        }

        // Retain the toolbar item so it stays alive during the async load
        let _: () = msg_send![toolbar_item, retain];

        let session: id = msg_send![class!(NSURLSession), sharedSession];
        let block = ConcreteBlock::new(move |data: id, _response: id, error: id| {
            if error != nil || data == nil {
                // Release on main thread since NSToolbarItem isn't thread-safe
                dispatch_async_f(
                    dispatch_get_main_queue(),
                    toolbar_item as *mut c_void,
                    Some(release_toolbar_item_on_main as unsafe extern "C" fn(*mut c_void)),
                );
                return;
            }

            let image: id = msg_send![class!(NSImage), alloc];
            let image: id = msg_send![image, initWithData: data];
            if image == nil {
                dispatch_async_f(
                    dispatch_get_main_queue(),
                    toolbar_item as *mut c_void,
                    Some(release_toolbar_item_on_main as unsafe extern "C" fn(*mut c_void)),
                );
                return;
            }

            // Scale image to toolbar-appropriate size (24x24 points)
            let toolbar_size = 24.0_f64;
            let target_size = NSSize {
                width: toolbar_size,
                height: toolbar_size,
            };

            let new_image: id = msg_send![class!(NSImage), alloc];
            let new_image: id = msg_send![new_image, initWithSize: target_size];
            let _: () = msg_send![new_image, lockFocus];

            if circular {
                let oval_rect = NSRect::new(NSPoint::new(0.0, 0.0), target_size);
                let path: id =
                    msg_send![class!(NSBezierPath), bezierPathWithOvalInRect: oval_rect];
                let _: () = msg_send![path, addClip];
            }

            let dst_rect = NSRect::new(NSPoint::new(0.0, 0.0), target_size);
            let zero_rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize { width: 0.0, height: 0.0 });
            // NSCompositingOperationSourceOver = 2
            let _: () = msg_send![
                image,
                drawInRect: dst_rect
                fromRect: zero_rect
                operation: 2u64
                fraction: 1.0f64
            ];

            let _: () = msg_send![new_image, unlockFocus];
            // Mark as template: NO so AppKit renders the actual image colors
            let _: () = msg_send![new_image, setTemplate: false];
            let _: () = msg_send![image, release];
            let final_image = new_image;

            // Dispatch to main thread to set the image on the toolbar item
            let ctx = Box::new(ToolbarImageContext {
                toolbar_item,
                image: final_image,
            });
            dispatch_async_f(
                dispatch_get_main_queue(),
                Box::into_raw(ctx) as *mut c_void,
                Some(set_toolbar_image_on_main as unsafe extern "C" fn(*mut c_void)),
            );
        });
        let block = block.copy();

        let task: id = msg_send![session, dataTaskWithURL: url completionHandler: &*block];
        let _: () = msg_send![task, resume];
    }
}

unsafe fn create_toolbar_button_item(
    this: &Object,
    state: &mut ToolbarState,
    identifier: id,
    identifier_string: &str,
) -> id {
    unsafe {
        let Some(PlatformNativeToolbarItem::Button(item)) =
            state.item_for_identifier(identifier_string)
        else {
            return nil;
        };
        let label = item.label.clone();
        let tool_tip = item.tool_tip.clone();
        let icon = item.icon.clone();
        let image_url = item.image_url.clone();
        let image_circular = item.image_circular;

        let toolbar_item: id = msg_send![class!(NSToolbarItem), alloc];
        let toolbar_item: id = msg_send![toolbar_item, initWithItemIdentifier: identifier];
        let button = crate::platform::native_controls::create_native_button(label.as_ref());

        let state_ptr: *mut c_void = *this.get_ivar(TOOLBAR_STATE_IVAR);
        let callback_identifier = identifier_string.to_owned();
        let action = Box::new(move || {
            let state = &*(state_ptr as *const ToolbarState);
            if let Some(PlatformNativeToolbarItem::Button(button_item)) =
                state.item_for_identifier(&callback_identifier)
                && let Some(callback) = button_item.on_click.as_ref()
            {
                callback();
            }
        });
        let target = crate::platform::native_controls::set_native_button_action(button, action);
        if let Some(tool_tip) = tool_tip.as_ref() {
            crate::platform::native_controls::set_native_view_tooltip(button, tool_tip.as_ref());
        }

        // Set SF Symbol icon on both the toolbar item and the button view.
        // The toolbar item image is used by the customization panel,
        // while the button image is what's actually displayed (since setView: overrides).
        if let Some(icon) = icon.as_ref() {
            let symbol_name = ns_string(icon.as_ref());
            let image: id = msg_send![
                class!(NSImage),
                imageWithSystemSymbolName: symbol_name
                accessibilityDescription: nil
            ];
            if image != nil {
                let _: () = msg_send![toolbar_item, setImage: image];
            }
            let image_only = label.is_empty();
            crate::platform::native_controls::set_native_button_sf_symbol(
                button,
                icon.as_ref(),
                image_only,
            );
        }

        // Load image from URL asynchronously if provided
        if let Some(url_str) = image_url.as_ref() {
            load_toolbar_image_from_url(toolbar_item, url_str, image_circular);
        }

        let _: () = msg_send![toolbar_item, setLabel: ns_string(label.as_ref())];
        let size: NSSize = msg_send![button, fittingSize];
        let _: () = msg_send![toolbar_item, setMinSize: size];
        let _: () = msg_send![toolbar_item, setMaxSize: size];
        let _: () = msg_send![toolbar_item, setView: button];

        state
            .resources
            .push(ToolbarNativeResource::Button { button, target });

        msg_send![toolbar_item, autorelease]
    }
}

unsafe fn create_toolbar_search_item(
    this: &Object,
    state: &mut ToolbarState,
    identifier: id,
    identifier_string: &str,
) -> id {
    unsafe {
        let Some(PlatformNativeToolbarItem::SearchField(item)) =
            state.item_for_identifier(identifier_string)
        else {
            return nil;
        };
        let placeholder = item.placeholder.clone();
        let text = item.text.clone();
        let min_width = item.min_width;
        let max_width = item.max_width;

        let toolbar_item: id = msg_send![class!(NSToolbarItem), alloc];
        let toolbar_item: id = msg_send![toolbar_item, initWithItemIdentifier: identifier];
        let field =
            crate::platform::native_controls::create_native_search_field(placeholder.as_ref());
        crate::platform::native_controls::set_native_search_field_string_value(field, &text);

        let state_ptr: *mut c_void = *this.get_ivar(TOOLBAR_STATE_IVAR);
        let change_identifier = identifier_string.to_owned();
        let submit_identifier = identifier_string.to_owned();
        let move_up_identifier = identifier_string.to_owned();
        let move_down_identifier = identifier_string.to_owned();
        let cancel_identifier = identifier_string.to_owned();
        let begin_editing_identifier = identifier_string.to_owned();
        let end_editing_identifier = identifier_string.to_owned();

        let on_change = Some(Box::new(move |text: String| {
            let state = &*(state_ptr as *const ToolbarState);
            if let Some(PlatformNativeToolbarItem::SearchField(search_item)) =
                state.item_for_identifier(&change_identifier)
                && let Some(callback) = search_item.on_change.as_ref()
            {
                callback(text);
            }
        }) as Box<dyn Fn(String)>);

        let on_submit = Some(Box::new(move |text: String| {
            let state = &*(state_ptr as *const ToolbarState);
            if let Some(PlatformNativeToolbarItem::SearchField(search_item)) =
                state.item_for_identifier(&submit_identifier)
                && let Some(callback) = search_item.on_submit.as_ref()
            {
                callback(text);
            }
        }) as Box<dyn Fn(String)>);

        let on_move_up = Some(Box::new(move || {
            let state = &*(state_ptr as *const ToolbarState);
            if let Some(PlatformNativeToolbarItem::SearchField(search_item)) =
                state.item_for_identifier(&move_up_identifier)
                && let Some(callback) = search_item.on_move_up.as_ref()
            {
                callback();
            }
        }) as Box<dyn Fn()>);

        let on_move_down = Some(Box::new(move || {
            let state = &*(state_ptr as *const ToolbarState);
            if let Some(PlatformNativeToolbarItem::SearchField(search_item)) =
                state.item_for_identifier(&move_down_identifier)
                && let Some(callback) = search_item.on_move_down.as_ref()
            {
                callback();
            }
        }) as Box<dyn Fn()>);

        let on_cancel = Some(Box::new(move || {
            let state = &*(state_ptr as *const ToolbarState);
            if let Some(PlatformNativeToolbarItem::SearchField(search_item)) =
                state.item_for_identifier(&cancel_identifier)
                && let Some(callback) = search_item.on_cancel.as_ref()
            {
                callback();
            }
        }) as Box<dyn Fn()>);

        let on_end_editing = Some(Box::new(move |text: String| {
            let state = &*(state_ptr as *const ToolbarState);
            if let Some(PlatformNativeToolbarItem::SearchField(search_item)) =
                state.item_for_identifier(&end_editing_identifier)
                && let Some(callback) = search_item.on_end_editing.as_ref()
            {
                callback(text);
            }
        }) as Box<dyn Fn(String)>);

        let on_begin_editing_cb = Some(Box::new(move || {
            let state = &*(state_ptr as *const ToolbarState);
            if let Some(PlatformNativeToolbarItem::SearchField(search_item)) =
                state.item_for_identifier(&begin_editing_identifier)
                && let Some(callback) = search_item.on_begin_editing.as_ref()
            {
                callback();
            }
        }) as Box<dyn Fn()>);

        let callbacks = crate::platform::native_controls::TextFieldCallbacks {
            on_change,
            on_begin_editing: on_begin_editing_cb,
            on_end_editing,
            on_submit,
            on_move_up,
            on_move_down,
            on_cancel,
        };
        let delegate =
            crate::platform::native_controls::set_native_text_field_delegate(field, callbacks);

        let frame: NSRect = msg_send![field, frame];
        let min_size = NSSize {
            width: min_width.to_f64(),
            height: frame.size.height,
        };
        let max_size = NSSize {
            width: max_width.to_f64(),
            height: frame.size.height,
        };

        let _: () = msg_send![toolbar_item, setMinSize: min_size];
        let _: () = msg_send![toolbar_item, setMaxSize: max_size];
        let _: () = msg_send![toolbar_item, setView: field];

        state
            .resources
            .push(ToolbarNativeResource::SearchField { field, delegate });

        msg_send![toolbar_item, autorelease]
    }
}

unsafe fn create_toolbar_segmented_item(
    this: &Object,
    state: &mut ToolbarState,
    identifier: id,
    identifier_string: &str,
) -> id {
    unsafe {
        let Some(PlatformNativeToolbarItem::SegmentedControl(item)) =
            state.item_for_identifier(identifier_string)
        else {
            return nil;
        };
        let labels: Vec<&str> = item.labels.iter().map(|l| l.as_ref()).collect();
        let selected_index = item.selected_index;
        let icons: Vec<Option<SharedString>> = item.icons.clone();

        let toolbar_item: id = msg_send![class!(NSToolbarItem), alloc];
        let toolbar_item: id = msg_send![toolbar_item, initWithItemIdentifier: identifier];

        let control =
            crate::platform::native_controls::create_native_segmented_control(&labels, Some(selected_index));

        for (segment_index, icon) in icons.iter().enumerate() {
            if let Some(symbol_name) = icon {
                crate::platform::native_controls::set_native_segmented_image(
                    control,
                    segment_index,
                    symbol_name.as_ref(),
                );
            }
        }

        let state_ptr: *mut c_void = *this.get_ivar(TOOLBAR_STATE_IVAR);
        let callback_identifier = identifier_string.to_owned();
        let action = Box::new(move |selected_index: usize| {
            let state = &*(state_ptr as *const ToolbarState);
            if let Some(PlatformNativeToolbarItem::SegmentedControl(seg_item)) =
                state.item_for_identifier(&callback_identifier)
                && let Some(callback) = seg_item.on_select.as_ref()
            {
                callback(selected_index);
            }
        });
        let target = crate::platform::native_controls::set_native_segmented_action(control, action);

        let size: NSSize = msg_send![control, fittingSize];
        let _: () = msg_send![toolbar_item, setMinSize: size];
        let _: () = msg_send![toolbar_item, setMaxSize: size];
        let _: () = msg_send![toolbar_item, setView: control];

        state
            .resources
            .push(ToolbarNativeResource::SegmentedControl { control, target });

        msg_send![toolbar_item, autorelease]
    }
}

unsafe fn create_toolbar_popup_item(
    this: &Object,
    state: &mut ToolbarState,
    identifier: id,
    identifier_string: &str,
) -> id {
    unsafe {
        let Some(PlatformNativeToolbarItem::PopUpButton(item)) =
            state.item_for_identifier(identifier_string)
        else {
            return nil;
        };
        let menu_items: Vec<&str> = item.items.iter().map(|i| i.as_ref()).collect();
        let selected_index = item.selected_index;

        let toolbar_item: id = msg_send![class!(NSToolbarItem), alloc];
        let toolbar_item: id = msg_send![toolbar_item, initWithItemIdentifier: identifier];

        let popup =
            crate::platform::native_controls::create_native_popup_button(&menu_items, selected_index);

        let state_ptr: *mut c_void = *this.get_ivar(TOOLBAR_STATE_IVAR);
        let callback_identifier = identifier_string.to_owned();
        let action = Box::new(move |selected_index: usize| {
            let state = &*(state_ptr as *const ToolbarState);
            if let Some(PlatformNativeToolbarItem::PopUpButton(popup_item)) =
                state.item_for_identifier(&callback_identifier)
                && let Some(callback) = popup_item.on_select.as_ref()
            {
                callback(selected_index);
            }
        });
        let target = crate::platform::native_controls::set_native_popup_action(popup, action);

        let size: NSSize = msg_send![popup, fittingSize];
        let _: () = msg_send![toolbar_item, setMinSize: size];
        let _: () = msg_send![toolbar_item, setMaxSize: size];
        let _: () = msg_send![toolbar_item, setView: popup];

        state
            .resources
            .push(ToolbarNativeResource::PopUpButton { popup, target });

        msg_send![toolbar_item, autorelease]
    }
}

unsafe fn create_toolbar_combo_box_item(
    this: &Object,
    state: &mut ToolbarState,
    identifier: id,
    identifier_string: &str,
) -> id {
    unsafe {
        let Some(PlatformNativeToolbarItem::ComboBox(item)) =
            state.item_for_identifier(identifier_string)
        else {
            return nil;
        };
        let combo_items: Vec<&str> = item.items.iter().map(|i| i.as_ref()).collect();
        let text = item.text.clone();
        let placeholder = item.placeholder.clone();
        let min_width = item.min_width;
        let max_width = item.max_width;

        let toolbar_item: id = msg_send![class!(NSToolbarItem), alloc];
        let toolbar_item: id = msg_send![toolbar_item, initWithItemIdentifier: identifier];

        let combo =
            crate::platform::native_controls::create_native_combo_box(&combo_items, 0, true);

        if !text.is_empty() {
            crate::platform::native_controls::set_native_combo_box_string_value(
                combo,
                text.as_ref(),
            );
        }

        let state_ptr: *mut c_void = *this.get_ivar(TOOLBAR_STATE_IVAR);
        let change_identifier = identifier_string.to_owned();
        let select_identifier = identifier_string.to_owned();

        let on_change = Some(Box::new(move |text: String| {
            let state = &*(state_ptr as *const ToolbarState);
            if let Some(PlatformNativeToolbarItem::ComboBox(combo_item)) =
                state.item_for_identifier(&change_identifier)
                && let Some(callback) = combo_item.on_change.as_ref()
            {
                callback(text);
            }
        }) as Box<dyn Fn(String)>);

        let on_select = Some(Box::new(move |selected_index: usize| {
            let state = &*(state_ptr as *const ToolbarState);
            if let Some(PlatformNativeToolbarItem::ComboBox(combo_item)) =
                state.item_for_identifier(&select_identifier)
                && let Some(callback) = combo_item.on_select.as_ref()
            {
                callback(selected_index);
            }
        }) as Box<dyn Fn(usize)>);

        let submit_identifier = identifier_string.to_owned();
        let on_submit = Some(Box::new(move |text: String| {
            let state = &*(state_ptr as *const ToolbarState);
            if let Some(PlatformNativeToolbarItem::ComboBox(combo_item)) =
                state.item_for_identifier(&submit_identifier)
                && let Some(callback) = combo_item.on_submit.as_ref()
            {
                callback(text);
            }
        }) as Box<dyn Fn(String)>);

        let combo_box_callbacks = crate::platform::native_controls::ComboBoxCallbacks {
            on_select,
            on_change,
            on_submit,
        };
        let combo_delegate =
            crate::platform::native_controls::set_native_combo_box_delegate(combo, combo_box_callbacks);

        if !placeholder.is_empty() {
            crate::platform::native_controls::set_native_text_field_placeholder(
                combo,
                placeholder.as_ref(),
            );
        }

        let frame: NSRect = msg_send![combo, frame];
        let min_size = NSSize {
            width: min_width.to_f64(),
            height: frame.size.height,
        };
        let max_size = NSSize {
            width: max_width.to_f64(),
            height: frame.size.height,
        };

        let _: () = msg_send![toolbar_item, setMinSize: min_size];
        let _: () = msg_send![toolbar_item, setMaxSize: max_size];
        let _: () = msg_send![toolbar_item, setView: combo];

        state
            .resources
            .push(ToolbarNativeResource::ComboBox { combo, delegate: combo_delegate });

        msg_send![toolbar_item, autorelease]
    }
}

fn convert_platform_menu_items_to_native(
    items: &[PlatformNativeToolbarMenuItemData],
) -> Vec<crate::platform::native_controls::NativeMenuItemData> {
    items
        .iter()
        .map(|item| match item {
            PlatformNativeToolbarMenuItemData::Action {
                title,
                enabled,
                icon,
            } => crate::platform::native_controls::NativeMenuItemData::Action {
                title: title.to_string(),
                enabled: *enabled,
                icon: icon.as_ref().map(|s| s.to_string()),
            },
            PlatformNativeToolbarMenuItemData::Submenu {
                title,
                enabled,
                icon,
                items,
            } => crate::platform::native_controls::NativeMenuItemData::Submenu {
                title: title.to_string(),
                enabled: *enabled,
                icon: icon.as_ref().map(|s| s.to_string()),
                items: convert_platform_menu_items_to_native(items),
            },
            PlatformNativeToolbarMenuItemData::Separator => {
                crate::platform::native_controls::NativeMenuItemData::Separator
            }
        })
        .collect()
}

unsafe fn create_toolbar_menu_button_item(
    this: &Object,
    state: &mut ToolbarState,
    identifier: id,
    identifier_string: &str,
) -> id {
    unsafe {
        let Some(PlatformNativeToolbarItem::MenuButton(item)) =
            state.item_for_identifier(identifier_string)
        else {
            return nil;
        };
        let label = item.label.clone();
        let tool_tip = item.tool_tip.clone();
        let icon = item.icon.clone();
        let image_url = item.image_url.clone();
        let image_circular = item.image_circular;
        let shows_indicator = item.shows_indicator;
        let native_menu_items = convert_platform_menu_items_to_native(&item.items);

        let toolbar_item: id = msg_send![class!(NSMenuToolbarItem), alloc];
        let toolbar_item: id = msg_send![toolbar_item, initWithItemIdentifier: identifier];

        let state_ptr: *mut c_void = *this.get_ivar(TOOLBAR_STATE_IVAR);
        let callback_identifier = identifier_string.to_owned();

        let on_select: Option<Box<dyn Fn(usize)>> = Some(Box::new(move |index: usize| {
            let state = &*(state_ptr as *const ToolbarState);
            if let Some(PlatformNativeToolbarItem::MenuButton(menu_item)) =
                state.item_for_identifier(&callback_identifier)
                && let Some(callback) = menu_item.on_select.as_ref()
            {
                callback(index);
            }
        }));

        let (menu, target_ptr) = crate::platform::native_controls::create_native_menu_target(
            &native_menu_items,
            on_select,
        );

        let _: () = msg_send![toolbar_item, setMenu: menu];
        // Release our extra retain - NSMenuToolbarItem retains the menu internally
        let _: () = msg_send![menu, release];

        let _: () = msg_send![toolbar_item, setLabel: ns_string(label.as_ref())];
        let _: () = msg_send![toolbar_item, setShowsIndicator: shows_indicator as BOOL];

        if let Some(tool_tip) = tool_tip.as_ref() {
            let _: () = msg_send![toolbar_item, setToolTip: ns_string(tool_tip.as_ref())];
        }

        if let Some(icon) = icon.as_ref() {
            let symbol_name = ns_string(icon.as_ref());
            let image: id = msg_send![class!(NSImage), imageWithSystemSymbolName: symbol_name accessibilityDescription: nil];
            if image != nil {
                let _: () = msg_send![toolbar_item, setImage: image];
            }
        }

        if let Some(url_str) = image_url.as_ref() {
            load_toolbar_image_from_url(toolbar_item, url_str, image_circular);
        }

        state
            .resources
            .push(ToolbarNativeResource::MenuButton { target: target_ptr });

        msg_send![toolbar_item, autorelease]
    }
}

unsafe fn create_toolbar_label_item(
    state: &mut ToolbarState,
    identifier: id,
    identifier_string: &str,
) -> id {
    unsafe {
        let Some(PlatformNativeToolbarItem::Label(item)) =
            state.item_for_identifier(identifier_string)
        else {
            return nil;
        };
        let text = item.text.clone();
        let icon = item.icon.clone();

        let toolbar_item: id = msg_send![class!(NSToolbarItem), alloc];
        let toolbar_item: id = msg_send![toolbar_item, initWithItemIdentifier: identifier];

        let label_string = ns_string(text.as_ref());
        let _: () = msg_send![toolbar_item, setLabel: label_string];

        if let Some(icon) = icon.as_ref() {
            let symbol_name = ns_string(icon.as_ref());
            let image: id = msg_send![class!(NSImage), imageWithSystemSymbolName: symbol_name accessibilityDescription: nil];
            if image != nil {
                let _: () = msg_send![toolbar_item, setImage: image];
            }
        }

        // Create NSTextField as a non-editable, borderless label
        let text_field: id = msg_send![class!(NSTextField), labelWithString: label_string];
        let _: () = msg_send![text_field, setEditable: NO];
        let _: () = msg_send![text_field, setBordered: NO];
        let _: () = msg_send![text_field, setDrawsBackground: NO];
        let _: () = msg_send![text_field, setSelectable: NO];

        // Use small system font to match toolbar style
        let small_font_size: f64 = msg_send![class!(NSFont), smallSystemFontSize];
        let font: id = msg_send![class!(NSFont), systemFontOfSize: small_font_size];
        let _: () = msg_send![text_field, setFont: font];

        // Use secondary label color for subtle appearance
        let secondary_color: id = msg_send![class!(NSColor), secondaryLabelColor];
        let _: () = msg_send![text_field, setTextColor: secondary_color];

        let size: NSSize = msg_send![text_field, fittingSize];
        let _: () = msg_send![toolbar_item, setMinSize: size];
        let _: () = msg_send![toolbar_item, setMaxSize: size];
        let _: () = msg_send![toolbar_item, setView: text_field];

        msg_send![toolbar_item, autorelease]
    }
}

extern "C" fn blurred_view_init_with_frame(this: &Object, _: Sel, frame: NSRect) -> id {
    unsafe {
        let view = msg_send![super(this, class!(NSVisualEffectView)), initWithFrame: frame];
        // Use a colorless semantic material. The default value `AppearanceBased`, though not
        // manually set, is deprecated.
        NSVisualEffectView::setMaterial_(view, NSVisualEffectMaterial::Selection);
        NSVisualEffectView::setState_(view, NSVisualEffectState::Active);
        view
    }
}

extern "C" fn blurred_view_update_layer(this: &Object, _: Sel) {
    unsafe {
        let _: () = msg_send![super(this, class!(NSVisualEffectView)), updateLayer];
        let layer: id = msg_send![this, layer];
        if !layer.is_null() {
            remove_layer_background(layer);
        }
    }
}

unsafe fn remove_layer_background(layer: id) {
    unsafe {
        let _: () = msg_send![layer, setBackgroundColor:nil];

        let class_name: id = msg_send![layer, className];
        if class_name.isEqualToString("CAChameleonLayer") {
            // Remove the desktop tinting effect.
            let _: () = msg_send![layer, setHidden: YES];
            return;
        }

        let filters: id = msg_send![layer, filters];
        if !filters.is_null() {
            // Remove the increased saturation.
            // The effect of a `CAFilter` or `CIFilter` is determined by its name, and the
            // `description` reflects its name and some parameters. Currently `NSVisualEffectView`
            // uses a `CAFilter` named "colorSaturate". If one day they switch to `CIFilter`, the
            // `description` will still contain "Saturat" ("... inputSaturation = ...").
            let test_string: id = ns_string("Saturat");
            let count = NSArray::count(filters);
            for i in 0..count {
                let description: id = msg_send![filters.objectAtIndex(i), description];
                let hit: BOOL = msg_send![description, containsString: test_string];
                if hit == NO {
                    continue;
                }

                let all_indices = NSRange {
                    location: 0,
                    length: count,
                };
                let indices: id = msg_send![class!(NSMutableIndexSet), indexSet];
                let _: () = msg_send![indices, addIndexesInRange: all_indices];
                let _: () = msg_send![indices, removeIndex:i];
                let filtered: id = msg_send![filters, objectsAtIndexes: indices];
                let _: () = msg_send![layer, setFilters: filtered];
                break;
            }
        }

        let sublayers: id = msg_send![layer, sublayers];
        if !sublayers.is_null() {
            let count = NSArray::count(sublayers);
            for i in 0..count {
                let sublayer = sublayers.objectAtIndex(i);
                remove_layer_background(sublayer);
            }
        }
    }
}

extern "C" fn add_titlebar_accessory_view_controller(this: &Object, _: Sel, view_controller: id) {
    unsafe {
        let _: () = msg_send![super(this, class!(NSWindow)), addTitlebarAccessoryViewController: view_controller];

        // Hide the native tab bar and set its height to 0, since we render our own.
        let accessory_view: id = msg_send![view_controller, view];
        let _: () = msg_send![accessory_view, setHidden: YES];
        let mut frame: NSRect = msg_send![accessory_view, frame];
        frame.size.height = 0.0;
        let _: () = msg_send![accessory_view, setFrame: frame];
    }
}

extern "C" fn move_tab_to_new_window(this: &Object, _: Sel, _: id) {
    unsafe {
        let _: () = msg_send![super(this, class!(NSWindow)), moveTabToNewWindow:nil];

        let window_state = get_window_state(this);
        let mut lock = window_state.as_ref().lock();
        if let Some(mut callback) = lock.move_tab_to_new_window_callback.take() {
            drop(lock);
            callback();
            window_state.lock().move_tab_to_new_window_callback = Some(callback);
        }
    }
}

extern "C" fn merge_all_windows(this: &Object, _: Sel, _: id) {
    unsafe {
        let _: () = msg_send![super(this, class!(NSWindow)), mergeAllWindows:nil];

        let window_state = get_window_state(this);
        let mut lock = window_state.as_ref().lock();
        if let Some(mut callback) = lock.merge_all_windows_callback.take() {
            drop(lock);
            callback();
            window_state.lock().merge_all_windows_callback = Some(callback);
        }
    }
}

extern "C" fn select_next_tab(this: &Object, _sel: Sel, _id: id) {
    let window_state = unsafe { get_window_state(this) };
    let mut lock = window_state.as_ref().lock();
    if let Some(mut callback) = lock.select_next_tab_callback.take() {
        drop(lock);
        callback();
        window_state.lock().select_next_tab_callback = Some(callback);
    }
}

extern "C" fn select_previous_tab(this: &Object, _sel: Sel, _id: id) {
    let window_state = unsafe { get_window_state(this) };
    let mut lock = window_state.as_ref().lock();
    if let Some(mut callback) = lock.select_previous_tab_callback.take() {
        drop(lock);
        callback();
        window_state.lock().select_previous_tab_callback = Some(callback);
    }
}

extern "C" fn toggle_tab_bar(this: &Object, _sel: Sel, _id: id) {
    unsafe {
        let _: () = msg_send![super(this, class!(NSWindow)), toggleTabBar:nil];

        let window_state = get_window_state(this);
        let mut lock = window_state.as_ref().lock();
        lock.move_traffic_light();

        if let Some(mut callback) = lock.toggle_tab_bar_callback.take() {
            drop(lock);
            callback();
            window_state.lock().toggle_tab_bar_callback = Some(callback);
        }
    }
}
