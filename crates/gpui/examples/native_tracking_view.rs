use gpui::{
    App, Application, Bounds, Context, Window, WindowAppearance, WindowBounds, WindowOptions, div,
    native_tracking_view, prelude::*, px, rgb, size,
};

struct TrackingViewExample {
    hovered: Option<String>,
}

impl Render for TrackingViewExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let fg = if is_dark { rgb(0xffffff) } else { rgb(0x1a1a1a) };
        let muted = if is_dark { rgb(0x999999) } else { rgb(0x666666) };

        let colors = [
            ("Red", rgb(0xcc3333)),
            ("Green", rgb(0x33cc33)),
            ("Blue", rgb(0x3333cc)),
            ("Yellow", rgb(0xcccc33)),
        ];

        let hover_label = self
            .hovered
            .as_deref()
            .unwrap_or("None");

        let mut row = div().flex().flex_row().gap_4().justify_center();
        for (idx, (name, color)) in colors.iter().enumerate() {
            let name_str = name.to_string();
            let name_enter = name_str.clone();
            let is_hovered = self.hovered.as_deref() == Some(*name);
            let opacity = if is_hovered { 1.0 } else { 0.6 };

            row = row.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .relative()
                            .w(px(100.0))
                            .h(px(100.0))
                            .rounded(px(8.0))
                            .bg(*color)
                            .opacity(opacity)
                            .child(
                                native_tracking_view(format!("track-{idx}"))
                                    .on_mouse_enter(cx.listener(move |this, _event, _window, cx| {
                                        this.hovered = Some(name_enter.clone());
                                        cx.notify();
                                    }))
                                    .on_mouse_exit(cx.listener(|this, _event, _window, cx| {
                                        this.hovered = None;
                                        cx.notify();
                                    }))
                                    .w(px(100.0))
                                    .h(px(100.0)),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(name_str),
                    ),
            );
        }

        div()
            .flex()
            .flex_col()
            .size_full()
            .justify_center()
            .items_center()
            .gap_6()
            .text_color(fg)
            .child(div().text_lg().child("NSTrackingArea — Hover Detection"))
            .child(row)
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(format!("Hovered: {hover_label}")),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(550.), px(350.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| TrackingViewExample { hovered: None }),
        )
        .unwrap();
        cx.activate(true);
    });
}
