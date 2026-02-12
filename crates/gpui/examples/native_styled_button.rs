use gpui::{
    App, Application, Bounds, Context, NativeButtonStyle, NativeButtonTint, Window,
    WindowAppearance, WindowBounds, WindowOptions, div, native_button, prelude::*, px, rgb, size,
};

struct StyledButtonExample {
    last_clicked: String,
}

impl Render for StyledButtonExample {
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
            .child(format!("Last clicked: {}", self.last_clicked))
            // Row 1: Button styles
            .child(
                div()
                    .flex()
                    .gap_3()
                    .items_center()
                    .child("Styles:")
                    .child(
                        native_button("rounded", "Rounded")
                            .button_style(NativeButtonStyle::Rounded)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.last_clicked = "Rounded".into();
                                cx.notify();
                            })),
                    )
                    .child(
                        native_button("filled", "Filled")
                            .button_style(NativeButtonStyle::Filled)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.last_clicked = "Filled".into();
                                cx.notify();
                            })),
                    )
                    .child(
                        native_button("inline", "Inline")
                            .button_style(NativeButtonStyle::Inline)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.last_clicked = "Inline".into();
                                cx.notify();
                            })),
                    )
                    .child(
                        native_button("borderless", "Borderless")
                            .button_style(NativeButtonStyle::Borderless)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.last_clicked = "Borderless".into();
                                cx.notify();
                            })),
                    ),
            )
            // Row 2: Tinted buttons
            .child(
                div()
                    .flex()
                    .gap_3()
                    .items_center()
                    .child("Tints:")
                    .child(
                        native_button("accent", "Accent")
                            .button_style(NativeButtonStyle::Filled)
                            .tint(NativeButtonTint::Accent)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.last_clicked = "Accent".into();
                                cx.notify();
                            })),
                    )
                    .child(
                        native_button("destructive", "Destructive")
                            .button_style(NativeButtonStyle::Filled)
                            .tint(NativeButtonTint::Destructive)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.last_clicked = "Destructive".into();
                                cx.notify();
                            })),
                    )
                    .child(
                        native_button("warning", "Warning")
                            .button_style(NativeButtonStyle::Filled)
                            .tint(NativeButtonTint::Warning)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.last_clicked = "Warning".into();
                                cx.notify();
                            })),
                    )
                    .child(
                        native_button("success", "Success")
                            .button_style(NativeButtonStyle::Filled)
                            .tint(NativeButtonTint::Success)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.last_clicked = "Success".into();
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
                cx.new(|_| StyledButtonExample {
                    last_clicked: "(none)".into(),
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
