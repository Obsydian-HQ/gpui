use gpui::{
    App, Application, Bounds, Context, NativeOutlineNode, OutlineRowSelectEvent, Window,
    WindowAppearance, WindowBounds, WindowOptions, div, native_outline_view, prelude::*, px, rgb,
    size,
};

struct OutlineViewExample {
    selected_label: String,
}

impl OutlineViewExample {
    fn nodes() -> Vec<NativeOutlineNode> {
        vec![
            NativeOutlineNode::branch(
                "Workspace",
                vec![
                    NativeOutlineNode::branch(
                        "Apps",
                        vec![
                            NativeOutlineNode::leaf("Glass"),
                            NativeOutlineNode::leaf("Settings"),
                            NativeOutlineNode::leaf("Dashboard"),
                        ],
                    ),
                    NativeOutlineNode::branch(
                        "Services",
                        vec![
                            NativeOutlineNode::leaf("Auth"),
                            NativeOutlineNode::leaf("Billing"),
                            NativeOutlineNode::leaf("Notifications"),
                        ],
                    ),
                ],
            ),
            NativeOutlineNode::branch(
                "Personal",
                vec![
                    NativeOutlineNode::leaf("Notes"),
                    NativeOutlineNode::leaf("Archive"),
                ],
            ),
        ]
    }
}

impl Render for OutlineViewExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (bg, fg, muted) = if is_dark {
            (rgb(0x171a21), rgb(0xffffff), rgb(0xb6bfce))
        } else {
            (rgb(0xf5f7fc), rgb(0x1b2434), rgb(0x606b7d))
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .gap_3()
            .p_4()
            .bg(bg)
            .text_color(fg)
            .child(div().text_xl().child("Native OutlineView (Tree)"))
            .child(
                native_outline_view("tree", &Self::nodes())
                    .expand_all(true)
                    .row_height(24.0)
                    .on_select(cx.listener(|this, event: &OutlineRowSelectEvent, _, cx| {
                        this.selected_label = event.title.to_string();
                        cx.notify();
                    }))
                    .h(px(320.0)),
            )
            .child(div().text_sm().text_color(muted).child(format!(
                "Selected: {}",
                if self.selected_label.is_empty() {
                    "<none>".to_string()
                } else {
                    self.selected_label.clone()
                }
            )))
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(720.), px(540.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| OutlineViewExample {
                    selected_label: String::new(),
                })
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
