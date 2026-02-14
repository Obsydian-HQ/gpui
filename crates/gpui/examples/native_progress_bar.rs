use gpui::{
    App, Application, Bounds, Context, NativeProgressStyle, Window, WindowAppearance, WindowBounds,
    WindowOptions, div, native_button, native_progress_bar, prelude::*, px, rgb, size,
};

struct ProgressExample {
    progress: f64,
    indeterminate_bar: bool,
}

impl Render for ProgressExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (bg, fg, muted) = if is_dark {
            (rgb(0x1f1f1f), rgb(0xffffff), rgb(0xbdbdbd))
        } else {
            (rgb(0xf2f2f2), rgb(0x1a1a1a), rgb(0x666666))
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
            .child(div().text_xl().child("Native Progress Demo"))
            .child(format!("Determinate value: {:.0}%", self.progress))
            .child(
                native_progress_bar("determinate")
                    .range(0.0, 100.0)
                    .value(self.progress),
            )
            .child(
                native_progress_bar("bar_indeterminate")
                    .range(0.0, 100.0)
                    .maybe_value(if self.indeterminate_bar {
                        None
                    } else {
                        Some(self.progress)
                    }),
            )
            .child(
                div().flex().items_center().gap_3().child("Spinner:").child(
                    native_progress_bar("spinner")
                        .progress_style(NativeProgressStyle::Spinner)
                        .indeterminate(true),
                ),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(native_button("inc", "Increase").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.progress = (this.progress + 10.0).min(100.0);
                            cx.notify();
                        },
                    )))
                    .child(native_button("reset", "Reset").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.progress = 0.0;
                            cx.notify();
                        },
                    )))
                    .child(
                        native_button("toggle_indeterminate", "Toggle Indeterminate").on_click(
                            cx.listener(|this, _, _, cx| {
                                this.indeterminate_bar = !this.indeterminate_bar;
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .child(div().text_sm().text_color(muted).child(format!(
                "Bar mode: {}",
                if self.indeterminate_bar {
                    "Indeterminate"
                } else {
                    "Determinate"
                }
            )))
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(620.), px(420.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| ProgressExample {
                    progress: 30.0,
                    indeterminate_bar: false,
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
