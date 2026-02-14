use gpui::{
    App, Application, Bounds, Context, StepperChangeEvent, Window, WindowAppearance, WindowBounds,
    WindowOptions, div, native_stepper, prelude::*, px, rgb, size,
};

struct StepperExample {
    retries: f64,
    timeout_seconds: f64,
}

impl Render for StepperExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (bg, fg, muted) = if is_dark {
            (rgb(0x202226), rgb(0xffffff), rgb(0xb9bec7))
        } else {
            (rgb(0xf5f6f8), rgb(0x1b1e25), rgb(0x5f6775))
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
            .child(div().text_xl().child("Native Stepper Demo"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(format!("Retries: {:.0}", self.retries))
                    .child(
                        native_stepper("retries")
                            .range(0.0, 10.0)
                            .increment(1.0)
                            .value(self.retries)
                            .on_change(cx.listener(|this, event: &StepperChangeEvent, _, cx| {
                                this.retries = event.value;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(format!("Timeout: {:.0}s", self.timeout_seconds))
                    .child(
                        native_stepper("timeout")
                            .range(5.0, 120.0)
                            .increment(5.0)
                            .value(self.timeout_seconds)
                            .on_change(cx.listener(|this, event: &StepperChangeEvent, _, cx| {
                                this.timeout_seconds = event.value;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Use the steppers to tune integer-like numeric settings."),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(560.), px(320.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| StepperExample {
                    retries: 3.0,
                    timeout_seconds: 30.0,
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
