#![cfg(target_os = "ios")]
use gpui::{
    App, ClipboardEntry, ClipboardItem, Context, ExternalPaths, FocusHandle,
    Focusable, Image, ImageFormat, KeyDownEvent, MouseButton, NativeImageSymbolWeight,
    PathPromptOptions, PinchEvent, RotationEvent, Window, WindowAppearance, WindowOptions, div,
    native_button, native_checkbox, native_image_view, native_progress_bar, native_slider,
    native_stepper, native_switch, native_text_field, native_toggle_group, prelude::*, px, rgb,
    rgba,
};
use log::LevelFilter;
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static STARTED: AtomicBool = AtomicBool::new(false);

const IOS_CLIPBOARD_TEST_PNG: [u8; 68] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00, 0xb5, 0x1c, 0x0c,
    0x02, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xfc, 0xff, 0x1f, 0x00,
    0x03, 0x03, 0x02, 0x00, 0xef, 0x97, 0xd9, 0x77, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44,
    0xae, 0x42, 0x60, 0x82,
];

// ---------------------------------------------------------------------------
// iOS Logger — os_log + stderr + TCP relay
// ---------------------------------------------------------------------------

struct TcpSinkState {
    stream: Option<TcpStream>,
    last_reconnect_attempt_ms: u64,
}

static TCP_SINK: Mutex<Option<TcpSinkState>> = Mutex::new(None);

/// Relay address parsed once at init, reused for reconnection.
static RELAY_ADDR: Mutex<Option<SocketAddr>> = Mutex::new(None);

/// Cooldown between TCP reconnection attempts (5 seconds).
const TCP_RECONNECT_COOLDOWN_MS: u64 = 5_000;

struct IosLogger {
    subsystem: String,
}

impl IosLogger {
    fn new(subsystem: &str) -> Self {
        Self {
            subsystem: subsystem.to_string(),
        }
    }

    fn level_color(level: log::Level) -> &'static str {
        match level {
            log::Level::Error => "\x1b[31m",
            log::Level::Warn => "\x1b[33m",
            log::Level::Info => "\x1b[32m",
            log::Level::Debug => "\x1b[36m",
            log::Level::Trace => "\x1b[90m",
        }
    }

    fn level_tag(level: log::Level) -> &'static str {
        match level {
            log::Level::Error => "ERROR",
            log::Level::Warn => "WARN ",
            log::Level::Info => "INFO ",
            log::Level::Debug => "DEBUG",
            log::Level::Trace => "TRACE",
        }
    }

    fn timestamp() -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let total_secs = now.as_secs();
        let millis = now.subsec_millis();
        let hours = (total_secs / 3600) % 24;
        let minutes = (total_secs / 60) % 60;
        let seconds = total_secs % 60;
        format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, seconds, millis)
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

impl log::Log for IosLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let message = format!("{}", record.args());
        let ts = Self::timestamp();

        let os_log = oslog::OsLog::new(&self.subsystem, record.target());
        os_log.with_level(record.level().into(), &message);

        let color = Self::level_color(record.level());
        let reset = "\x1b[0m";
        let tag = Self::level_tag(record.level());
        let mut stderr = std::io::stderr().lock();
        let _ = writeln!(
            stderr,
            "{ts} {color}{tag}{reset} [{}] {}",
            record.target(),
            message,
        );
        let _ = stderr.flush();

        if let Ok(mut guard) = TCP_SINK.lock() {
            if let Some(ref mut sink) = *guard {
                let line = format!(
                    "{ts} {color}{tag}{reset} [{}] {}\n",
                    record.target(),
                    message,
                );
                if let Some(ref mut stream) = sink.stream {
                    if stream.write_all(line.as_bytes()).is_err() {
                        sink.stream = None;
                        // Try reconnect if cooldown has passed
                        try_reconnect_tcp(sink);
                    }
                } else {
                    try_reconnect_tcp(sink);
                    // Retry write after reconnect
                    if let Some(ref mut stream) = sink.stream {
                        let _ = stream.write_all(line.as_bytes());
                    }
                }
            }
        }
    }

    fn flush(&self) {
        let _ = std::io::stderr().flush();
        if let Ok(mut guard) = TCP_SINK.lock() {
            if let Some(ref mut sink) = *guard {
                if let Some(ref mut stream) = sink.stream {
                    let _ = stream.flush();
                }
            }
        }
    }
}

fn try_reconnect_tcp(sink: &mut TcpSinkState) {
    let now = IosLogger::now_ms();
    if now.saturating_sub(sink.last_reconnect_attempt_ms) < TCP_RECONNECT_COOLDOWN_MS {
        return;
    }
    sink.last_reconnect_attempt_ms = now;

    let addr = match RELAY_ADDR.lock() {
        Ok(guard) => match *guard {
            Some(a) => a,
            None => return,
        },
        Err(_) => return,
    };

    if let Ok(stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(200)) {
        let _ = stream.set_nodelay(true);
        sink.stream = Some(stream);
    }
}

fn try_connect_log_relay() {
    let addr = match option_env!("GPUI_LOG_RELAY") {
        Some(a) if !a.is_empty() => a,
        _ => return,
    };

    let sock_addr = match addr.parse::<SocketAddr>() {
        Ok(a) => a,
        Err(_) => return,
    };

    // Store for reconnection
    if let Ok(mut guard) = RELAY_ADDR.lock() {
        *guard = Some(sock_addr);
    }

    match TcpStream::connect_timeout(&sock_addr, Duration::from_secs(2)) {
        Ok(stream) => {
            let _ = stream.set_nodelay(true);
            *TCP_SINK.lock().unwrap() = Some(TcpSinkState {
                stream: Some(stream),
                last_reconnect_attempt_ms: 0,
            });
        }
        Err(_) => {
            *TCP_SINK.lock().unwrap() = Some(TcpSinkState {
                stream: None,
                last_reconnect_attempt_ms: 0,
            });
        }
    }
}

fn init_logging(subsystem: &str) {
    try_connect_log_relay();
    let logger = IosLogger::new(subsystem);
    log::set_boxed_logger(Box::new(logger)).expect("failed to set logger");

    let level = match option_env!("GPUI_LOG_LEVEL") {
        Some("trace") | Some("TRACE") => LevelFilter::Trace,
        Some("debug") | Some("DEBUG") => LevelFilter::Debug,
        Some("info") | Some("INFO") => LevelFilter::Info,
        Some("warn") | Some("WARN") => LevelFilter::Warn,
        Some("error") | Some("ERROR") => LevelFilter::Error,
        _ => LevelFilter::Debug,
    };
    log::set_max_level(level);
}

fn run_ios_app<V: Render + 'static>(
    subsystem: &str,
    build_view: impl FnOnce(&mut Window, &mut Context<V>) -> V + 'static,
) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }

    init_logging(subsystem);

    std::panic::set_hook(Box::new(|info| {
        log::error!("[GPUI-iOS] PANIC: {}", info);
        let home = std::env::var("HOME").unwrap_or_default();
        let path = format!("{}/Documents/gpui_panic.log", home);
        let _ = std::fs::write(&path, format!("{}", info));
    }));

    log::info!("[GPUI-iOS] launching app ({subsystem})");

    let app = gpui_platform::application();
    let keepalive = app.clone();
    let _ = Box::leak(Box::new(keepalive));

    app.run(move |cx: &mut App| {
        cx.open_window(WindowOptions::default(), |window, cx| {
            cx.new(|cx| build_view(window, cx))
        })
        .expect("failed to open GPUI iOS window");
        cx.activate(true);
    });
}

// ---------------------------------------------------------------------------
// 1. Hello World — original colored boxes demo
// ---------------------------------------------------------------------------

struct IosHelloWorld;

impl Render for IosHelloWorld {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let safe = window.safe_area_insets();
        div()
            .size_full()
            .pt(safe.top)
            .pb(safe.bottom)
            .pl(safe.left)
            .pr(safe.right)
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(20.0))
            .bg(rgb(0x1e1e2e))
            .child(
                div()
                    .w(px(200.0))
                    .h(px(80.0))
                    .bg(rgb(0xf38ba8))
                    .rounded(px(12.0)),
            )
            .child(
                div()
                    .w(px(200.0))
                    .h(px(80.0))
                    .bg(rgb(0xa6e3a1))
                    .rounded(px(12.0)),
            )
            .child(
                div()
                    .w(px(200.0))
                    .h(px(80.0))
                    .bg(rgb(0x89b4fa))
                    .rounded(px(12.0)),
            )
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_run_hello_world() {
    run_ios_app("dev.glasshq.GPUIiOSHello", |_, _| IosHelloWorld);
}

// ---------------------------------------------------------------------------
// 2. Touch Input Demo — tappable boxes that change color on tap
// ---------------------------------------------------------------------------

struct IosTouchDemo {
    tapped_box: Option<usize>,
    tap_count: usize,
}

impl Render for IosTouchDemo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let safe = window.safe_area_insets();
        let tapped = self.tapped_box;
        let tap_count = self.tap_count;

        let box_color = |index: usize, base: u32, active: u32| -> u32 {
            if tapped == Some(index) { active } else { base }
        };

        div()
            .size_full()
            .pt(safe.top)
            .pb(safe.bottom)
            .pl(safe.left)
            .pr(safe.right)
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(20.0))
            .bg(rgb(0x1e1e2e))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(280.0))
                    .h(px(40.0))
                    .bg(rgb(0x313244))
                    .rounded(px(8.0))
                    .child(format!("Tap a box! (taps: {})", tap_count)),
            )
            .child(
                div()
                    .id("box-0")
                    .w(px(200.0))
                    .h(px(80.0))
                    .bg(rgb(box_color(0, 0xf38ba8, 0xff5577)))
                    .rounded(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("Red")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            log::info!("touch: red box tapped");
                            this.tapped_box = Some(0);
                            this.tap_count += 1;
                            cx.notify();
                        }),
                    ),
            )
            .child(
                div()
                    .id("box-1")
                    .w(px(200.0))
                    .h(px(80.0))
                    .bg(rgb(box_color(1, 0xa6e3a1, 0x55ff77)))
                    .rounded(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("Green")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            log::info!("touch: green box tapped");
                            this.tapped_box = Some(1);
                            this.tap_count += 1;
                            cx.notify();
                        }),
                    ),
            )
            .child(
                div()
                    .id("box-2")
                    .w(px(200.0))
                    .h(px(80.0))
                    .bg(rgb(box_color(2, 0x89b4fa, 0x5577ff)))
                    .rounded(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("Blue")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            log::info!("touch: blue box tapped");
                            this.tapped_box = Some(2);
                            this.tap_count += 1;
                            cx.notify();
                        }),
                    ),
            )
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_run_touch_demo() {
    run_ios_app("dev.glasshq.GPUIiOSTouchDemo", |_, _| IosTouchDemo {
        tapped_box: None,
        tap_count: 0,
    });
}

