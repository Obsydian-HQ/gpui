use gpui::{
    App, Application, Context, MouseButton, Window, WindowAppearance, WindowOptions, div,
    prelude::*, px, rgb,
};
use log::LevelFilter;
use std::io::Write;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static STARTED: AtomicBool = AtomicBool::new(false);

// ---------------------------------------------------------------------------
// iOS Logger — os_log + stderr + TCP relay
// ---------------------------------------------------------------------------

static TCP_SINK: Mutex<Option<TcpStream>> = Mutex::new(None);

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

        let os_log = oslog::OsLog::new(&self.subsystem, record.target());
        os_log.with_level(record.level().into(), &message);

        let color = Self::level_color(record.level());
        let reset = "\x1b[0m";
        let mut stderr = std::io::stderr().lock();
        let _ = writeln!(
            stderr,
            "{color}{}{reset} [{}] {}",
            Self::level_tag(record.level()),
            record.target(),
            message,
        );
        let _ = stderr.flush();

        if let Ok(mut guard) = TCP_SINK.lock() {
            if let Some(ref mut stream) = *guard {
                let line = format!(
                    "{color}{}{reset} [{}] {}\n",
                    Self::level_tag(record.level()),
                    record.target(),
                    message,
                );
                if stream.write_all(line.as_bytes()).is_err() {
                    *guard = None;
                }
            }
        }
    }

    fn flush(&self) {
        let _ = std::io::stderr().flush();
        if let Ok(mut guard) = TCP_SINK.lock() {
            if let Some(ref mut stream) = *guard {
                let _ = stream.flush();
            }
        }
    }
}

fn try_connect_log_relay() {
    let addr = match option_env!("GPUI_LOG_RELAY") {
        Some(a) if !a.is_empty() => a,
        _ => return,
    };

    let sock_addr = match addr.parse::<std::net::SocketAddr>() {
        Ok(a) => a,
        Err(_) => return,
    };

    match TcpStream::connect_timeout(&sock_addr, std::time::Duration::from_secs(2)) {
        Ok(stream) => {
            let _ = stream.set_nodelay(true);
            *TCP_SINK.lock().unwrap() = Some(stream);
        }
        Err(_) => {}
    }
}

fn init_logging(subsystem: &str) {
    try_connect_log_relay();
    let logger = IosLogger::new(subsystem);
    log::set_boxed_logger(Box::new(logger)).expect("failed to set logger");
    log::set_max_level(LevelFilter::Debug);
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
