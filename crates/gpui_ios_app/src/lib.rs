use gpui::{App, Application, Context, Window, WindowOptions, div, prelude::*, px};
use log::LevelFilter;
use std::io::Write;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static STARTED: AtomicBool = AtomicBool::new(false);

// ---------------------------------------------------------------------------
// iOS Logger — os_log + stderr + TCP relay
//
// Desktop platforms (macOS, Linux, Windows) use `env_logger` which writes to
// stderr — you run the app, you see logs. On iOS there is no terminal attached
// to the process, so we need a different transport.
//
// Three output sinks:
//   1. os_log  — always on. Captured by Console.app, `log collect`, idevicesyslog.
//   2. stderr  — always on. Useful on simulator and if Apple ever fixes
//                `devicectl --console` for UIKit apps.
//   3. TCP     — connects to a log collector on the developer's Mac over Wi-Fi.
//                The Mac's IP is baked in at build time via GPUI_LOG_RELAY env var.
//                If the collector isn't running, this silently degrades to no-op.
//
// The TCP relay is what gives us the same DX as desktop: run `./scripts/run-device.sh`,
// see logs streaming in your terminal. Same approach as Expo/React Native (they use
// a WebSocket back to Metro), but simpler — raw TCP with newline-delimited text.
// ---------------------------------------------------------------------------

/// Global TCP connection to the log collector on the developer's Mac.
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
            log::Level::Error => "\x1b[31m", // red
            log::Level::Warn => "\x1b[33m",  // yellow
            log::Level::Info => "\x1b[32m",   // green
            log::Level::Debug => "\x1b[36m",  // cyan
            log::Level::Trace => "\x1b[90m",  // gray
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

        // 1) os_log — always available (Console.app, log collect, idevicesyslog)
        let os_log = oslog::OsLog::new(&self.subsystem, record.target());
        os_log.with_level(record.level().into(), &message);

        // 2) stderr — useful on simulator
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

        // 3) TCP relay — streams to developer's Mac terminal over Wi-Fi
        if let Ok(mut guard) = TCP_SINK.lock() {
            if let Some(ref mut stream) = *guard {
                let line = format!(
                    "{color}{}{reset} [{}] {}\n",
                    Self::level_tag(record.level()),
                    record.target(),
                    message,
                );
                if stream.write_all(line.as_bytes()).is_err() {
                    // Connection lost — drop it, fall back to os_log + stderr
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

/// Try to connect to the log collector running on the developer's Mac.
/// The address is baked in at build time via the GPUI_LOG_RELAY env var
/// (set automatically by build-rust.sh). If the collector isn't running
/// or the env var isn't set, this is a silent no-op.
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
        Err(_) => {
            // Collector not running — that's fine, os_log + stderr still work
        }
    }
}

fn init_logging(subsystem: &str) {
    // Connect TCP relay before installing the logger so the first log
    // message can already flow over the network.
    try_connect_log_relay();

    let logger = IosLogger::new(subsystem);
    log::set_boxed_logger(Box::new(logger)).expect("failed to set logger");
    log::set_max_level(LevelFilter::Debug);
}

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
            .bg(gpui::rgb(0x1e1e2e))
            .child(
                div()
                    .w(px(200.0))
                    .h(px(80.0))
                    .bg(gpui::rgb(0xf38ba8))
                    .rounded(px(12.0)),
            )
            .child(
                div()
                    .w(px(200.0))
                    .h(px(80.0))
                    .bg(gpui::rgb(0xa6e3a1))
                    .rounded(px(12.0)),
            )
            .child(
                div()
                    .w(px(200.0))
                    .h(px(80.0))
                    .bg(gpui::rgb(0x89b4fa))
                    .rounded(px(12.0)),
            )
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_run_hello_world() {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }

    // Triple-output logger: os_log + stderr + TCP relay to developer's Mac.
    init_logging("dev.glasshq.GPUIiOSHello");

    // Panic hook: log via the logger, then write backup file.
    std::panic::set_hook(Box::new(|info| {
        log::error!("[GPUI-iOS] PANIC: {}", info);
        let home = std::env::var("HOME").unwrap_or_default();
        let path = format!("{}/Documents/gpui_panic.log", home);
        let _ = std::fs::write(&path, format!("{}", info));
    }));

    log::info!("[GPUI-iOS] launching app");

    let app = Application::new();
    // Keep one cloned handle alive for the lifetime of the process since iOS
    // platform `run` is non-blocking.
    let keepalive = app.clone();
    let _ = Box::leak(Box::new(keepalive));

    app.run(|cx: &mut App| {
        cx.open_window(WindowOptions::default(), |_, cx| cx.new(|_| IosHelloWorld))
            .expect("failed to open GPUI iOS window");
        cx.activate(true);
    });
}
