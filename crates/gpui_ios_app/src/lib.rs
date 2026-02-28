use gpui::{
    App, Application, Context, FocusHandle, Focusable, KeyDownEvent, MouseButton, Window,
    WindowAppearance, WindowOptions, div, prelude::*, px, rgb,
};
use log::LevelFilter;
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static STARTED: AtomicBool = AtomicBool::new(false);

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

    log::info!("[GPUI-iOS] launching app");

    let app = Application::new();
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
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(20.0))
            .bg(rgb(0x1e1e2e))
            .child(div().w(px(200.0)).h(px(80.0)).bg(rgb(0xf38ba8)).rounded(px(12.0)))
            .child(div().w(px(200.0)).h(px(80.0)).bg(rgb(0xa6e3a1)).rounded(px(12.0)))
            .child(div().w(px(200.0)).h(px(80.0)).bg(rgb(0x89b4fa)).rounded(px(12.0)))
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tapped = self.tapped_box;
        let tap_count = self.tap_count;

        let box_color = |index: usize, base: u32, active: u32| -> u32 {
            if tapped == Some(index) { active } else { base }
        };

        div()
            .size_full()
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
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        log::info!("touch: red box tapped");
                        this.tapped_box = Some(0);
                        this.tap_count += 1;
                        cx.notify();
                    })),
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
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        log::info!("touch: green box tapped");
                        this.tapped_box = Some(1);
                        this.tap_count += 1;
                        cx.notify();
                    })),
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
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        log::info!("touch: blue box tapped");
                        this.tapped_box = Some(2);
                        this.tap_count += 1;
                        cx.notify();
                    })),
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
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(16.0))
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .child(
                div()
                    .text_size(px(32.0))
                    .child("Hello iOS!"),
            )
            .child(
                div()
                    .text_size(px(20.0))
                    .child("CoreText text rendering"),
            )
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
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(16.0))
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .child(
                div()
                    .text_size(px(24.0))
                    .child("Window Lifecycle"),
            )
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
        let bounds = window.bounds();
        let appearance = window.appearance();
        let scale = window.scale_factor();
        let tap_count = self.tap_count;
        let last_tapped = self.last_tapped.unwrap_or("none");

        let is_dark = matches!(appearance, WindowAppearance::Dark | WindowAppearance::VibrantDark);
        let bg_color = if is_dark { rgb(0x1e1e2e) } else { rgb(0xeff1f5) };
        let text_color = if is_dark { rgb(0xcdd6f4) } else { rgb(0x4c4f69) };
        let panel_bg = if is_dark { rgb(0x313244) } else { rgb(0xccd0da) };
        let muted_text = if is_dark { rgb(0x6c7086) } else { rgb(0x9ca0b0) };

        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(12.0))
            .bg(bg_color)
            .text_color(text_color)
            // Title
            .child(
                div()
                    .text_size(px(28.0))
                    .child("GPUI on iOS"),
            )
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
                    .child(
                        div()
                            .text_size(px(14.0))
                            .child(format!(
                                "{:.0}x{:.0} @{:.0}x  {:?}",
                                f32::from(bounds.size.width),
                                f32::from(bounds.size.height),
                                scale,
                                appearance,
                            )),
                    )
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
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.tap_count += 1;
                        this.last_tapped = Some("red");
                        cx.notify();
                    })),
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
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.tap_count += 1;
                        this.last_tapped = Some("green");
                        cx.notify();
                    })),
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
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.tap_count += 1;
                        this.last_tapped = Some("blue");
                        cx.notify();
                    })),
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
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let colors = [
            0xf38ba8u32, 0xa6e3a1, 0x89b4fa, 0xfab387, 0xcba6f7,
            0xf9e2af, 0x94e2d5, 0xf2cdcd, 0x89dceb, 0xb4befe,
        ];

        let mut scroll_content = div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(16.0));

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
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .child(
                div()
                    .w_full()
                    .h(px(60.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgb(0x313244))
                    .child(
                        div()
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let text = self.text.clone();
        let focused = self.focus_handle.is_focused(_window);

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
            .child(
                div()
                    .text_size(px(24.0))
                    .child("Text Input Demo"),
            )
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
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                        cx.focus_self(window);
                        cx.notify();
                    })),
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
    run_ios_app("dev.glasshq.GPUIiOSTextInputDemo", |window, cx| {
        let focus_handle = cx.focus_handle();
        IosTextInputDemo {
            focus_handle,
            text: String::new(),
        }
    });
}