// ---------------------------------------------------------------------------
// 3. Text Rendering Demo — text at various sizes
// ---------------------------------------------------------------------------

struct IosTextDemo;

impl Render for IosTextDemo {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let safe = window.safe_area_insets();
        div()
            .size_full()
            .pt(safe.top)
            .pb(safe.bottom)
            .pl(safe.left)
            .pr(safe.right)
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(16.0))
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .child(div().text_size(px(32.0)).child("Hello iOS!"))
            .child(div().text_size(px(20.0)).child("CoreText text rendering"))
            .child(
                div()
                    .text_size(px(16.0))
                    .text_color(rgb(0xa6adc8))
                    .child("The quick brown fox jumps over the lazy dog"),
            )
            .child(
                div()
                    .text_size(px(14.0))
                    .text_color(rgb(0x6c7086))
                    .child("ABCDEFGHIJKLMNOPQRSTUVWXYZ"),
            )
            .child(
                div()
                    .text_size(px(14.0))
                    .text_color(rgb(0x6c7086))
                    .child("abcdefghijklmnopqrstuvwxyz"),
            )
            .child(
                div()
                    .text_size(px(14.0))
                    .text_color(rgb(0x6c7086))
                    .child("0123456789 !@#$%^&*()"),
            )
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_run_text_demo() {
    run_ios_app("dev.glasshq.GPUIiOSTextDemo", |_, _| IosTextDemo);
}

// ---------------------------------------------------------------------------
// 4. Window Lifecycle Demo — shows active state, appearance, and size
// ---------------------------------------------------------------------------

struct IosLifecycleDemo {
    resize_count: usize,
}

impl Render for IosLifecycleDemo {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let safe = window.safe_area_insets();
        let bounds = window.bounds();
        let appearance = window.appearance();
        let scale = window.scale_factor();

        let appearance_name = format!("{:?}", appearance);
        let size_text = format!(
            "{:.0}x{:.0} @{:.0}x",
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
            scale,
        );

        div()
            .size_full()
            .pt(safe.top)
            .pb(safe.bottom)
            .pl(safe.left)
            .pr(safe.right)
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(16.0))
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .child(div().text_size(px(24.0)).child("Window Lifecycle"))
            .child(
                div()
                    .w(px(300.0))
                    .p(px(16.0))
                    .bg(rgb(0x313244))
                    .rounded(px(12.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(16.0))
                            .child(format!("Appearance: {}", appearance_name)),
                    )
                    .child(
                        div()
                            .text_size(px(16.0))
                            .child(format!("Size: {}", size_text)),
                    )
                    .child(
                        div()
                            .text_size(px(16.0))
                            .child(format!("Resizes: {}", self.resize_count)),
                    )
                    .child(
                        div()
                            .text_size(px(14.0))
                            .text_color(rgb(0x6c7086))
                            .child("Rotate device or toggle dark mode to see changes"),
                    ),
            )
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_run_lifecycle_demo() {
    run_ios_app("dev.glasshq.GPUIiOSLifecycleDemo", |_, _| {
        IosLifecycleDemo { resize_count: 0 }
    });
}

// ---------------------------------------------------------------------------
// 5. Combined Demo — touch + text + lifecycle info in one view
// ---------------------------------------------------------------------------

struct IosCombinedDemo {
    tap_count: usize,
    last_tapped: Option<&'static str>,
}

impl Render for IosCombinedDemo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let safe = window.safe_area_insets();
        let bounds = window.bounds();
        let appearance = window.appearance();
        let scale = window.scale_factor();
        let tap_count = self.tap_count;
        let last_tapped = self.last_tapped.unwrap_or("none");

        let is_dark = matches!(
            appearance,
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let bg_color = if is_dark {
            rgb(0x1e1e2e)
        } else {
            rgb(0xeff1f5)
        };
        let text_color = if is_dark {
            rgb(0xcdd6f4)
        } else {
            rgb(0x4c4f69)
        };
        let panel_bg = if is_dark {
            rgb(0x313244)
        } else {
            rgb(0xccd0da)
        };
        let muted_text = if is_dark {
            rgb(0x6c7086)
        } else {
            rgb(0x9ca0b0)
        };

        div()
            .size_full()
            .pt(safe.top)
            .pb(safe.bottom)
            .pl(safe.left)
            .pr(safe.right)
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(12.0))
            .bg(bg_color)
            .text_color(text_color)
            // Title
            .child(div().text_size(px(28.0)).child("GPUI on iOS"))
            // Info panel
            .child(
                div()
                    .w(px(300.0))
                    .p(px(12.0))
                    .bg(panel_bg)
                    .rounded(px(8.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(div().text_size(px(14.0)).child(format!(
                        "{:.0}x{:.0} @{:.0}x  {:?}",
                        f32::from(bounds.size.width),
                        f32::from(bounds.size.height),
                        scale,
                        appearance,
                    )))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .child(format!("Taps: {}  Last: {}", tap_count, last_tapped)),
                    ),
            )
            // Tappable boxes
            .child(
                div()
                    .id("red")
                    .w(px(200.0))
                    .h(px(60.0))
                    .bg(rgb(0xf38ba8))
                    .rounded(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("Tap me")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.tap_count += 1;
                            this.last_tapped = Some("red");
                            cx.notify();
                        }),
                    ),
            )
            .child(
                div()
                    .id("green")
                    .w(px(200.0))
                    .h(px(60.0))
                    .bg(rgb(0xa6e3a1))
                    .rounded(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(0x1e1e2e))
                    .child("Tap me")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.tap_count += 1;
                            this.last_tapped = Some("green");
                            cx.notify();
                        }),
                    ),
            )
            .child(
                div()
                    .id("blue")
                    .w(px(200.0))
                    .h(px(60.0))
                    .bg(rgb(0x89b4fa))
                    .rounded(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(0x1e1e2e))
                    .child("Tap me")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.tap_count += 1;
                            this.last_tapped = Some("blue");
                            cx.notify();
                        }),
                    ),
            )
            // Text samples
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(muted_text)
                    .child("The quick brown fox jumps over the lazy dog"),
            )
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_run_combined_demo() {
    run_ios_app("dev.glasshq.GPUIiOSCombinedDemo", |_, _| IosCombinedDemo {
        tap_count: 0,
        last_tapped: None,
    });
}

// ---------------------------------------------------------------------------
// 6. Scroll Demo — two-finger pan scrollable list
// ---------------------------------------------------------------------------

struct IosScrollDemo;

impl Render for IosScrollDemo {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let safe = window.safe_area_insets();
        let colors = [
            0xf38ba8u32,
            0xa6e3a1,
            0x89b4fa,
            0xfab387,
            0xcba6f7,
            0xf9e2af,
            0x94e2d5,
            0xf2cdcd,
            0x89dceb,
            0xb4befe,
        ];

        let mut scroll_content = div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(16.0))
            .pb(safe.bottom);

        for i in 0..50 {
            let color = colors[i % colors.len()];
            scroll_content = scroll_content.child(
                div()
                    .w_full()
                    .h(px(60.0))
                    .bg(rgb(color))
                    .rounded(px(8.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(0x1e1e2e))
                    .child(format!("Item {}", i + 1)),
            );
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .pl(safe.left)
            .pr(safe.right)
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .child(
                div()
                    .w_full()
                    .pt(safe.top)
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_end()
                    .bg(rgb(0x313244))
                    .child(
                        div()
                            .h(px(60.0))
                            .flex()
                            .items_center()
                            .text_size(px(20.0))
                            .child("Scroll Demo (2-finger pan)"),
                    ),
            )
            .child(
                div()
                    .id("scroll-container")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(scroll_content),
            )
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_run_scroll_demo() {
    run_ios_app("dev.glasshq.GPUIiOSScrollDemo", |_, _| IosScrollDemo);
}

// ---------------------------------------------------------------------------
// 7. Text Input Demo — tap to focus, type text via software keyboard
// ---------------------------------------------------------------------------

struct IosTextInputDemo {
    focus_handle: FocusHandle,
    text: String,
}

impl Focusable for IosTextInputDemo {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for IosTextInputDemo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let safe = window.safe_area_insets();
        let text = self.text.clone();
        let focused = self.focus_handle.is_focused(window);

        let border_color = if focused { 0x89b4fau32 } else { 0x585b70 };
        let display_text = if text.is_empty() && !focused {
            "Tap here to type...".to_string()
        } else if text.is_empty() {
            "|".to_string()
        } else {
            format!("{}|", text)
        };

        div()
            .id("text-input-root")
            .track_focus(&self.focus_handle)
            .size_full()
            .pt(safe.top)
            .pb(safe.bottom)
            .pl(safe.left)
            .pr(safe.right)
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(20.0))
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .on_key_down(cx.listener(|this: &mut Self, event: &KeyDownEvent, _, cx| {
                let key = &event.keystroke.key;
                if key == "backspace" {
                    this.text.pop();
                    cx.notify();
                } else if key == "enter" {
                    log::info!("submitted: {:?}", this.text);
                } else if let Some(ch) = &event.keystroke.key_char {
                    this.text.push_str(ch);
                    cx.notify();
                }
            }))
            .child(div().text_size(px(24.0)).child("Text Input Demo"))
            .child(
                div()
                    .id("text-field")
                    .w(px(300.0))
                    .h(px(44.0))
                    .px(px(12.0))
                    .bg(rgb(0x313244))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(rgb(border_color))
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(16.0))
                            .text_color(if self.text.is_empty() && !focused {
                                rgb(0x6c7086)
                            } else {
                                rgb(0xcdd6f4)
                            })
                            .child(display_text),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_this, _, window, cx| {
                            cx.focus_self(window);
                            cx.notify();
                        }),
                    ),
            )
            .child(
                div()
                    .text_size(px(14.0))
                    .text_color(rgb(0x6c7086))
                    .child("Tap the input field, then type"),
            )
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_run_text_input_demo() {
    run_ios_app("dev.glasshq.GPUIiOSTextInputDemo", |_window, cx| {
        let focus_handle = cx.focus_handle();
        IosTextInputDemo {
            focus_handle,
            text: String::new(),
        }
    });
}

// ---------------------------------------------------------------------------
// 8. Vertical Scroll Demo — single-finger scrollable list with momentum
// ---------------------------------------------------------------------------

struct IosVerticalScrollDemo;

impl Render for IosVerticalScrollDemo {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let safe = window.safe_area_insets();
        let colors = [
            0xf38ba8u32,
            0xa6e3a1,
            0x89b4fa,
            0xfab387,
            0xcba6f7,
            0xf9e2af,
            0x94e2d5,
            0xf2cdcd,
            0x89dceb,
            0xb4befe,
        ];

