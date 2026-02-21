/// Native Alert Example
///
/// Demonstrates NSAlert presented as a sheet (non-blocking) with different styles
/// (warning, informational, critical) and multiple buttons.
use gpui::{
    App, Application, Bounds, Context, NativeAlert, NativeAlertStyle, Window, WindowBounds,
    WindowOptions, div, prelude::*, px, rgb, size,
};

struct AlertExample {
    last_response: String,
}

impl AlertExample {
    fn new() -> Self {
        Self {
            last_response: "Click a button to show an alert".to_string(),
        }
    }

    fn show_alert(&mut self, alert: NativeAlert, labels: &'static [&'static str], window: &mut Window, cx: &mut Context<Self>) {
        if let Some(rx) = window.show_native_alert(alert) {
            cx.spawn(async move |this, cx| {
                if let Ok(button_index) = rx.await {
                    this.update(cx, |this: &mut Self, cx| {
                        this.last_response = if button_index < labels.len() {
                            format!("Clicked: {}", labels[button_index])
                        } else {
                            format!("Clicked button index: {}", button_index)
                        };
                        cx.notify();
                    })
                    .ok();
                }
            })
            .detach();
        }
    }

    fn show_warning(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let alert = NativeAlert::new("Unsaved Changes")
            .style(NativeAlertStyle::Warning)
            .informative_text("You have unsaved changes. Do you want to save before closing?")
            .button("Save")
            .button("Don't Save")
            .button("Cancel");
        self.show_alert(alert, &["Save", "Don't Save", "Cancel"], window, cx);
    }

    fn show_informational(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let alert = NativeAlert::new("Update Available")
            .style(NativeAlertStyle::Informational)
            .informative_text("A new version (2.0.0) is available. Would you like to update now?")
            .button("Update Now")
            .button("Later");
        self.show_alert(alert, &["Update Now", "Later"], window, cx);
    }

    fn show_critical(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let alert = NativeAlert::new("Delete Project?")
            .style(NativeAlertStyle::Critical)
            .informative_text(
                "This will permanently delete the project and all its files. This action cannot be undone.",
            )
            .button("Delete")
            .button("Cancel");
        self.show_alert(alert, &["Delete", "Cancel"], window, cx);
    }

    fn show_simple(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let alert = NativeAlert::new("Operation Complete")
            .informative_text("The build finished successfully with 0 errors and 3 warnings.")
            .button("OK");
        self.show_alert(alert, &["OK"], window, cx);
    }

    fn show_suppression(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let alert = NativeAlert::new("Enable Notifications?")
            .style(NativeAlertStyle::Informational)
            .informative_text("Allow this application to send you desktop notifications?")
            .button("Allow")
            .button("Don't Allow")
            .shows_suppression_button(true);
        self.show_alert(alert, &["Allow", "Don't Allow"], window, cx);
    }
}

impl Render for AlertExample {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1e1e2e))
            .p(px(24.0))
            .gap(px(12.0))
            .child(
                div()
                    .text_color(rgb(0xcdd6f4))
                    .text_xl()
                    .child("Native Alert Example"),
            )
            .child(
                div()
                    .text_color(rgb(0xa6adc8))
                    .child(self.last_response.clone()),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(make_button(
                        "Warning Alert (Save Changes)",
                        0xf9e2af,
                        0x1e1e2e,
                        cx.listener(|this, _, window, cx| this.show_warning(window, cx)),
                    ))
                    .child(make_button(
                        "Informational Alert (Update)",
                        0x89b4fa,
                        0x1e1e2e,
                        cx.listener(|this, _, window, cx| this.show_informational(window, cx)),
                    ))
                    .child(make_button(
                        "Critical Alert (Delete)",
                        0xf38ba8,
                        0x1e1e2e,
                        cx.listener(|this, _, window, cx| this.show_critical(window, cx)),
                    ))
                    .child(make_button(
                        "Simple Alert (OK)",
                        0x45475a,
                        0xcdd6f4,
                        cx.listener(|this, _, window, cx| this.show_simple(window, cx)),
                    ))
                    .child(make_button(
                        "Alert with Suppression Checkbox",
                        0x45475a,
                        0xcdd6f4,
                        cx.listener(|this, _, window, cx| this.show_suppression(window, cx)),
                    )),
            )
    }
}

fn make_button(
    label: &'static str,
    bg_color: u32,
    text_color: u32,
    handler: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(label)
        .px(px(12.0))
        .py(px(8.0))
        .bg(rgb(bg_color))
        .rounded(px(6.0))
        .text_color(rgb(text_color))
        .cursor_pointer()
        .on_click(handler)
        .child(label)
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(500.0), px(400.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| AlertExample::new()),
        )
        .ok();
    });
}
