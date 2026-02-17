use gpui::{
    App, Application, Bounds, Context, TabSelectEvent, Window, WindowAppearance, WindowBounds,
    WindowOptions, div, native_tab_view, prelude::*, px, rgb, size,
};

struct TabViewExample {
    selected: usize,
}

impl TabViewExample {
    const LABELS: [&str; 5] = ["Overview", "Apps", "Settings", "Billing", "Logs"];
}

impl Render for TabViewExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (bg, fg, muted) = if is_dark {
            (rgb(0x181c22), rgb(0xffffff), rgb(0xb5bdcb))
        } else {
            (rgb(0xf4f7fb), rgb(0x1a2433), rgb(0x616b7b))
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .gap_3()
            .p_4()
            .bg(bg)
            .text_color(fg)
            .child(div().text_xl().child("Native TabView"))
            .child(
                native_tab_view("tabs", &Self::LABELS)
                    .selected_index(self.selected)
                    .on_select(cx.listener(|this, event: &TabSelectEvent, _, cx| {
                        this.selected = event.index;
                        cx.notify();
                    }))
                    .h(px(300.0)),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(format!("Selected: {}", Self::LABELS[self.selected])),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(680.), px(500.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| TabViewExample { selected: 0 }),
        )
        .unwrap();

        cx.activate(true);
    });
}