        let mut list = div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(16.0))
            .pb(safe.bottom);

        for i in 0..100 {
            let color = colors[i % colors.len()];
            list = list.child(
                div()
                    .w_full()
                    .h(px(56.0))
                    .bg(rgb(color))
                    .rounded(px(8.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(0x1e1e2e))
                    .child(format!("Row {}", i + 1)),
            );
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .pl(safe.left)
            .pr(safe.right)
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .child(
                div()
                    .w_full()
                    .pt(safe.top)
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_end()
                    .bg(rgb(0x313244))
                    .child(
                        div()
                            .h(px(60.0))
                            .flex()
                            .items_center()
                            .text_size(px(20.0))
                            .child("Vertical Scroll (1-finger)"),
                    ),
            )
            .child(div().id("vscroll").flex_1().overflow_y_scroll().child(list))
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_run_vertical_scroll_demo() {
    run_ios_app("dev.glasshq.GPUIiOSVerticalScrollDemo", |_, _| {
        IosVerticalScrollDemo
    });
}

// ---------------------------------------------------------------------------
// 9. Horizontal Scroll Demo — single-finger horizontal scroll
// ---------------------------------------------------------------------------

struct IosHorizontalScrollDemo;

impl Render for IosHorizontalScrollDemo {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let safe = window.safe_area_insets();
        let colors = [
            0xf38ba8u32,
            0xa6e3a1,
            0x89b4fa,
            0xfab387,
            0xcba6f7,
            0xf9e2af,
            0x94e2d5,
            0xf2cdcd,
            0x89dceb,
            0xb4befe,
        ];

        let card_count = 30;
        let card_w = 140.0;
        let gap = 12.0;
        let pad = 16.0;
        let total_w = (card_count as f32) * card_w + ((card_count - 1) as f32) * gap + pad * 2.0;

        let mut strip = div()
            .flex()
            .flex_row()
            .gap(px(gap))
            .p(px(pad))
            .min_w(px(total_w));

        for i in 0..card_count {
            let color = colors[i % colors.len()];
            strip = strip.child(
                div()
                    .w(px(140.0))
                    .h(px(180.0))
                    .flex_shrink_0()
                    .bg(rgb(color))
                    .rounded(px(12.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(8.0))
                    .text_color(rgb(0x1e1e2e))
                    .child(div().text_size(px(24.0)).child(format!("{}", i + 1)))
                    .child(div().text_size(px(14.0)).child(format!("Card {}", i + 1))),
            );
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .child(
                div()
                    .w_full()
                    .pt(safe.top)
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_end()
                    .bg(rgb(0x313244))
                    .child(
                        div()
                            .h(px(60.0))
                            .flex()
                            .items_center()
                            .text_size(px(20.0))
                            .child("Horizontal Scroll (1-finger)"),
                    ),
            )
            .child(
                div().flex_1().pb(safe.bottom).flex().items_center().child(
                    div()
                        .id("hscroll")
                        .w_full()
                        .h(px(220.0))
                        .overflow_x_scroll()
                        .child(strip),
                ),
            )
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_run_horizontal_scroll_demo() {
    run_ios_app("dev.glasshq.GPUIiOSHorizontalScrollDemo", |_, _| {
        IosHorizontalScrollDemo
    });
}

// ---------------------------------------------------------------------------
// 10. Pinch Gesture Demo — pinch to scale a colored square
// ---------------------------------------------------------------------------

struct IosPinchDemo {
    scale: f32,
}

impl Render for IosPinchDemo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let safe = window.safe_area_insets();
        let scale = self.scale;
        let size = 120.0 * scale;

        div()
            .id("pinch-root")
            .size_full()
            .pt(safe.top)
            .pb(safe.bottom)
            .pl(safe.left)
            .pr(safe.right)
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(24.0))
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .on_pinch(cx.listener(|this: &mut Self, event: &PinchEvent, _, cx| {
                this.scale *= event.scale;
                this.scale = this.scale.clamp(0.25, 5.0);
                cx.notify();
            }))
            .child(div().text_size(px(24.0)).child("Pinch to Scale"))
            .child(
                div()
                    .w(px(size))
                    .h(px(size))
                    .bg(rgb(0xcba6f7))
                    .rounded(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(0x1e1e2e))
                    .child(div().text_size(px(16.0)).child(format!("{:.1}x", scale))),
            )
            .child(
                div()
                    .text_size(px(14.0))
                    .text_color(rgb(0x6c7086))
                    .child("Use two fingers to pinch in/out"),
            )
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_run_pinch_demo() {
    run_ios_app("dev.glasshq.GPUIiOSPinchDemo", |_, _| IosPinchDemo {
        scale: 1.0,
    });
}

// ---------------------------------------------------------------------------
// 11. Rotation Gesture Demo — two-finger rotate a colored rectangle
// ---------------------------------------------------------------------------

struct IosRotationDemo {
    angle_rad: f32,
}

impl Render for IosRotationDemo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let safe = window.safe_area_insets();
        let angle_deg = self.angle_rad.to_degrees();

        // Map angle to a hue shift for visual feedback
        let hue = ((angle_deg % 360.0 + 360.0) % 360.0) / 360.0;
        let (r, g, b) = hsv_to_rgb(hue, 0.6, 0.95);
        let box_color = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);

        div()
            .id("rotation-root")
            .size_full()
            .pt(safe.top)
            .pb(safe.bottom)
            .pl(safe.left)
            .pr(safe.right)
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(24.0))
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .on_rotation(
                cx.listener(|this: &mut Self, event: &RotationEvent, _, cx| {
                    this.angle_rad += event.rotation;
                    cx.notify();
                }),
            )
            .child(div().text_size(px(24.0)).child("Two-Finger Rotate"))
            .child(
                div()
                    .w(px(160.0))
                    .h(px(100.0))
                    .bg(rgb(box_color))
                    .rounded(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(0x1e1e2e))
                    .child(
                        div()
                            .text_size(px(20.0))
                            .child(format!("{:.1}\u{00b0}", angle_deg)),
                    ),
            )
            .child(
                div()
                    .text_size(px(14.0))
                    .text_color(rgb(0x6c7086))
                    .child("Color shifts as you rotate"),
            )
    }
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h * 6.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_run_rotation_demo() {
    run_ios_app("dev.glasshq.GPUIiOSRotationDemo", |_, _| IosRotationDemo {
        angle_rad: 0.0,
    });
}

// ---------------------------------------------------------------------------
// 12. Controls Demo — GPU-painted GPUI controls on iOS
// ---------------------------------------------------------------------------

struct IosControlsDemo {
    focus_handle: FocusHandle,
    button_tap_count: usize,
    switch_on: bool,
    checkbox_checked: bool,
    slider_value: f32,
    stepper_value: i32,
    text_field_value: String,
    selected_segment: usize,
}

impl Focusable for IosControlsDemo {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for IosControlsDemo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let safe = window.safe_area_insets();
        let button_tap_count = self.button_tap_count;
        let switch_on = self.switch_on;
        let checkbox_checked = self.checkbox_checked;
        let slider_value = self.slider_value;
        let stepper_value = self.stepper_value;
        let text_field_value = self.text_field_value.clone();
        let selected_segment = self.selected_segment;

        let slider_percent = (slider_value * 100.0).round() as i32;
        let progress_value = slider_value;
        let focused = self.focus_handle.is_focused(window);

        fn row(label: &str, control: impl IntoElement) -> gpui::Div {
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .w_full()
                .gap(px(12.0))
                .child(
                    div()
                        .text_size(px(15.0))
                        .text_color(rgb(0xcdd6f4))
                        .flex_shrink_0()
                        .child(label.to_string()),
                )
                .child(control)
        }

        let section = |title: &str| {
            div().w_full().pt(px(16.0)).pb(px(4.0)).child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(0x6c7086))
                    .child(title.to_string()),
            )
        };

        let mut slider_ticks = div().flex().flex_row().gap(px(2.0));
        for i in 0..=20 {
            let tick_value = i as f32 / 20.0;
            let active = tick_value <= slider_value + 0.0001;
            let tick_color = if active { 0x89b4fa } else { 0x45475a };
            slider_ticks = slider_ticks.child(
                div()
                    .w(px(8.0))
                    .h(px(18.0))
                    .rounded(px(2.0))
                    .bg(rgb(tick_color))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.slider_value = tick_value;
                            cx.notify();
                        }),
                    ),
            );
        }

        let text_border_color = if focused { 0x89b4fa } else { 0x585b70 };
        let text_display = if text_field_value.is_empty() && !focused {
            "Tap here to type...".to_string()
        } else if text_field_value.is_empty() {
            "|".to_string()
        } else {
            format!("{}|", text_field_value)
        };

        let mut toggle_group = div()
            .flex()
            .flex_row()
            .rounded(px(8.0))
            .border_1()
            .border_color(rgb(0x585b70))
            .overflow_hidden();

        for (index, label) in ["One", "Two", "Three"].iter().enumerate() {
            let is_selected = selected_segment == index;
            let bg = if is_selected { 0x89b4fa } else { 0x313244 };
            let fg = if is_selected { 0x1e1e2e } else { 0xcdd6f4 };
            toggle_group = toggle_group.child(
                div()
                    .px(px(12.0))
                    .py(px(6.0))
                    .bg(rgb(bg))
                    .text_color(rgb(fg))
                    .child((*label).to_string())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.selected_segment = index;
                            cx.notify();
                        }),
                    ),
            );
        }

        let mut content = div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .p(px(20.0))
            .pb(safe.bottom)
            .w_full();

        content = content.child(
            div()
                .text_size(px(24.0))
                .text_color(rgb(0xcdd6f4))
                .pb(px(8.0))
                .child("Controls"),
        );

        content = content.child(section("BUTTON")).child(row(
            &format!("Taps: {}", button_tap_count),
            div()
                .px(px(12.0))
                .py(px(8.0))
                .rounded(px(8.0))
                .bg(rgb(0x89b4fa))
                .text_color(rgb(0x1e1e2e))
                .child("Tap Me")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.button_tap_count += 1;
                        cx.notify();
                    }),
                ),
        ));

        content = content.child(section("SWITCH")).child(row(
            &format!("Switch: {}", if switch_on { "ON" } else { "OFF" }),
            div()
                .w(px(52.0))
                .h(px(30.0))
                .rounded(px(15.0))
                .bg(rgb(if switch_on { 0xa6e3a1 } else { 0x585b70 }))
                .flex()
                .items_center()
                .justify_start()
                .child(
                    div()
                        .w(px(22.0))
                        .h(px(22.0))
                        .ml(if switch_on { px(27.0) } else { px(3.0) })
                        .rounded(px(11.0))
                        .bg(rgb(0xf5e0dc)),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.switch_on = !this.switch_on;
                        cx.notify();
                    }),
                ),
        ));

        content = content.child(section("CHECKBOX")).child(row(
            &format!("Checked: {}", checkbox_checked),
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .w(px(20.0))
                        .h(px(20.0))
                        .rounded(px(4.0))
                        .border_1()
                        .border_color(rgb(0x89b4fa))
                        .bg(rgb(if checkbox_checked { 0x89b4fa } else { 0x1e1e2e }))
                        .text_color(rgb(0x1e1e2e))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(if checkbox_checked { "✓" } else { "" }),
                )
                .child("Enable feature")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.checkbox_checked = !this.checkbox_checked;
                        cx.notify();
                    }),
                ),
        ));

        content = content.child(section("SLIDER")).child(row(
            &format!("Value: {}%", slider_percent),
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(slider_ticks)
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(0x6c7086))
                        .child("Tap ticks to adjust"),
                ),
        ));

        content = content.child(section("STEPPER")).child(row(
            &format!("Count: {}", stepper_value),
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .w(px(28.0))
                        .h(px(28.0))
                        .rounded(px(6.0))
                        .bg(rgb(0x313244))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child("-")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                this.stepper_value = (this.stepper_value - 1).clamp(0, 20);
                                cx.notify();
                            }),
                        ),
                )
                .child(
                    div()
                        .w(px(48.0))
                        .text_center()
                        .child(stepper_value.to_string()),
                )
                .child(
                    div()
                        .w(px(28.0))
                        .h(px(28.0))
                        .rounded(px(6.0))
                        .bg(rgb(0x313244))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child("+")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                this.stepper_value = (this.stepper_value + 1).clamp(0, 20);
                                cx.notify();
                            }),
                        ),
                ),
        ));

        content = content.child(section("TEXT FIELD")).child(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .w_full()
                        .h(px(40.0))
                        .px(px(10.0))
                        .bg(rgb(0x313244))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(rgb(text_border_color))
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .text_size(px(14.0))
                                .text_color(if text_field_value.is_empty() && !focused {
                                    rgb(0x6c7086)
                                } else {
                                    rgb(0xcdd6f4)
                                })
                                .child(text_display),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|_this, _, window, cx| {
                                cx.focus_self(window);
                                cx.notify();
                            }),
                        ),
                )
                .child(div().text_size(px(12.0)).text_color(rgb(0x6c7086)).child(
                    if text_field_value.is_empty() {
                        "No text entered".to_string()
                    } else {
                        format!("Text: {}", text_field_value)
                    },
                )),
        );

        content = content.child(section("PROGRESS BAR")).child(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .w_full()
                        .h(px(12.0))
                        .rounded(px(6.0))
                        .bg(rgb(0x45475a))
                        .child(
                            div()
                                .w(px(progress_value * 260.0))
                                .h(px(12.0))
                                .rounded(px(6.0))
                                .bg(rgb(0xa6e3a1)),
                        ),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(0x6c7086))
                        .child(format!("{}% (driven by slider)", slider_percent)),
                ),
        );

        content = content.child(section("TOGGLE GROUP")).child(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(toggle_group)
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(0x6c7086))
                        .child(format!(
                            "Selected: {} ({})",
                            ["One", "Two", "Three"][selected_segment],
                            selected_segment,
                        )),
                ),
        );

        content = content.child(section("IMAGE (GPU-PAINTED)")).child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(16.0))
                .child(
                    div()
                        .w(px(40.0))
                        .h(px(40.0))
                        .rounded(px(20.0))
                        .bg(rgb(0x89b4fa))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child("🌍"),
                )
                .child(
                    div()
                        .w(px(40.0))
                        .h(px(40.0))
                        .rounded(px(20.0))
                        .bg(rgb(0xf9e2af))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child("⭐"),
                )
                .child(
                    div()
                        .w(px(40.0))
                        .h(px(40.0))
                        .rounded(px(20.0))
                        .bg(rgb(0xf2cdcd))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child("❤"),
                )
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(rgb(0xa6adc8))
                        .child("GPUI painted"),
                ),
        );

        div()
            .track_focus(&self.focus_handle)
            .on_key_down(
                cx.listener(|this: &mut Self, event: &KeyDownEvent, window, cx| {
                    if !this.focus_handle.is_focused(window) {
                        return;
                    }

                    let key = &event.keystroke.key;
                    if key == "backspace" {
                        this.text_field_value.pop();
                        cx.notify();
                    } else if key == "enter" {
                        log::info!("controls text submit: {:?}", this.text_field_value);
                    } else if let Some(ch) = &event.keystroke.key_char {
                        this.text_field_value.push_str(ch);
                        cx.notify();
                    }
                }),
            )
            .size_full()
            .flex()
            .flex_col()
            .pl(safe.left)
            .pr(safe.right)
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .child(
                div()
                    .w_full()
                    .pt(safe.top)
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_end()
                    .bg(rgb(0x313244))
                    .child(
                        div()
                            .h(px(60.0))
                            .flex()
                            .items_center()
                            .child(div().text_size(px(20.0)).child("Controls Demo")),
                    ),
            )
            .child(
                div()
                    .id("controls-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(content),
            )
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_run_controls_demo() {
    run_ios_app("dev.glasshq.GPUIiOSControlsDemo", |_window, cx| {
        IosControlsDemo {
            focus_handle: cx.focus_handle(),
            button_tap_count: 0,
            switch_on: false,
            checkbox_checked: false,
            slider_value: 0.5,
            stepper_value: 0,
            text_field_value: String::new(),
            selected_segment: 0,
        }
    });
}

// ---------------------------------------------------------------------------
// 13. Native Controls Demo — showcases platform-native UIKit controls on iOS
// ---------------------------------------------------------------------------

struct IosNativeControlsDemo {
    button_tap_count: usize,
    switch_on: bool,
    checkbox_checked: bool,
    slider_value: f64,
    stepper_value: f64,
    text_field_value: String,
    progress_value: f64,
    selected_segment: usize,
}

impl Render for IosNativeControlsDemo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let safe = window.safe_area_insets();
        let button_tap_count = self.button_tap_count;
        let switch_on = self.switch_on;
        let checkbox_checked = self.checkbox_checked;
        let slider_value = self.slider_value;
        let stepper_value = self.stepper_value;
        let text_field_value = self.text_field_value.clone();
        let progress_value = self.progress_value;
        let selected_segment = self.selected_segment.min(2);

        // Row helper: label on the left, control on the right
        fn row(label: &str, control: impl IntoElement) -> gpui::Div {
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .w_full()
                .gap(px(12.0))
                .child(
                    div()
                        .text_size(px(15.0))
                        .text_color(rgb(0xcdd6f4))
                        .flex_shrink_0()
                        .child(label.to_string()),
                )
                .child(div().flex_shrink_0().child(control))
        }

        // Section header
        let section = |title: &str| {
            div().w_full().pt(px(16.0)).pb(px(4.0)).child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(0x6c7086))
                    .child(title.to_string()),
            )
        };

        let mut content = div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .p(px(20.0))
            .pb(safe.bottom)
            .w_full();

        // --- Title ---
        content = content.child(
            div()
                .text_size(px(24.0))
                .text_color(rgb(0xcdd6f4))
                .pb(px(8.0))
                .child("Native Controls"),
        );

        // --- Button ---
        content = content.child(section("BUTTON")).child(row(
            &format!("Taps: {}", button_tap_count),
            native_button("demo-btn", "Tap Me").on_click(cx.listener(
                |this, _event, _window, cx| {
                    this.button_tap_count += 1;
                    log::info!("native button tapped: {}", this.button_tap_count);
                    cx.notify();
                },
            )),
        ));

        // --- Switch ---
        content = content.child(section("SWITCH")).child(row(
            &format!("Switch: {}", if switch_on { "ON" } else { "OFF" }),
            native_switch("demo-switch")
                .checked(switch_on)
                .on_change(
                    cx.listener(|this, event: &gpui::SwitchChangeEvent, _window, cx| {
                        this.switch_on = event.checked;
                        log::info!("switch changed: {}", this.switch_on);
                        cx.notify();
                    }),
                ),
        ));

        // --- Checkbox ---
        content = content.child(section("CHECKBOX")).child(row(
            &format!("Checked: {}", checkbox_checked),
            native_checkbox("demo-checkbox", "Enable feature")
                .checked(checkbox_checked)
                .on_change(
                    cx.listener(|this, event: &gpui::CheckboxChangeEvent, _window, cx| {
                        this.checkbox_checked = event.checked;
                        log::info!("checkbox changed: {}", this.checkbox_checked);
                        cx.notify();
                    }),
                ),
        ));

        // --- Slider ---
        content = content.child(section("SLIDER")).child(row(
            &format!("Value: {:.0}%", slider_value * 100.0),
            native_slider("demo-slider")
                .range(0.0, 1.0)
                .value(slider_value)
                .on_change(
                    cx.listener(|this, event: &gpui::SliderChangeEvent, _window, cx| {
                        this.slider_value = event.value;
                        // Drive the progress bar from the slider
                        this.progress_value = event.value;
                        cx.notify();
                    }),
                ),
        ));

        // --- Stepper ---
        content = content.child(section("STEPPER")).child(row(
            &format!("Count: {:.0}", stepper_value),
            native_stepper("demo-stepper")
                .range(0.0, 20.0)
                .value(stepper_value)
                .increment(1.0)
                .on_change(
                    cx.listener(|this, event: &gpui::StepperChangeEvent, _window, cx| {
                        this.stepper_value = event.value;
                        log::info!("stepper changed: {}", this.stepper_value);
                        cx.notify();
                    }),
                ),
        ));

        // --- Text Field ---
        content = content.child(section("TEXT FIELD")).child(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    native_text_field("demo-textfield")
                        .placeholder("Type something...")
                        .value(gpui::SharedString::from(text_field_value.clone()))
                        .on_change(cx.listener(
                            |this, event: &gpui::TextChangeEvent, _window, cx| {
                                this.text_field_value = event.text.clone();
                                cx.notify();
                            },
                        )),
                )
                .child(div().text_size(px(12.0)).text_color(rgb(0x6c7086)).child(
                    if text_field_value.is_empty() {
                        "No text entered".to_string()
                    } else {
                        format!("Text: {}", text_field_value)
                    },
                )),
        );

        // --- Progress Bar ---
        content = content.child(section("PROGRESS BAR")).child(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    native_progress_bar("demo-progress")
                        .range(0.0, 1.0)
                        .value(progress_value),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(0x6c7086))
                        .child(format!(
                            "{:.0}% (drag slider above)",
                            progress_value * 100.0
                        )),
                ),
        );

        // --- Toggle Group ---
        content = content.child(section("TOGGLE GROUP")).child(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    native_toggle_group("demo-toggle", &["One", "Two", "Three"])
                        .selected_index(selected_segment)
                        .on_select(cx.listener(
                            |this, event: &gpui::SegmentSelectEvent, _window, cx| {
                                this.selected_segment = event.index.min(2);
                                log::info!("segment selected: {}", this.selected_segment);
                                cx.notify();
                            },
                        )),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(0x6c7086))
                        .child(format!(
                            "Selected: {} ({})",
                            ["One", "Two", "Three"][selected_segment],
                            selected_segment,
                        )),
                ),
        );

        // --- Image View (SF Symbol) ---
        content = content.child(section("IMAGE VIEW (SF Symbol)")).child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(16.0))
                .child(
                    native_image_view("demo-img-globe")
                        .sf_symbol_config("globe", 32.0, NativeImageSymbolWeight::Medium)
                        .tint_color(0.337, 0.706, 0.98, 1.0) // blue
                        .w(px(40.0))
                        .h(px(40.0)),
                )
                .child(
                    native_image_view("demo-img-star")
                        .sf_symbol_config("star.fill", 32.0, NativeImageSymbolWeight::Medium)
                        .tint_color(0.949, 0.886, 0.686, 1.0) // yellow
                        .w(px(40.0))
                        .h(px(40.0)),
                )
                .child(
                    native_image_view("demo-img-heart")
                        .sf_symbol_config("heart.fill", 32.0, NativeImageSymbolWeight::Medium)
                        .tint_color(0.953, 0.545, 0.659, 1.0) // pink
                        .w(px(40.0))
                        .h(px(40.0)),
                )
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(rgb(0xa6adc8))
                        .child("SF Symbols"),
                ),
        );

        // Scrollable wrapper
        div()
            .size_full()
            .flex()
            .flex_col()
            .pl(safe.left)
            .pr(safe.right)
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .child(
                div()
                    .w_full()
                    .pt(safe.top)
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_end()
                    .bg(rgb(0x313244))
                    .child(
                        div()
                            .h(px(60.0))
                            .flex()
                            .items_center()
                            .child(div().text_size(px(20.0)).child("Native Controls Demo")),
                    ),
            )
            .child(
                div()
                    .id("native-controls-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(content),
            )
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_run_native_controls_demo() {
    run_ios_app("dev.glasshq.GPUIiOSNativeControlsDemo", |_, _| {
        IosNativeControlsDemo {
            button_tap_count: 0,
            switch_on: false,
            checkbox_checked: false,
            slider_value: 0.5,
            stepper_value: 0.0,
            text_field_value: String::new(),
            progress_value: 0.5,
            selected_segment: 0,
        }
    });
}

// ---------------------------------------------------------------------------
// 14. Safe Area Demo — visual safe area inset display + opt-out example
// ---------------------------------------------------------------------------

struct IosSafeAreaDemo {
    show_raw: bool,
}

impl Render for IosSafeAreaDemo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let safe = window.safe_area_insets();
        let show_raw = self.show_raw;

        // Visual: show a colored band along each safe area edge
        let inset_label =
            |label: &str, px_val: f32| -> String { format!("{}: {:.0}px", label, px_val) };

        // The outer div fills the full screen — background visible behind notch/home indicator
        div()
            .size_full()
            .bg(rgb(0x1e1e2e))
            .relative()
            // Top safe area band (shows the notch zone)
            .child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .right(px(0.0))
                    .h(safe.top)
                    .bg(rgba(0xcba6f730))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(0xcba6f7))
                            .child(inset_label("top", f32::from(safe.top))),
                    ),
            )
            // Bottom safe area band (home indicator)
            .child(
                div()
                    .absolute()
                    .bottom(px(0.0))
                    .left(px(0.0))
                    .right(px(0.0))
                    .h(safe.bottom)
                    .bg(rgba(0xf38ba830))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(0xf38ba8))
                            .child(inset_label("bottom", f32::from(safe.bottom))),
                    ),
            )
            // Left safe area band (landscape notch side)
            .child(
                div()
                    .absolute()
                    .top(safe.top)
                    .bottom(safe.bottom)
                    .left(px(0.0))
                    .w(safe.left)
                    .bg(rgba(0x89b4fa30))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(0x89b4fa))
                            .child(inset_label("l", f32::from(safe.left))),
                    ),
            )
            // Right safe area band (landscape home indicator side)
            .child(
                div()
                    .absolute()
                    .top(safe.top)
                    .bottom(safe.bottom)
                    .right(px(0.0))
                    .w(safe.right)
                    .bg(rgba(0xa6e3a130))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(0xa6e3a1))
                            .child(inset_label("r", f32::from(safe.right))),
                    ),
            )
            // Safe content area — the green rectangle shows the actual safe zone
            .child(
                div()
                    .absolute()
                    .top(safe.top)
                    .bottom(safe.bottom)
                    .left(safe.left)
                    .right(safe.right)
                    .border_2()
                    .border_color(rgb(0xa6e3a1))
                    .rounded(px(4.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(12.0))
                    .text_color(rgb(0xcdd6f4))
                    .child(div().text_size(px(18.0)).child("Safe Area Demo"))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(0xa6adc8))
                            .child("Colored bands = unsafe zones"),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(0xa6adc8))
                            .child("Green border = safe content area"),
                    )
                    .child(
                        div()
                            .p(px(12.0))
                            .bg(rgb(0x313244))
                            .rounded(px(8.0))
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(0xcba6f7))
                                    .child(format!("top:    {:.0}px", f32::from(safe.top))),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(0xf38ba8))
                                    .child(format!("bottom: {:.0}px", f32::from(safe.bottom))),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(0x89b4fa))
                                    .child(format!("left:   {:.0}px", f32::from(safe.left))),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(0xa6e3a1))
                                    .child(format!("right:  {:.0}px", f32::from(safe.right))),
                            ),
                    )
                    // Toggle button to demo ignoring safe area vs respecting it
                    .child(
                        div()
                            .id("toggle-safe")
                            .px(px(16.0))
                            .py(px(8.0))
                            .rounded(px(8.0))
                            .bg(if show_raw {
                                rgb(0xf38ba8)
                            } else {
                                rgb(0xa6e3a1)
                            })
                            .text_color(rgb(0x1e1e2e))
                            .text_size(px(13.0))
                            .child(if show_raw {
                                "Mode: Full Screen (unsafe)"
                            } else {
                                "Mode: Safe Area (default)"
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.show_raw = !this.show_raw;
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(0x6c7086))
                            .child("Tap button to toggle safe area mode"),
                    ),
            )
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_run_safe_area_demo() {
    run_ios_app("dev.glasshq.GPUIiOSSafeAreaDemo", |_, _| IosSafeAreaDemo {
        show_raw: false,
    });
}

