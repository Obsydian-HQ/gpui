use gpui::{
    App, Application, Bounds, Context, SearchChangeEvent, SearchSubmitEvent, Window,
    WindowAppearance, WindowBounds, WindowOptions, div, native_search_field, prelude::*, px, rgb,
    size,
};

struct SearchFieldExample {
    query: String,
    submitted: String,
}

impl Render for SearchFieldExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (bg, fg, muted) = if is_dark {
            (rgb(0x1d2026), rgb(0xffffff), rgb(0xb9bec7))
        } else {
            (rgb(0xf4f6fa), rgb(0x1b2230), rgb(0x5f6978))
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
            .child(div().text_xl().child("Native Search Field Demo"))
            .child(
                native_search_field("search")
                    .placeholder("Search settings, files, or commands")
                    .value(self.query.clone())
                    .on_change(cx.listener(|this, event: &SearchChangeEvent, _, cx| {
                        this.query = event.text.clone();
                        cx.notify();
                    }))
                    .on_submit(cx.listener(|this, event: &SearchSubmitEvent, _, cx| {
                        this.submitted = event.text.clone();
                        cx.notify();
                    }))
                    .w(px(320.0)),
            )
            .child(div().text_sm().text_color(muted).child(format!(
                "Live: {}",
                if self.query.is_empty() {
                    "<empty>".to_string()
                } else {
                    self.query.clone()
                }
            )))
            .child(div().text_sm().text_color(muted).child(format!(
                "Last submitted: {}",
                if self.submitted.is_empty() {
                    "<none>".to_string()
                } else {
                    self.submitted.clone()
                }
            )))
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(620.), px(330.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| SearchFieldExample {
                    query: String::new(),
                    submitted: String::new(),
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
