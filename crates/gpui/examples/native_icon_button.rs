use gpui::{
    App, Application, Bounds, Context, NativeButtonStyle, NativeButtonTint, Window,
    WindowAppearance, WindowBounds, WindowOptions, div, native_icon_button, prelude::*, px, rgb,
    size,
};

struct IconButtonExample {
    last_action: String,
    starred: bool,
}

impl Render for IconButtonExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (bg, fg) = if is_dark {
            (rgb(0x1e1e1e), rgb(0xffffff))
        } else {
            (rgb(0xf0f0f0), rgb(0x1a1a1a))
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .justify_center()
            .items_center()
            .gap_4()
            .bg(bg)
            .text_color(fg)
            .child(format!("Action: {}", self.last_action))
            .child(format!("Starred: {}", self.starred))
            // Row 1: Common icon buttons (borderless)
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .child("Borderless:")
                    .child(
                        native_icon_button("gear", "gear")
                            .tooltip("Settings")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.last_action = "Settings".into();
                                cx.notify();
                            })),
                    )
                    .child(
                        native_icon_button("plus", "plus")
                            .tooltip("Add")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.last_action = "Add".into();
                                cx.notify();
                            })),
                    )
                    .child(
                        native_icon_button("trash", "trash")
                            .tooltip("Delete")
                            .tint(NativeButtonTint::Destructive)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.last_action = "Delete".into();
                                cx.notify();
                            })),
                    )
                    .child(
                        native_icon_button("search", "magnifyingglass")
                            .tooltip("Search")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.last_action = "Search".into();
                                cx.notify();
                            })),
                    )
                    .child({
                        let symbol = if self.starred { "star.fill" } else { "star" };
                        native_icon_button("star", symbol)
                            .tooltip("Toggle Star")
                            .tint(NativeButtonTint::Warning)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.starred = !this.starred;
                                this.last_action = "Star toggled".into();
                                cx.notify();
                            }))
                    }),
            )
            // Row 2: Rounded icon buttons
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .child("Rounded:")
                    .child(
                        native_icon_button("folder_r", "folder")
                            .button_style(NativeButtonStyle::Rounded)
                            .tooltip("Open Folder")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.last_action = "Folder".into();
                                cx.notify();
                            })),
                    )
                    .child(
                        native_icon_button("bell_r", "bell")
                            .button_style(NativeButtonStyle::Rounded)
                            .tooltip("Notifications")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.last_action = "Notifications".into();
                                cx.notify();
                            })),
                    )
                    .child(
                        native_icon_button("share_r", "square.and.arrow.up")
                            .button_style(NativeButtonStyle::Rounded)
                            .tooltip("Share")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.last_action = "Share".into();
                                cx.notify();
                            })),
                    ),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(600.), px(300.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| IconButtonExample {
                    last_action: "(none)".into(),
                    starred: false,
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
