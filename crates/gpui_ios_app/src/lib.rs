use gpui::{App, Application, Context, Window, WindowOptions, div, prelude::*};
use std::sync::atomic::{AtomicBool, Ordering};

static STARTED: AtomicBool = AtomicBool::new(false);

struct IosHelloWorld;

impl Render for IosHelloWorld {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgb(0x101a2b))
            .text_color(gpui::rgb(0xffffff))
            .text_2xl()
            .child("Hello from GPUI iOS")
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_run_hello_world() {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }

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
