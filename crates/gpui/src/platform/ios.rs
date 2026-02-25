use crate::{
    Action, AnyWindowHandle, AtlasKey, AtlasTextureId, AtlasTextureKind, AtlasTile,
    BackgroundExecutor, Bounds, ClipboardItem, CursorStyle, DispatchEventResult, DisplayId,
    DummyKeyboardMapper, ForegroundExecutor, GLOBAL_THREAD_TIMINGS, GpuSpecs, Keymap, Menu,
    MenuItem, Modifiers, NoopTextSystem, OwnedMenu, PathPromptOptions, Pixels, Platform,
    PlatformAtlas, PlatformDispatcher, PlatformDisplay, PlatformInput, PlatformInputHandler,
    PlatformKeyboardLayout, PlatformKeyboardMapper, PlatformTextSystem, PlatformWindow, Point,
    Priority, PromptButton, RequestFrameOptions, Task, TaskTiming, ThermalState, THREAD_TIMINGS,
    ThreadTaskTimings, WindowAppearance, WindowBackgroundAppearance, WindowBounds, WindowControlArea,
    WindowParams, point, px, size,
};
use anyhow::{Result, anyhow};
use futures::channel::oneshot;
use objc::{
    class, msg_send,
    runtime::Object,
    sel, sel_impl,
};
use parking_lot::Mutex;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, UiKitWindowHandle, WindowHandle,
};
use std::{
    borrow::Cow,
    ffi::c_void,
    path::{Path, PathBuf},
    ptr::{NonNull, addr_of},
    rc::Rc,
    sync::Arc,
    thread,
    time::Duration,
};

pub(crate) type PlatformScreenCaptureFrame = ();

type DispatchQueue = *mut c_void;
type DispatchTime = u64;

const DISPATCH_TIME_NOW: DispatchTime = 0;
const DISPATCH_QUEUE_PRIORITY_HIGH: isize = 2;
const DISPATCH_QUEUE_PRIORITY_DEFAULT: isize = 0;
const DISPATCH_QUEUE_PRIORITY_LOW: isize = -2;

unsafe extern "C" {
    static _dispatch_main_q: c_void;
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
        // SAFETY: UIKit class messaging on main thread.
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
        Ok(uuid::Uuid::new_v4())
    }

    fn bounds(&self) -> Bounds<Pixels> {
        self.bounds
    }
}

pub(crate) struct IosDispatcher {
    main_thread: thread::ThreadId,
}