// ---------------------------------------------------------------------------
// 15. Layout Showcase — comprehensive demo of all GPUI layout APIs
// ---------------------------------------------------------------------------

struct IosLayoutShowcase {
    selected_tab: usize,
}

impl Render for IosLayoutShowcase {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let safe = window.safe_area_insets();
        let selected_tab = self.selected_tab;

        // Tab labels
        let tabs = ["Flex", "Gaps", "Sizing", "Overflow", "Position"];

        // Tab bar at bottom (like a native iOS tab bar)
        let tab_bar = div()
            .w_full()
            .pb(safe.bottom)
            .bg(rgb(0x181825))
            .border_t_1()
            .border_color(rgb(0x313244))
            .flex()
            .flex_row()
            .children(tabs.iter().enumerate().map(|(i, label)| {
                let is_active = selected_tab == i;
                div()
                    .flex_1()
                    .py(px(10.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(2.0))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.selected_tab = i;
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(if is_active {
                                rgb(0x89b4fa)
                            } else {
                                rgb(0x585b70)
                            })
                            .child(*label),
                    )
            }));

        // Section label helper
        fn section_label(text: &str) -> gpui::Div {
            div()
                .text_size(px(11.0))
                .text_color(rgb(0x6c7086))
                .pb(px(6.0))
                .child(text.to_string())
        }

