use gpui::{
    App, Application, Bounds, Context, NativeTextFieldStyle, Window, WindowAppearance,
    WindowBounds, WindowOptions, div, native_button, native_text_field, prelude::*, px, rgb, size,
};

struct TextFieldExample {
    basic_text: String,
    password_text: String,
    submitted_text: String,
}

impl Render for TextFieldExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (bg, fg, muted) = if is_dark {
            (rgb(0x2e2e2e), rgb(0xffffff), rgb(0x999999))
        } else {
            (rgb(0xececec), rgb(0x1a1a1a), rgb(0x666666))
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
            .child(
                div()
                    .text_xl()
                    .child("Native Text Field Demo"),
            )
            // Basic text field
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .items_center()
                    .child(
                        native_text_field("basic")
                            .placeholder("Type something...")
                            .on_change(cx.listener(|this, event: &gpui::TextChangeEvent, _window, cx| {
                                this.basic_text = event.text.clone();
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(format!("Text: {}", self.basic_text)),
                    ),
            )
            // Password field
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .items_center()
                    .child(
                        native_text_field("password")
                            .placeholder("Enter password...")
                            .secure(true)
                            .on_change(cx.listener(|this, event: &gpui::TextChangeEvent, _window, cx| {
                                this.password_text = event.text.clone();
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(format!("Password length: {}", self.password_text.len())),
                    ),
            )
            // Rounded style text field with submit
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .items_center()
                    .child(
                        native_text_field("search")
                            .placeholder("Search (press Enter)...")
                            .field_style(NativeTextFieldStyle::Rounded)
                            .on_submit(cx.listener(|this, event: &gpui::TextSubmitEvent, _window, cx| {
                                this.submitted_text = event.text.clone();
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(format!("Submitted: {}", self.submitted_text)),
                    ),
            )
            // Disabled text field
            .child(
                native_text_field("disabled")
                    .placeholder("Disabled field")
                    .disabled(true),
            )
            // Clear button
            .child(
                native_button("clear", "Clear All")
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.basic_text.clear();
                        this.password_text.clear();
                        this.submitted_text.clear();
                        cx.notify();
                    })),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(450.), px(400.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| TextFieldExample {
                    basic_text: String::new(),
                    password_text: String::new(),
                    submitted_text: String::new(),
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
