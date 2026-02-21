use gpui::{
    App, Application, Bounds, Context, NativeStackDistribution, NativeStackOrientation, Window,
    WindowAppearance, WindowBounds, WindowOptions, div, native_button, native_stack_view,
    prelude::*, px, rgb, size,
};

struct StackViewExample {
    horizontal_count: usize,
    vertical_count: usize,
}

impl Render for StackViewExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let fg = if is_dark { rgb(0xffffff) } else { rgb(0x1a1a1a) };
        let muted = if is_dark { rgb(0x999999) } else { rgb(0x666666) };

        div()
            .flex()
            .flex_col()
            .size_full()
            .justify_center()
            .items_center()
            .gap_6()
            .text_color(fg)
            .child(div().text_lg().child("NSStackView Layouts"))
            // Horizontal stack with buttons
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(format!(
                                "Horizontal — EqualSpacing ({} buttons)",
                                self.horizontal_count
                            )),
                    )
                    .child(
                        div().flex().flex_row().gap_2().children(
                            (0..self.horizontal_count).map(|i| {
                                native_button(format!("h-btn-{i}"), format!("Item {}", i + 1))
                                    .w(px(80.0))
                                    .h(px(24.0))
                            }),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(
                                native_button("h-add", "+ Add")
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.horizontal_count += 1;
                                        cx.notify();
                                    }))
                                    .w(px(60.0))
                                    .h(px(22.0)),
                            )
                            .child(
                                native_button("h-remove", "- Remove")
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        if this.horizontal_count > 0 {
                                            this.horizontal_count -= 1;
                                        }
                                        cx.notify();
                                    }))
                                    .w(px(70.0))
                                    .h(px(22.0)),
                            ),
                    ),
            )
            // Underlying NSStackView element (showing it can be positioned/sized)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child("NSStackView container (native, empty — for composition with FFI)"),
                    )
                    .child(
                        div()
                            .border_1()
                            .border_color(muted)
                            .rounded(px(4.0))
                            .child(
                                native_stack_view("h-stack", NativeStackOrientation::Horizontal)
                                    .spacing(8.0)
                                    .distribution(NativeStackDistribution::EqualSpacing)
                                    .w(px(400.0))
                                    .h(px(40.0)),
                            ),
                    ),
            )
            // Vertical layout demo using GPUI flex (showing the concept)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(format!("Vertical buttons ({})", self.vertical_count)),
                    )
                    .child(
                        div().flex().flex_col().gap_1().children(
                            (0..self.vertical_count).map(|i| {
                                native_button(format!("v-btn-{i}"), format!("Row {}", i + 1))
                                    .w(px(120.0))
                                    .h(px(24.0))
                            }),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(
                                native_button("v-add", "+ Add")
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.vertical_count += 1;
                                        cx.notify();
                                    }))
                                    .w(px(60.0))
                                    .h(px(22.0)),
                            )
                            .child(
                                native_button("v-remove", "- Remove")
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        if this.vertical_count > 0 {
                                            this.vertical_count -= 1;
                                        }
                                        cx.notify();
                                    }))
                                    .w(px(70.0))
                                    .h(px(22.0)),
                            ),
                    ),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(550.), px(500.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| StackViewExample {
                    horizontal_count: 3,
                    vertical_count: 2,
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
