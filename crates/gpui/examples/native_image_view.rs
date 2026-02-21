use gpui::{
    App, Application, Bounds, Context, NativeImageSymbolWeight, Window, WindowAppearance,
    WindowBounds, WindowOptions, div, native_image_view, prelude::*, px, rgb, size,
};

struct ImageViewExample;

impl Render for ImageViewExample {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let fg = if is_dark { rgb(0xffffff) } else { rgb(0x1a1a1a) };
        let muted = if is_dark { rgb(0x999999) } else { rgb(0x666666) };

        let symbols = [
            ("globe", 0.0, 0.478, 1.0),
            ("star.fill", 1.0, 0.8, 0.0),
            ("folder.fill", 0.0, 0.7, 0.3),
            ("doc.text.fill", 0.6, 0.4, 0.8),
            ("xmark.circle.fill", 1.0, 0.3, 0.3),
        ];

        let mut row1 = div().flex().flex_row().gap_6().justify_center();
        for (idx, (name, r, g, b)) in symbols.iter().enumerate() {
            row1 = row1.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .child(
                        native_image_view(format!("sym-{idx}"))
                            .sf_symbol(*name)
                            .tint_color(*r, *g, *b, 1.0)
                            .w(px(32.0))
                            .h(px(32.0)),
                    )
                    .child(div().text_xs().text_color(muted).child(name.to_string())),
            );
        }

        let sizes: [(f64, NativeImageSymbolWeight); 4] = [
            (14.0, NativeImageSymbolWeight::Light),
            (20.0, NativeImageSymbolWeight::Regular),
            (28.0, NativeImageSymbolWeight::Semibold),
            (36.0, NativeImageSymbolWeight::Heavy),
        ];

        let mut row2 = div().flex().flex_row().gap_6().items_end().justify_center();
        for (idx, (pt_size, weight)) in sizes.iter().enumerate() {
            let dim = *pt_size as f32 + 8.0;
            row2 = row2.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .child(
                        native_image_view(format!("sized-{idx}"))
                            .sf_symbol_config("heart.fill", *pt_size, *weight)
                            .tint_color(1.0, 0.3, 0.5, 1.0)
                            .w(px(dim))
                            .h(px(dim)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(format!("{pt_size}pt")),
                    ),
            );
        }

        div()
            .flex()
            .flex_col()
            .size_full()
            .justify_center()
            .items_center()
            .gap_8()
            .text_color(fg)
            .child(div().text_lg().child("NSImageView — SF Symbols"))
            .child(row1)
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Different sizes & weights:"),
            )
            .child(row2)
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
            |_, cx| cx.new(|_| ImageViewExample),
        )
        .unwrap();
        cx.activate(true);
    });
}