        // Content for each tab
        let content = match selected_tab {
            // --- Tab 0: Flexbox ---
            0 => div()
                .flex()
                .flex_col()
                .gap(px(16.0))
                .p(px(16.0))
                // Row: flex_row + justify_between
                .child(section_label("flex_row + justify_between"))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_row()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .w(px(60.0))
                                .h(px(40.0))
                                .bg(rgb(0xf38ba8))
                                .rounded(px(6.0)),
                        )
                        .child(
                            div()
                                .w(px(60.0))
                                .h(px(40.0))
                                .bg(rgb(0xa6e3a1))
                                .rounded(px(6.0)),
                        )
                        .child(
                            div()
                                .w(px(60.0))
                                .h(px(40.0))
                                .bg(rgb(0x89b4fa))
                                .rounded(px(6.0)),
                        ),
                )
                // Row: flex_row + justify_center + gap
                .child(section_label("flex_row + justify_center + gap"))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_row()
                        .justify_center()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .w(px(50.0))
                                .h(px(50.0))
                                .bg(rgb(0xfab387))
                                .rounded(px(8.0)),
                        )
                        .child(
                            div()
                                .w(px(50.0))
                                .h(px(50.0))
                                .bg(rgb(0xcba6f7))
                                .rounded(px(8.0)),
                        )
                        .child(
                            div()
                                .w(px(50.0))
                                .h(px(50.0))
                                .bg(rgb(0xf9e2af))
                                .rounded(px(8.0)),
                        ),
                )
                // Column: items_start / center / end
                .child(section_label("flex_col + items_start / center / end"))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_row()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex_1()
                                .h(px(80.0))
                                .bg(rgb(0x313244))
                                .rounded(px(6.0))
                                .flex()
                                .flex_col()
                                .items_start()
                                .p(px(4.0))
                                .child(
                                    div()
                                        .w(px(30.0))
                                        .h(px(20.0))
                                        .bg(rgb(0xf38ba8))
                                        .rounded(px(4.0)),
                                )
                                .child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(0x6c7086))
                                        .child("start"),
                                ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .h(px(80.0))
                                .bg(rgb(0x313244))
                                .rounded(px(6.0))
                                .flex()
                                .flex_col()
                                .items_center()
                                .p(px(4.0))
                                .child(
                                    div()
                                        .w(px(30.0))
                                        .h(px(20.0))
                                        .bg(rgb(0xa6e3a1))
                                        .rounded(px(4.0)),
                                )
                                .child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(0x6c7086))
                                        .child("center"),
                                ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .h(px(80.0))
                                .bg(rgb(0x313244))
                                .rounded(px(6.0))
                                .flex()
                                .flex_col()
                                .items_end()
                                .p(px(4.0))
                                .child(
                                    div()
                                        .w(px(30.0))
                                        .h(px(20.0))
                                        .bg(rgb(0x89b4fa))
                                        .rounded(px(4.0)),
                                )
                                .child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(0x6c7086))
                                        .child("end"),
                                ),
                        ),
                )
                // justify_start / center / end / between
                .child(section_label("justify_start / center / end / between"))
                .child(
                    div().w_full().flex().flex_col().gap(px(6.0)).children(
                        [
                            ("start", 0x89b4fau32),
                            ("center", 0xa6e3a1u32),
                            ("end", 0xfab387u32),
                            ("between", 0xcba6f7u32),
                        ]
                        .iter()
                        .map(|(label, color)| {
                            let row = div()
                                .w_full()
                                .h(px(28.0))
                                .bg(rgb(0x313244))
                                .rounded(px(4.0))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(4.0));
                            let row = match *label {
                                "start" => row.justify_start(),
                                "center" => row.justify_center(),
                                "end" => row.justify_end(),
                                _ => row.justify_between(),
                            };
                            row.child(
                                div()
                                    .w(px(20.0))
                                    .h(px(16.0))
                                    .bg(rgb(*color))
                                    .rounded(px(3.0)),
                            )
                            .child(
                                div()
                                    .w(px(20.0))
                                    .h(px(16.0))
                                    .bg(rgb(*color))
                                    .rounded(px(3.0)),
                            )
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(rgb(0x6c7086))
                                    .child(label.to_string()),
                            )
                        }),
                    ),
                )
                .into_any_element(),

            // --- Tab 1: Gaps & Padding ---
            1 => div()
                .flex()
                .flex_col()
                .gap(px(16.0))
                .p(px(16.0))
                // gap() uniform
                .child(section_label("gap(px) — uniform spacing"))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_row()
                        .gap(px(4.0))
                        .children((0..8).map(|_| {
                            div()
                                .flex_1()
                                .h(px(32.0))
                                .bg(rgb(0x89b4fa))
                                .rounded(px(4.0))
                        })),
                )
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_row()
                        .gap(px(12.0))
                        .children((0..4).map(|_| {
                            div()
                                .flex_1()
                                .h(px(32.0))
                                .bg(rgb(0xcba6f7))
                                .rounded(px(4.0))
                        })),
                )
                // Padding variants
                .child(section_label("padding — p / px / py / pt / pb / pl / pr"))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(
                            div()
                                .w_full()
                                .bg(rgb(0x313244))
                                .rounded(px(6.0))
                                .p(px(16.0))
                                .child(
                                    div()
                                        .w_full()
                                        .h(px(20.0))
                                        .bg(rgb(0xf38ba8))
                                        .rounded(px(4.0))
                                        .child(
                                            div()
                                                .text_size(px(9.0))
                                                .text_color(rgb(0x1e1e2e))
                                                .child("p(16)"),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .w_full()
                                .bg(rgb(0x313244))
                                .rounded(px(6.0))
                                .px(px(24.0))
                                .py(px(8.0))
                                .child(
                                    div()
                                        .w_full()
                                        .h(px(20.0))
                                        .bg(rgb(0xa6e3a1))
                                        .rounded(px(4.0))
                                        .child(
                                            div()
                                                .text_size(px(9.0))
                                                .text_color(rgb(0x1e1e2e))
                                                .child("px(24) py(8)"),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .w_full()
                                .bg(rgb(0x313244))
                                .rounded(px(6.0))
                                .pt(px(20.0))
                                .pb(px(4.0))
                                .pl(px(8.0))
                                .pr(px(32.0))
                                .child(
                                    div()
                                        .w_full()
                                        .h(px(20.0))
                                        .bg(rgb(0x89b4fa))
                                        .rounded(px(4.0))
                                        .child(
                                            div()
                                                .text_size(px(9.0))
                                                .text_color(rgb(0x1e1e2e))
                                                .child("pt20 pb4 pl8 pr32"),
                                        ),
                                ),
                        ),
                )
                // Margin variants
                .child(section_label("margin — m / mx / my / mt / mb / ml / mr"))
                .child(
                    div()
                        .w_full()
                        .bg(rgb(0x313244))
                        .rounded(px(6.0))
                        .flex()
                        .flex_row()
                        .child(
                            div()
                                .m(px(8.0))
                                .flex_1()
                                .h(px(36.0))
                                .bg(rgb(0xf9e2af))
                                .rounded(px(4.0))
                                .child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(0x1e1e2e))
                                        .child("m(8)"),
                                ),
                        )
                        .child(
                            div()
                                .mx(px(4.0))
                                .my(px(12.0))
                                .flex_1()
                                .h(px(36.0))
                                .bg(rgb(0x94e2d5))
                                .rounded(px(4.0))
                                .child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(0x1e1e2e))
                                        .child("mx4 my12"),
                                ),
                        ),
                )
                .into_any_element(),

            // --- Tab 2: Sizing ---
            2 => div()
                .flex()
                .flex_col()
                .gap(px(16.0))
                .p(px(16.0))
                // Fixed sizes
                .child(section_label("fixed w() / h()"))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(
                            div()
                                .w(px(100.0))
                                .h(px(24.0))
                                .bg(rgb(0xf38ba8))
                                .rounded(px(4.0))
                                .child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(0x1e1e2e))
                                        .child("w100 h24"),
                                ),
                        )
                        .child(
                            div()
                                .w(px(200.0))
                                .h(px(32.0))
                                .bg(rgb(0xa6e3a1))
                                .rounded(px(4.0))
                                .child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(0x1e1e2e))
                                        .child("w200 h32"),
                                ),
                        )
                        .child(
                            div()
                                .w(px(300.0))
                                .h(px(40.0))
                                .bg(rgb(0x89b4fa))
                                .rounded(px(4.0))
                                .child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(0x1e1e2e))
                                        .child("w300 h40"),
                                ),
                        ),
                )
                // w_full, flex_1, flex_shrink_0
                .child(section_label("w_full / flex_1 / flex_shrink_0"))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_row()
                        .gap(px(6.0))
                        .child(
                            div()
                                .flex_1()
                                .h(px(40.0))
                                .bg(rgb(0xfab387))
                                .rounded(px(4.0))
                                .child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(0x1e1e2e))
                                        .child("flex_1"),
                                ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .h(px(40.0))
                                .bg(rgb(0xcba6f7))
                                .rounded(px(4.0))
                                .child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(0x1e1e2e))
                                        .child("flex_1"),
                                ),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .w(px(80.0))
                                .h(px(40.0))
                                .bg(rgb(0xf9e2af))
                                .rounded(px(4.0))
                                .child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(0x1e1e2e))
                                        .child("shrink_0"),
                                ),
                        ),
                )
                // min/max width & height
                .child(section_label("min_w / max_w / min_h / max_h"))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(
                            div()
                                .w_full()
                                .bg(rgb(0x313244))
                                .rounded(px(6.0))
                                .p(px(8.0))
                                .child(
                                    div()
                                        .min_w(px(80.0))
                                        .max_w(px(200.0))
                                        .h(px(28.0))
                                        .bg(rgb(0x94e2d5))
                                        .rounded(px(4.0))
                                        .child(
                                            div()
                                                .text_size(px(9.0))
                                                .text_color(rgb(0x1e1e2e))
                                                .child("min80 max200"),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .w_full()
                                .bg(rgb(0x313244))
                                .rounded(px(6.0))
                                .p(px(8.0))
                                .flex()
                                .flex_row()
                                .gap(px(4.0))
                                .children((0..5).map(|i| {
                                    div()
                                        .flex_1()
                                        .min_h(px(20.0))
                                        .max_h(px(60.0))
                                        .h(px(20.0 + i as f32 * 10.0))
                                        .bg(rgb(0x89b4fa))
                                        .rounded(px(4.0))
                                })),
                        ),
                )
                .into_any_element(),

            // --- Tab 3: Overflow ---
            3 => {
                let scroll_colors = [
                    0xf38ba8u32,
                    0xa6e3a1,
                    0x89b4fa,
                    0xfab387,
                    0xcba6f7,
                    0xf9e2af,
                    0x94e2d5,
                ];

                let mut vlist = div().flex().flex_col().gap(px(6.0));
                for i in 0..20 {
                    vlist = vlist.child(
                        div()
                            .w_full()
                            .h(px(44.0))
                            .bg(rgb(scroll_colors[i % scroll_colors.len()]))
                            .rounded(px(6.0))
                            .flex()
                            .items_center()
                            .px(px(12.0))
                            .text_color(rgb(0x1e1e2e))
                            .child(format!("overflow_y row {}", i + 1)),
                    );
                }

                let mut hstrip = div().flex().flex_row().gap(px(8.0));
                for i in 0..15 {
                    hstrip = hstrip.child(
                        div()
                            .w(px(100.0))
                            .h(px(80.0))
                            .flex_shrink_0()
                            .bg(rgb(scroll_colors[i % scroll_colors.len()]))
                            .rounded(px(8.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(rgb(0x1e1e2e))
                            .child(format!("{}", i + 1)),
                    );
                }

                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .p(px(16.0))
                    .child(section_label("overflow_y_scroll"))
                    .child(
                        div()
                            .id("overflow-y-demo")
                            .w_full()
                            .h(px(200.0))
                            .overflow_y_scroll()
                            .bg(rgb(0x181825))
                            .rounded(px(8.0))
                            .p(px(8.0))
                            .child(vlist),
                    )
                    .child(section_label("overflow_x_scroll"))
                    .child(
                        div()
                            .id("overflow-x-demo")
                            .w_full()
                            .overflow_x_scroll()
                            .bg(rgb(0x181825))
                            .rounded(px(8.0))
                            .p(px(8.0))
                            .child(hstrip),
                    )
                    .child(section_label("overflow_hidden (clips content)"))
                    .child(
                        div()
                            .w(px(200.0))
                            .h(px(60.0))
                            .overflow_hidden()
                            .bg(rgb(0x313244))
                            .rounded(px(8.0))
                            .flex()
                            .items_center()
                            .child(
                                div()
                                    .w(px(400.0))
                                    .h(px(40.0))
                                    .bg(rgb(0xf38ba8))
                                    .flex()
                                    .items_center()
                                    .px(px(8.0))
                                    .child(
                                        "This text is wider than the container and gets clipped",
                                    ),
                            ),
                    )
                    .into_any_element()
            }

            // --- Tab 4: Position ---
            _ => div()
                .flex()
                .flex_col()
                .gap(px(16.0))
                .p(px(16.0))
                .child(section_label("relative + absolute positioning"))
                .child(
                    div()
                        .w_full()
                        .h(px(180.0))
                        .bg(rgb(0x313244))
                        .rounded(px(8.0))
                        .relative()
                        // Corners
                        .child(
                            div()
                                .absolute()
                                .top(px(8.0))
                                .left(px(8.0))
                                .w(px(48.0))
                                .h(px(48.0))
                                .bg(rgb(0xf38ba8))
                                .rounded(px(6.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(8.0))
                                .text_color(rgb(0x1e1e2e))
                                .child("t8 l8"),
                        )
                        .child(
                            div()
                                .absolute()
                                .top(px(8.0))
                                .right(px(8.0))
                                .w(px(48.0))
                                .h(px(48.0))
                                .bg(rgb(0xa6e3a1))
                                .rounded(px(6.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(8.0))
                                .text_color(rgb(0x1e1e2e))
                                .child("t8 r8"),
                        )
                        .child(
                            div()
                                .absolute()
                                .bottom(px(8.0))
                                .left(px(8.0))
                                .w(px(48.0))
                                .h(px(48.0))
                                .bg(rgb(0x89b4fa))
                                .rounded(px(6.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(8.0))
                                .text_color(rgb(0x1e1e2e))
                                .child("b8 l8"),
                        )
                        .child(
                            div()
                                .absolute()
                                .bottom(px(8.0))
                                .right(px(8.0))
                                .w(px(48.0))
                                .h(px(48.0))
                                .bg(rgb(0xfab387))
                                .rounded(px(6.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(8.0))
                                .text_color(rgb(0x1e1e2e))
                                .child("b8 r8"),
                        )
                        // Center element
                        .child(
                            div()
                                .absolute()
                                .inset(px(60.0))
                                .bg(rgb(0xcba6f7))
                                .rounded(px(8.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(10.0))
                                .text_color(rgb(0x1e1e2e))
                                .child("inset(60)"),
                        ),
                )
                .child(section_label("z-index layering (absolute stacked divs)"))
                .child(
                    div()
                        .w_full()
                        .h(px(120.0))
                        .relative()
                        .child(
                            div()
                                .absolute()
                                .top(px(0.0))
                                .left(px(0.0))
                                .w(px(120.0))
                                .h(px(80.0))
                                .bg(rgb(0xf38ba8))
                                .rounded(px(8.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(10.0))
                                .text_color(rgb(0x1e1e2e))
                                .child("layer 1"),
                        )
                        .child(
                            div()
                                .absolute()
                                .top(px(20.0))
                                .left(px(40.0))
                                .w(px(120.0))
                                .h(px(80.0))
                                .bg(rgb(0xa6e3a1))
                                .rounded(px(8.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(10.0))
                                .text_color(rgb(0x1e1e2e))
                                .child("layer 2"),
                        )
                        .child(
                            div()
                                .absolute()
                                .top(px(40.0))
                                .left(px(80.0))
                                .w(px(120.0))
                                .h(px(80.0))
                                .bg(rgb(0x89b4fa))
                                .rounded(px(8.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(10.0))
                                .text_color(rgb(0x1e1e2e))
                                .child("layer 3"),
                        ),
                )
                .into_any_element(),
        };

        // Full layout: header + scrollable tab content + tab bar
        div()
            .size_full()
            .flex()
            .flex_col()
            .pl(safe.left)
            .pr(safe.right)
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            // Header (notch-aware)
            .child(
                div()
                    .w_full()
                    .pt(safe.top)
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_end()
                    .bg(rgb(0x181825))
                    .child(
                        div()
                            .h(px(52.0))
                            .flex()
                            .items_center()
                            .text_size(px(17.0))
                            .text_color(rgb(0xcdd6f4))
                            .child(format!("Layout — {}", tabs[selected_tab])),
                    ),
            )
            // Scrollable content area
            .child(
                div()
                    .id("layout-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(content),
            )
            // Tab bar (home indicator-aware)
            .child(tab_bar)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_run_layout_showcase() {
    run_ios_app("dev.glasshq.GPUIiOSLayoutShowcase", |_, _| {
        IosLayoutShowcase { selected_tab: 0 }
    });
}

// ---------------------------------------------------------------------------
// Validation Demos
// ---------------------------------------------------------------------------

struct IosTier1KeyboardImeDemo {
    focus_handle: FocusHandle,
    text: String,
    recent_events: Vec<String>,
}

impl Focusable for IosTier1KeyboardImeDemo {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for IosTier1KeyboardImeDemo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let safe = window.safe_area_insets();
        let focused = self.focus_handle.is_focused(window);
        let border_color = if focused { 0x89b4fa } else { 0x585b70 };
        let display_text = if self.text.is_empty() {
            "<empty>".to_string()
        } else {
            self.text.clone()
        };

        let mut events = div().flex().flex_col().gap(px(4.0));
        for line in self.recent_events.iter().rev().take(10) {
            events = events.child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(0xa6adc8))
                    .child(line.clone()),
            );
        }

        div()
            .id("tier1-keyboard-ime-root")
            .track_focus(&self.focus_handle)
            .size_full()
            .pt(safe.top + px(16.0))
            .pb(safe.bottom + px(16.0))
            .pl(safe.left + px(16.0))
            .pr(safe.right + px(16.0))
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .on_key_down(cx.listener(|this: &mut Self, event: &KeyDownEvent, _, cx| {
                let key = event.keystroke.key.clone();
                let key_char = event.keystroke.key_char.clone();

                if key == "backspace" {
                    this.text.pop();
                } else if key == "enter" {
                    this.text.push('\n');
                } else if let Some(ch) = key_char.as_deref() {
                    this.text.push_str(ch);
                }

                this.recent_events.push(format!(
                    "key={} key_char={:?} held={} mods={:?} native={:?}",
                    key,
                    key_char,
                    event.is_held,
                    event.keystroke.modifiers,
                    event.keystroke.native_key_code
                ));
                if this.recent_events.len() > 40 {
                    this.recent_events.remove(0);
                }

                cx.notify();
            }))
            .child(div().text_size(px(22.0)).child("Keyboard + IME Validation"))
            .child(div().text_size(px(12.0)).text_color(rgb(0xa6adc8)).child(
                "Tap the input area, then test hardware keys and IME composition (CJK/emoji).",
            ))
            .child(
                div()
                    .id("tier1-keyboard-ime-input")
                    .w_full()
                    .min_h(px(120.0))
                    .p(px(12.0))
                    .bg(rgb(0x313244))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(rgb(border_color))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_this, _, window, cx| {
                            cx.focus_self(window);
                            cx.notify();
                        }),
                    )
                    .child(div().text_size(px(15.0)).child(display_text)),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(rgb(0xf9e2af))
                    .child(if focused {
                        "Focus: active"
                    } else {
                        "Focus: inactive (tap input area)"
                    }),
            )
            .child(
                div()
                    .w_full()
                    .p(px(10.0))
                    .rounded(px(8.0))
                    .bg(rgb(0x181825))
                    .child(events),
            )
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_run_keyboard_ime_demo() {
    run_ios_app("dev.glasshq.GPUIiOSKeyboardIMEValidation", |_window, cx| {
        let focus_handle = cx.focus_handle();
        IosTier1KeyboardImeDemo {
            focus_handle,
            text: String::new(),
            recent_events: Vec::new(),
        }
    });
}

struct IosTier1FilePickerDemo {
    status: String,
    picked_paths: Vec<String>,
    save_path: Option<String>,
}

impl IosTier1FilePickerDemo {
    fn open_files(&mut self, cx: &mut Context<Self>) {
        self.status = "Opening file picker (files)...".to_string();
        cx.notify();

        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Open Files".into()),
        });

        cx.spawn(async move |this, cx| {
            let result = rx.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(Some(paths))) => {
                        this.picked_paths = paths
                            .into_iter()
                            .map(|path| path.display().to_string())
                            .collect();
                        this.status = format!("Selected {} file(s)", this.picked_paths.len());
                    }
                    Ok(Ok(None)) => {
                        this.status = "File picker cancelled".to_string();
                    }
                    Ok(Err(err)) => {
                        this.status = format!("File picker failed: {err}");
                    }
                    Err(err) => {
                        this.status = format!("File picker channel failed: {err}");
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn open_directories(&mut self, cx: &mut Context<Self>) {
        self.status = "Opening file picker (directories)...".to_string();
        cx.notify();

        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: true,
            prompt: Some("Open Folders".into()),
        });

        cx.spawn(async move |this, cx| {
            let result = rx.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(Some(paths))) => {
                        this.picked_paths = paths
                            .into_iter()
                            .map(|path| path.display().to_string())
                            .collect();
                        this.status = format!("Selected {} folder(s)", this.picked_paths.len());
                    }
                    Ok(Ok(None)) => {
                        this.status = "Directory picker cancelled".to_string();
                    }
                    Ok(Err(err)) => {
                        this.status = format!("Directory picker failed: {err}");
                    }
                    Err(err) => {
                        this.status = format!("Directory picker channel failed: {err}");
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn save_file(&mut self, cx: &mut Context<Self>) {
        self.status = "Opening save picker...".to_string();
        cx.notify();

        let default_dir = std::env::temp_dir();
        let rx = cx.prompt_for_new_path(&default_dir, Some("gpui-tier1-save.txt"));
        cx.spawn(async move |this, cx| {
            let result = rx.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(Some(path))) => {
                        let path = path.display().to_string();
                        this.save_path = Some(path.clone());
                        this.status = "Save destination selected".to_string();
                        this.picked_paths = vec![path];
                    }
                    Ok(Ok(None)) => {
                        this.status = "Save picker cancelled".to_string();
                    }
                    Ok(Err(err)) => {
                        this.status = format!("Save picker failed: {err}");
                    }
                    Err(err) => {
                        this.status = format!("Save picker channel failed: {err}");
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

impl Render for IosTier1FilePickerDemo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let safe = window.safe_area_insets();

        let mut picked = div().flex().flex_col().gap(px(4.0));
        if self.picked_paths.is_empty() {
            picked = picked.child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(0xa6adc8))
                    .child("No selected paths yet"),
            );
        } else {
            for path in self.picked_paths.iter().take(8) {
                picked = picked.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(0xa6adc8))
                        .child(path.clone()),
                );
            }
        }

        div()
            .id("tier1-file-picker-root")
            .size_full()
            .pt(safe.top + px(16.0))
            .pb(safe.bottom + px(16.0))
            .pl(safe.left + px(16.0))
            .pr(safe.right + px(16.0))
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(div().text_size(px(22.0)).child("File Open/Save Validation"))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(0xa6adc8))
                    .child("Uses real UIDocumentPicker open/export flows."),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(10.0))
                    .child(
                        native_button("tier1-pick-files", "Open Files")
                            .on_click(cx.listener(|this, _event, _window, cx| this.open_files(cx))),
                    )
                    .child(
                        native_button("tier1-pick-folders", "Open Folders").on_click(
                            cx.listener(|this, _event, _window, cx| this.open_directories(cx)),
                        ),
                    )
                    .child(
                        native_button("tier1-save-file", "Save As")
                            .on_click(cx.listener(|this, _event, _window, cx| this.save_file(cx))),
                    ),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(rgb(0xf9e2af))
                    .child(format!("Status: {}", self.status)),
            )
            .child(
                div()
                    .w_full()
                    .p(px(10.0))
                    .rounded(px(8.0))
                    .bg(rgb(0x181825))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(0xcdd6f4))
                            .child("Selected paths"),
                    )
                    .child(picked),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(0xa6adc8))
                    .child(format!(
                        "Last save path: {}",
                        self.save_path.as_deref().unwrap_or("<none>")
                    )),
            )
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_run_file_picker_demo() {
    run_ios_app("dev.glasshq.GPUIiOSFilePickerValidation", |_window, _cx| {
        IosTier1FilePickerDemo {
            status: "Ready".to_string(),
            picked_paths: Vec::new(),
            save_path: None,
        }
    });
}

struct IosTier1ClipboardDemo {
    status: String,
    last_text: String,
    last_metadata: String,
    last_image_size: usize,
}

impl IosTier1ClipboardDemo {
    fn copy_text_with_metadata(&mut self, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string_with_metadata(
            "Tier1 clipboard text".to_string(),
            "{\"source\":\"ios-demo\",\"kind\":\"text+metadata\"}".to_string(),
        ));
        self.status = "Wrote text + metadata to clipboard".to_string();
        cx.notify();
    }

    fn copy_image(&mut self, cx: &mut Context<Self>) {
        let image = Image::from_bytes(ImageFormat::Png, IOS_CLIPBOARD_TEST_PNG.to_vec());
        cx.write_to_clipboard(ClipboardItem::new_image(&image));
        self.status = format!(
            "Wrote PNG image to clipboard ({} bytes)",
            image.bytes().len()
        );
        cx.notify();
    }

    fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            self.status = "Clipboard was empty".to_string();
            self.last_text = "<none>".to_string();
            self.last_metadata = "<none>".to_string();
            self.last_image_size = 0;
            cx.notify();
            return;
        };

        self.last_text = item.text().unwrap_or_else(|| "<none>".to_string());
        self.last_metadata = item
            .metadata()
            .cloned()
            .unwrap_or_else(|| "<none>".to_string());
        self.last_image_size = item
            .entries()
            .iter()
            .find_map(|entry| match entry {
                ClipboardEntry::Image(image) => Some(image.bytes().len()),
                _ => None,
            })
            .unwrap_or(0);
        let count = item.entries().len();
        self.status = format!(
            "Read {} clipboard {}",
            count,
            if count == 1 { "entry" } else { "entries" }
        );
        cx.notify();
    }
}

impl Render for IosTier1ClipboardDemo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let safe = window.safe_area_insets();

        div()
            .id("tier1-clipboard-root")
            .size_full()
            .pt(safe.top + px(16.0))
            .pb(safe.bottom + px(16.0))
            .pl(safe.left + px(16.0))
            .pr(safe.right + px(16.0))
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(div().text_size(px(22.0)).child("Clipboard Validation"))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(0xa6adc8))
                    .child("Verifies text+metadata and image clipboard read/write paths."),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(10.0))
                    .child(
                        native_button("tier1-copy-text-meta", "Copy Text+Meta").on_click(
                            cx.listener(|this, _event, _window, cx| {
                                this.copy_text_with_metadata(cx)
                            }),
                        ),
                    )
                    .child(
                        native_button("tier1-copy-image", "Copy Image")
                            .on_click(cx.listener(|this, _event, _window, cx| this.copy_image(cx))),
                    )
                    .child(
                        native_button("tier1-paste", "Paste")
                            .on_click(cx.listener(|this, _event, _window, cx| this.paste(cx))),
                    ),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(rgb(0xf9e2af))
                    .child(format!("Status: {}", self.status)),
            )
            .child(
                div()
                    .w_full()
                    .p(px(10.0))
                    .rounded(px(8.0))
                    .bg(rgb(0x181825))
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(0xa6adc8))
                            .child(format!("Text: {}", self.last_text)),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(0xa6adc8))
                            .child(format!("Metadata: {}", self.last_metadata)),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(0xa6adc8))
                            .child(format!("Image bytes: {}", self.last_image_size)),
                    ),
            )
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_run_clipboard_demo() {
    run_ios_app("dev.glasshq.GPUIiOSClipboardValidation", |_window, _cx| {
        IosTier1ClipboardDemo {
            status: "Ready".to_string(),
            last_text: "<none>".to_string(),
            last_metadata: "<none>".to_string(),
            last_image_size: 0,
        }
    });
}

