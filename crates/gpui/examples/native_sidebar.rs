use gpui::{
    App, Application, Bounds, Context, Window, WindowAppearance, WindowBounds, WindowOptions, div,
    prelude::*, px, rgb, size,
};

struct NativeSidebarExample;

fn button(text: &str, on_click: impl Fn(&mut Window, &mut App) + 'static) -> impl IntoElement {
    div()
        .id(text.to_string())
        .px_3()
        .py_1()
        .border_1()
        .rounded_sm()
        .border_color(rgb(0xc9c9c9))
        .bg(rgb(0xffffff))
        .hover(|style| style.bg(rgb(0xf4f4f4)))
        .active(|style| style.bg(rgb(0xe7e7e7)))
        .cursor_pointer()
        .child(text.to_string())
        .on_click(move |_, window, cx| {
            on_click(window, cx);
            window.refresh();
        })
}

impl Render for NativeSidebarExample {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (bg, fg, muted, panel_bg) = if is_dark {
            (rgb(0x202124), rgb(0xf1f3f4), rgb(0xa8adb2), rgb(0x2a2c2f))
        } else {
            (rgb(0xf7f7f7), rgb(0x121417), rgb(0x5f6368), rgb(0xffffff))
        };

        let collapsed = window.is_native_sidebar_collapsed().unwrap_or(false);
        let width_probe_available = window.native_sidebar_width().is_some();

        div().size_full().bg(bg).text_color(fg).p_4().child(
            div()
                .size_full()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_lg().child("Native macOS Sidebar"))
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child("AppKit NSSplitViewController sidebarWithViewController binding"),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .items_center()
                        .child(button("Toggle", |window, _| {
                            window.toggle_native_sidebar();
                        }))
                        .child(button("Collapse", |window, _| {
                            window.set_native_sidebar_collapsed(true);
                        }))
                        .child(button("Expand", |window, _| {
                            window.set_native_sidebar_collapsed(false);
                        }))
                        .child(button("Narrower", |window, _| {
                            window.set_native_sidebar_width(px(180.0));
                        }))
                        .child(button("Wider", |window, _| {
                            window.set_native_sidebar_width(px(320.0));
                        })),
                )
                .child(
                    div()
                        .bg(panel_bg)
                        .border_1()
                        .border_color(rgb(0xd8d8d8))
                        .rounded_md()
                        .p_3()
                        .child(format!(
                            "Sidebar collapsed: {}  |  Width probe available: {}",
                            collapsed, width_probe_available
                        )),
                ),
        )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(980.), px(620.)), cx);
        if let Err(err) = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let _ = window.install_native_sidebar(
                    px(160.),
                    px(240.),
                    px(420.),
                    Some("gpui.native_sidebar.example"),
                );
                cx.new(|_| NativeSidebarExample)
            },
        ) {
            eprintln!("failed to open native_sidebar window: {err:#}");
            return;
        }
        cx.activate(true);
    });
}
