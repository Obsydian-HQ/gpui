use gpui::{
    App, Application, Bounds, Context, NativeSegmentedStyle, SegmentSelectEvent, Window,
    WindowAppearance, WindowBounds, WindowOptions, div, native_toggle_group, prelude::*, px, rgb,
    size,
};

struct ToggleGroupExample {
    view_mode: usize,
    sort_order: usize,
    style_index: usize,
}

impl ToggleGroupExample {
    const VIEW_MODES: [&str; 3] = ["List", "Grid", "Gallery"];
    const SORT_ORDERS: [&str; 3] = ["Name", "Date", "Size"];
    const STYLE_NAMES: [&str; 5] = ["Automatic", "Rounded", "RoundRect", "Capsule", "Separated"];
}

impl Render for ToggleGroupExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (bg, fg) = if is_dark {
            (rgb(0x1e1e1e), rgb(0xffffff))
        } else {
            (rgb(0xf0f0f0), rgb(0x1a1a1a))
        };

        let segmented_style = match self.style_index {
            0 => NativeSegmentedStyle::Automatic,
            1 => NativeSegmentedStyle::Rounded,
            2 => NativeSegmentedStyle::RoundRect,
            3 => NativeSegmentedStyle::Capsule,
            4 => NativeSegmentedStyle::Separated,
            _ => NativeSegmentedStyle::Automatic,
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
            .child(format!(
                "View: {}  |  Sort: {}",
                Self::VIEW_MODES[self.view_mode], Self::SORT_ORDERS[self.sort_order]
            ))
            // View mode selector
            .child(
                div()
                    .flex()
                    .gap_3()
                    .items_center()
                    .child("View:")
                    .child(
                        native_toggle_group("view_mode", &Self::VIEW_MODES)
                            .selected_index(self.view_mode)
                            .segment_style(segmented_style)
                            .on_select(cx.listener(|this, event: &SegmentSelectEvent, _, cx| {
                                this.view_mode = event.index;
                                cx.notify();
                            })),
                    ),
            )
            // Sort order selector
            .child(
                div()
                    .flex()
                    .gap_3()
                    .items_center()
                    .child("Sort:")
                    .child(
                        native_toggle_group("sort_order", &Self::SORT_ORDERS)
                            .selected_index(self.sort_order)
                            .segment_style(segmented_style)
                            .on_select(cx.listener(|this, event: &SegmentSelectEvent, _, cx| {
                                this.sort_order = event.index;
                                cx.notify();
                            })),
                    ),
            )
            // Style selector (segmented control to change the style of the other controls)
            .child(
                div()
                    .flex()
                    .gap_3()
                    .items_center()
                    .child("Style:")
                    .child(
                        native_toggle_group("style_selector", &Self::STYLE_NAMES)
                            .selected_index(self.style_index)
                            .on_select(cx.listener(|this, event: &SegmentSelectEvent, _, cx| {
                                this.style_index = event.index;
                                cx.notify();
                            })),
                    ),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(600.), px(350.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| ToggleGroupExample {
                    view_mode: 0,
                    sort_order: 0,
                    style_index: 0,
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