struct IosTier1FileDropDemo {
    status: String,
    hover_count: usize,
    dropped_paths: Vec<String>,
}

impl Render for IosTier1FileDropDemo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let safe = window.safe_area_insets();
        let border_color = if self.hover_count > 0 {
            0xa6e3a1
        } else {
            0x585b70
        };

        let mut dropped = div().flex().flex_col().gap(px(4.0));
        if self.dropped_paths.is_empty() {
            dropped = dropped.child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(0xa6adc8))
                    .child("No dropped paths yet"),
            );
        } else {
            for path in self.dropped_paths.iter().take(8) {
                dropped = dropped.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(0xa6adc8))
                        .child(path.clone()),
                );
            }
        }

        div()
            .id("tier1-file-drop-root")
            .size_full()
            .pt(safe.top + px(16.0))
            .pb(safe.bottom + px(16.0))
            .pl(safe.left + px(16.0))
            .pr(safe.right + px(16.0))
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .text_size(px(22.0))
                    .child("External File Drop Validation"),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(0xa6adc8))
                    .child("Drag files from Files app into the drop zone."),
            )
            .child(
                div()
                    .id("tier1-drop-zone")
                    .w_full()
                    .h(px(180.0))
                    .p(px(12.0))
                    .bg(rgb(0x313244))
                    .rounded(px(10.0))
                    .border_2()
                    .border_color(rgb(border_color))
                    .can_drop(|value, _, _| value.is::<ExternalPaths>())
                    .on_drag_move(cx.listener(
                        |this, event: &gpui::DragMoveEvent<ExternalPaths>, _window, cx| {
                            let paths = event.drag(cx).paths();
                            this.hover_count = paths.len();
                            this.status = format!("Dragging {} file(s)...", this.hover_count);
                            cx.notify();
                        },
                    ))
                    .on_drop(cx.listener(|this, paths: &ExternalPaths, _window, cx| {
                        this.hover_count = 0;
                        this.dropped_paths = paths
                            .paths()
                            .iter()
                            .map(|path| path.display().to_string())
                            .collect();
                        this.status = format!("Dropped {} file(s)", this.dropped_paths.len());
                        cx.notify();
                    }))
                    .child(div().text_size(px(14.0)).child("Drop zone")),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(rgb(0xf9e2af))
                    .child(format!("Status: {}", self.status)),
            )
            .child(
                div()
                    .w_full()
                    .p(px(10.0))
                    .rounded(px(8.0))
                    .bg(rgb(0x181825))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(0xcdd6f4))
                            .child("Dropped paths"),
                    )
                    .child(dropped),
            )
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_run_file_drop_demo() {
    run_ios_app("dev.glasshq.GPUIiOSFileDropValidation", |_window, _cx| {
        IosTier1FileDropDemo {
            status: "Waiting for drag".to_string(),
            hover_count: 0,
            dropped_paths: Vec::new(),
        }
    });
}