impl IosDispatcher {
    fn new() -> Self {
        Self {
            main_thread: thread::current().id(),
        }
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

extern "C" fn deferred_closure_trampoline(context: *mut c_void) {
    let closure: Box<Box<dyn FnOnce()>> =
        unsafe { Box::from_raw(context as *mut Box<dyn FnOnce()>) };
    closure();
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
        thread::current().id() == self.main_thread
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
                dispatch_get_global_queue(DISPATCH_QUEUE_PRIORITY_DEFAULT, 0),
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
        let dispatcher = Arc::new(IosDispatcher::new());
        let background_executor = BackgroundExecutor::new(dispatcher.clone());
        let foreground_executor = ForegroundExecutor::new(dispatcher);
        Self {
            state: Mutex::new(IosPlatformState {
                background_executor,
                foreground_executor,
                text_system: Arc::new(NoopTextSystem::new()),
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
        WindowAppearance::Light
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
        ThermalState::Nominal
    }

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

struct NullAtlas;

impl PlatformAtlas for NullAtlas {
    fn get_or_insert_with<'a>(
        &self,
        _key: &AtlasKey,
        build: &mut dyn FnMut() -> Result<Option<(crate::Size<crate::DevicePixels>, Cow<'a, [u8]>)>>,
    ) -> Result<Option<AtlasTile>> {
        let Some((size, _bytes)) = build()? else {
            return Ok(None);
        };
        Ok(Some(AtlasTile {
            texture_id: AtlasTextureId {
                index: 1,
                kind: AtlasTextureKind::Monochrome,
            },
            tile_id: crate::TileId(1),
            padding: 0,
            bounds: Bounds::new(crate::point(crate::DevicePixels(0), crate::DevicePixels(0)), size),
        }))
    }

    fn remove(&self, _key: &AtlasKey) {}
}

struct IosWindowState {
    handle: AnyWindowHandle,
    bounds: Bounds<Pixels>,
    display: Rc<dyn PlatformDisplay>,
    scale_factor: f32,
    ui_window: *mut Object,
    ui_view_controller: *mut Object,
    ui_view: *mut Object,
    sprite_atlas: Arc<dyn PlatformAtlas>,
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
        let (ui_window, ui_view_controller, ui_view, bounds, scale_factor) = unsafe {
            let screen: *mut Object = msg_send![class!(UIScreen), mainScreen];
            let screen_bounds: CGRect = msg_send![screen, bounds];
            let scale: f64 = msg_send![screen, scale];

            let ui_window: *mut Object = msg_send![class!(UIWindow), alloc];
            let ui_window: *mut Object = msg_send![ui_window, initWithFrame: screen_bounds];

            let ui_view_controller: *mut Object = msg_send![class!(UIViewController), new];
            let ui_view: *mut Object = msg_send![class!(UIView), alloc];
            let ui_view: *mut Object = msg_send![ui_view, initWithFrame: screen_bounds];

            let color: *mut Object = msg_send![
                class!(UIColor),
                colorWithRed: 0.07f64
                green: 0.10f64
                blue: 0.16f64
                alpha: 1.0f64
            ];
            let _: () = msg_send![ui_view, setBackgroundColor: color];

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
            (ui_window, ui_view_controller, ui_view, bounds, scale as f32)
        };

        Self(Rc::new(Mutex::new(IosWindowState {
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
            sprite_atlas: Arc::new(NullAtlas),
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
        })))
    }
}

impl Drop for IosWindow {
    fn drop(&mut self) {
        // SAFETY: Objective-C object references are owned by this state.
        unsafe {
            let mut state = self.0.lock();
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
        // SAFETY: pointers are held by this window for at least the borrowed lifetime.
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
        self.0.lock().bounds.size = size;
    }

    fn scale_factor(&self) -> f32 {
        self.0.lock().scale_factor
    }

    fn appearance(&self) -> WindowAppearance {
        WindowAppearance::Light
    }

    fn display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        Some(self.0.lock().display.clone())
    }

    fn mouse_position(&self) -> Point<Pixels> {
        Point::default()
    }

    fn modifiers(&self) -> Modifiers {
        Modifiers::default()
    }

    fn capslock(&self) -> crate::Capslock {
        crate::Capslock::default()
    }

    fn set_input_handler(&mut self, input_handler: PlatformInputHandler) {
        self.0.lock().input_handler = Some(input_handler);
    }

    fn take_input_handler(&mut self) -> Option<PlatformInputHandler> {
        self.0.lock().input_handler.take()
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
        true
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

        // Defer the first frame to the next run-loop iteration. Firing
        // synchronously here would re-enter `AppCell::borrow_mut()` because
        // we are still inside `Application::run()`'s mutable borrow.
        let state = self.0.clone();
        let trigger: Box<dyn FnOnce()> = Box::new(move || {
            if let Some(mut cb) = state.lock().request_frame.take() {
                cb(RequestFrameOptions {
                    require_presentation: true,
                    force_render: true,
                });
                state.lock().request_frame = Some(cb);
            }
        });
        let boxed: Box<Box<dyn FnOnce()>> = Box::new(trigger);
        let context = Box::into_raw(boxed) as *mut c_void;
        unsafe {
            dispatch_async_f(
                dispatch_get_main_queue_ptr(),
                context,
                Some(deferred_closure_trampoline),
            );
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

    fn draw(&self, _scene: &crate::Scene) {}

    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        self.0.lock().sprite_atlas.clone()
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
}
