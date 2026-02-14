use gpui::{
    App, Application, Bounds, Context, SwitchChangeEvent, Window, WindowAppearance, WindowBounds,
    WindowOptions, div, native_switch, prelude::*, px, rgb, size,
};

struct SwitchExample {
    auto_update: bool,
    telemetry: bool,
}

impl Render for SwitchExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (bg, fg, muted) = if is_dark {
            (rgb(0x1d1f23), rgb(0xffffff), rgb(0xb9bec7))
        } else {
            (rgb(0xf6f7f9), rgb(0x1a1f2a), rgb(0x5f6673))
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
            .child(div().text_xl().child("Native Switch Demo"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child("Auto-update")
                    .child(
                        native_switch("auto_update")
                            .checked(self.auto_update)
                            .on_change(cx.listener(|this, event: &SwitchChangeEvent, _, cx| {
                                this.auto_update = event.checked;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child("Share telemetry")
                    .child(
                        native_switch("telemetry")
                            .checked(self.telemetry)
                            .on_change(cx.listener(|this, event: &SwitchChangeEvent, _, cx| {
                                this.telemetry = event.checked;
                                cx.notify();
                            })),
                    ),
            )
            .child(div().text_sm().text_color(muted).child(format!(
                "Auto-update: {} | Telemetry: {}",
                self.auto_update, self.telemetry
            )))
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(540.), px(300.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| SwitchExample {
                    auto_update: true,
                    telemetry: false,
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
