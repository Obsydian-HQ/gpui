use gpui::{
    App, Application, Bounds, Context, SliderChangeEvent, Window, WindowAppearance, WindowBounds,
    WindowOptions, div, native_slider, prelude::*, px, rgb, size,
};

struct SliderExample {
    volume: f64,
    zoom: f64,
}

impl Render for SliderExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (bg, fg, muted) = if is_dark {
            (rgb(0x1e1e1e), rgb(0xffffff), rgb(0xbdbdbd))
        } else {
            (rgb(0xf0f0f0), rgb(0x1a1a1a), rgb(0x666666))
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
            .child(div().text_xl().child("Native Slider Demo"))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .items_center()
                    .child(format!("Volume: {:.0}", self.volume))
                    .child(
                        native_slider("volume")
                            .range(0.0, 100.0)
                            .value(self.volume)
                            .tick_marks(6)
                            .snap_to_ticks(true)
                            .on_change(cx.listener(|this, event: &SliderChangeEvent, _, cx| {
                                this.volume = event.value;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .items_center()
                    .child(format!("Zoom: {:.2}x", self.zoom))
                    .child(
                        native_slider("zoom")
                            .range(0.5, 2.0)
                            .value(self.zoom)
                            .continuous(true)
                            .on_change(cx.listener(|this, event: &SliderChangeEvent, _, cx| {
                                this.zoom = event.value;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Volume snaps to ticks; zoom is continuous."),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(560.), px(360.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| SliderExample {
                    volume: 40.0,
                    zoom: 1.0,
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
