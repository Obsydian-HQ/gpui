use gpui::{
    App, Application, Bounds, Context, Window, WindowAppearance, WindowBounds, WindowOptions, div,
    native_button, prelude::*, px, rgb, size,
};

struct NativeButtonExample {
    count: usize,
}

impl Render for NativeButtonExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (bg, fg) = if is_dark {
            (rgb(0x2e2e2e), rgb(0xffffff))
        } else {
            (rgb(0xececec), rgb(0x1a1a1a))
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .justify_center()
            .items_center()
            .gap_4()
            .bg(bg)
            .text_xl()
            .text_color(fg)
            .child(format!("Count: {}", self.count))
            .child(
                native_button("increment", "Increment")
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.count += 1;
                        cx.notify();
                    })),
            )
            .child(
                native_button("reset", "Reset")
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.count = 0;
                        cx.notify();
                    })),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(400.), px(300.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| NativeButtonExample { count: 0 }),
        )
        .unwrap();
        cx.activate(true);
    });
}