// ---------------------------------------------------------------------------
// Demo dispatcher — called from ObjC main.m with demo name as C string
// ---------------------------------------------------------------------------

const AVAILABLE_DEMOS: &[&str] = &[
    "hello_world",
    "touch",
    "text",
    "lifecycle",
    "combined",
    "scroll",
    "text_input",
    "vertical_scroll",
    "horizontal_scroll",
    "pinch",
    "rotation",
    "controls",
    "native_controls",
    "safe_area",
    "layout_showcase",
    "keyboard_ime",
    "file_picker",
    "clipboard",
    "file_drop",
];

#[unsafe(no_mangle)]
/// Run a named iOS demo entrypoint.
///
/// # Safety
/// `name` must be a valid, non-null, NUL-terminated C string pointer.
pub unsafe extern "C" fn gpui_ios_run_demo(name: *const std::ffi::c_char) {
    let name = unsafe { std::ffi::CStr::from_ptr(name) }
        .to_str()
        .unwrap_or("hello_world");
    match name {
        "hello_world" => gpui_ios_run_hello_world(),
        "touch" => gpui_ios_run_touch_demo(),
        "text" => gpui_ios_run_text_demo(),
        "lifecycle" => gpui_ios_run_lifecycle_demo(),
        "combined" => gpui_ios_run_combined_demo(),
        "scroll" => gpui_ios_run_scroll_demo(),
        "text_input" => gpui_ios_run_text_input_demo(),
        "vertical_scroll" => gpui_ios_run_vertical_scroll_demo(),
        "horizontal_scroll" => gpui_ios_run_horizontal_scroll_demo(),
        "pinch" => gpui_ios_run_pinch_demo(),
        "rotation" => gpui_ios_run_rotation_demo(),
        "controls" => gpui_ios_run_controls_demo(),
        "native_controls" => gpui_ios_run_native_controls_demo(),
        "safe_area" => gpui_ios_run_safe_area_demo(),
        "layout_showcase" => gpui_ios_run_layout_showcase(),
        "keyboard_ime" => gpui_ios_run_keyboard_ime_demo(),
        "file_picker" => gpui_ios_run_file_picker_demo(),
        "clipboard" => gpui_ios_run_clipboard_demo(),
        "file_drop" => gpui_ios_run_file_drop_demo(),
        unknown => {
            // Init logging so the error is visible
            init_logging("dev.glasshq.GPUIiOS");
            log::error!(
                "Unknown demo: '{}'. Available: {}",
                unknown,
                AVAILABLE_DEMOS.join(", ")
            );
            gpui_ios_run_hello_world();
        }
    }
}

/// Returns a newline-separated list of available demo names. Caller must free with `gpui_ios_free_string`.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_list_demos() -> *mut std::ffi::c_char {
    let list = AVAILABLE_DEMOS.join("\n");
    std::ffi::CString::new(list)
        .expect("demo names contain no NUL bytes")
        .into_raw()
}

/// Free a string returned by `gpui_ios_list_demos`.
///
/// # Safety
/// `s` must be a pointer returned by `gpui_ios_list_demos` and not already freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gpui_ios_free_string(s: *mut std::ffi::c_char) {
    if !s.is_null() {
        unsafe { drop(std::ffi::CString::from_raw(s)) };
    }
}
