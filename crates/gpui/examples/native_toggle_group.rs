use gpui::{
    App, Application, Bounds, Context, NativeSegmentedShape, NativeSegmentedSize,
    SegmentSelectEvent, Window, WindowAppearance, WindowBounds, WindowOptions, div,
    native_toggle_group, prelude::*, px, rgb, size,
};

struct ToggleGroupExample {
    view_mode: usize,
    sort_order: usize,
    shape_index: usize,
    size_index: usize,
    nav_selection: Option<usize>,
}

impl ToggleGroupExample {
    const VIEW_MODES: [&str; 3] = ["List", "Grid", "Gallery"];
    const SORT_ORDERS: [&str; 3] = ["Name", "Date", "Size"];
    const SHAPE_NAMES: [&str; 4] = ["Automatic", "Capsule", "RoundedRect", "Circle"];
    const SIZE_NAMES: [&str; 5] = ["Mini", "Small", "Regular", "Large", "ExtraLarge"];
    const NAV_LABELS: [&str; 4] = ["Files", "Git", "Debug", "Settings"];
    const NAV_ICONS: [&str; 4] = [
        "folder",
        "arrow.triangle.branch",
        "ant",
        "gearshape",
    ];
}

impl Render for ToggleGroupExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (bg, fg, muted) = if is_dark {
            (rgb(0x1e1e1e), rgb(0xffffff), rgb(0x8e8e93))
        } else {
            (rgb(0xf0f0f0), rgb(0x1a1a1a), rgb(0x86868b))
        };

        let border_shape = match self.shape_index {
            0 => NativeSegmentedShape::Automatic,
            1 => NativeSegmentedShape::Capsule,
            2 => NativeSegmentedShape::RoundedRectangle,
            3 => NativeSegmentedShape::Circle,
            _ => NativeSegmentedShape::Automatic,
        };

        let control_size = match self.size_index {
            0 => NativeSegmentedSize::Mini,
            1 => NativeSegmentedSize::Small,
            2 => NativeSegmentedSize::Regular,
            3 => NativeSegmentedSize::Large,
            4 => NativeSegmentedSize::ExtraLarge,
            _ => NativeSegmentedSize::Regular,
        };

        let nav_label = self
            .nav_selection
            .map(|i| Self::NAV_LABELS[i])
            .unwrap_or("None");

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
                "View: {}  |  Sort: {}  |  Nav: {}",
                Self::VIEW_MODES[self.view_mode],
                Self::SORT_ORDERS[self.sort_order],
                nav_label,
            ))
            .child(format!(
                "Shape: {}  |  Size: {}",
                Self::SHAPE_NAMES[self.shape_index],
                Self::SIZE_NAMES[self.size_index],
            ))
            // SF Symbols navigation (icon-only segments)
            .child(
                div()
                    .flex()
                    .gap_3()
                    .items_center()
                    .child("Nav:")
                    .child({
                        let mut group =
                            native_toggle_group("nav_icons", &Self::NAV_LABELS)
                                .sf_symbols(&Self::NAV_ICONS)
                                .on_select(cx.listener(|this, event: &SegmentSelectEvent, _, cx| {
                                    this.nav_selection = Some(event.index);
                                    cx.notify();
                                }));
                        if let Some(index) = self.nav_selection {
                            group = group.selected_index(index);
                        }
                        group
                    }),
            )
            // View mode — uses the dynamic shape/size from pickers
            .child(
                div()
                    .flex()
                    .gap_3()
                    .items_center()
                    .child("View:")
                    .child(
                        native_toggle_group("view_mode", &Self::VIEW_MODES)
                            .selected_index(self.view_mode)
                            .border_shape(border_shape)
                            .control_size(control_size)
                            .on_select(cx.listener(|this, event: &SegmentSelectEvent, _, cx| {
                                this.view_mode = event.index;
                                cx.notify();
                            })),
                    ),
            )
            // Sort order
            .child(
                div()
                    .flex()
                    .gap_3()
                    .items_center()
                    .child("Sort:")
                    .child(
                        native_toggle_group("sort_order", &Self::SORT_ORDERS)
                            .selected_index(self.sort_order)
                            .on_select(cx.listener(|this, event: &SegmentSelectEvent, _, cx| {
                                this.sort_order = event.index;
                                cx.notify();
                            })),
                    ),
            )
            // Shape picker
            .child(
                div()
                    .flex()
                    .gap_3()
                    .items_center()
                    .child(
                        div()
                            .text_color(muted)
                            .text_sm()
                            .child("Shape:"),
                    )
                    .child(
                        native_toggle_group("shape_selector", &Self::SHAPE_NAMES)
                            .selected_index(self.shape_index)
                            .on_select(cx.listener(|this, event: &SegmentSelectEvent, _, cx| {
                                this.shape_index = event.index;
                                cx.notify();
                            })),
                    ),
            )
            // Size picker
            .child(
                div()
                    .flex()
                    .gap_3()
                    .items_center()
                    .child(
                        div()
                            .text_color(muted)
                            .text_sm()
                            .child("Size:"),
                    )
                    .child(
                        native_toggle_group("size_selector", &Self::SIZE_NAMES)
                            .selected_index(self.size_index)
                            .on_select(cx.listener(|this, event: &SegmentSelectEvent, _, cx| {
                                this.size_index = event.index;
                                cx.notify();
                            })),
                    ),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(650.), px(450.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| ToggleGroupExample {
                    view_mode: 0,
                    sort_order: 0,
                    shape_index: 0,
                    size_index: 2, // Regular
                    nav_selection: Some(0),
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
