use gpui::{
    App, Application, Bounds, CollectionSelectEvent, Context, NativeCollectionItemStyle,
    NativeOutlineNode, OutlineRowSelectEvent, TableRowSelectEvent, Window, WindowAppearance,
    WindowBounds, WindowOptions, div, native_collection_view, native_outline_view,
    native_table_view, prelude::*, px, rgb, size,
};

struct ListViewsExample {
    selected_collection: Option<usize>,
    selected_table: Option<usize>,
    selected_outline: String,
}

impl ListViewsExample {
    const ITEMS: [&str; 10] = [
        "Accounts",
        "Projects",
        "Deployments",
        "Domains",
        "Settings",
        "Members",
        "Tokens",
        "Alerts",
        "Logs",
        "Billing",
    ];

    fn outline_nodes() -> Vec<NativeOutlineNode> {
        vec![
            NativeOutlineNode::branch(
                "Workspace",
                vec![
                    NativeOutlineNode::leaf("Overview"),
                    NativeOutlineNode::leaf("Members"),
                    NativeOutlineNode::leaf("Audit Log"),
                ],
            ),
            NativeOutlineNode::branch(
                "Applications",
                vec![
                    NativeOutlineNode::leaf("Glass"),
                    NativeOutlineNode::leaf("Console"),
                    NativeOutlineNode::leaf("Dashboard"),
                ],
            ),
        ]
    }
}

impl Render for ListViewsExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (bg, fg, muted) = if is_dark {
            (rgb(0x171b22), rgb(0xffffff), rgb(0xb7c0ce))
        } else {
            (rgb(0xf5f7fb), rgb(0x1b2434), rgb(0x606a7c))
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .gap_3()
            .p_4()
            .bg(bg)
            .text_color(fg)
            .child(
                div()
                    .text_xl()
                    .child("Native Lists: Collection / Table / Outline"),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().text_sm().child("NSCollectionView (list mode)"))
                            .child(
                                native_collection_view("collection", &Self::ITEMS)
                                    .columns(1)
                                    .item_height(34.0)
                                    .spacing(2.0)
                                    .item_style(NativeCollectionItemStyle::Label)
                                    .selected_index(self.selected_collection)
                                    .on_select(cx.listener(
                                        |this, event: &CollectionSelectEvent, _, cx| {
                                            this.selected_collection = Some(event.index);
                                            cx.notify();
                                        },
                                    ))
                                    .w(px(260.0))
                                    .h(px(360.0)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().text_sm().child("NSTableView (source list style)"))
                            .child(
                                native_table_view("table", &Self::ITEMS)
                                    .table_style(gpui::NativeTableStyle::SourceList)
                                    .row_size_style(gpui::NativeTableRowSizeStyle::Small)
                                    .show_header(false)
                                    .alternating_rows(false)
                                    .selected_index(self.selected_table)
                                    .on_select(cx.listener(
                                        |this, event: &TableRowSelectEvent, _, cx| {
                                            this.selected_table = Some(event.index);
                                            cx.notify();
                                        },
                                    ))
                                    .w(px(260.0))
                                    .h(px(360.0)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().text_sm().child("NSOutlineView"))
                            .child(
                                native_outline_view("outline", &Self::outline_nodes())
                                    .expand_all(true)
                                    .on_select(cx.listener(
                                        |this, event: &OutlineRowSelectEvent, _, cx| {
                                            this.selected_outline = event.title.to_string();
                                            cx.notify();
                                        },
                                    ))
                                    .w(px(260.0))
                                    .h(px(360.0)),
                            ),
                    ),
            )
            .child(div().text_sm().text_color(muted).child(format!(
                    "Collection: {} | Table: {} | Outline: {}",
                    self.selected_collection
                        .map(|idx| Self::ITEMS[idx].to_string())
                        .unwrap_or_else(|| "<none>".to_string()),
                    self.selected_table
                        .map(|idx| Self::ITEMS[idx].to_string())
                        .unwrap_or_else(|| "<none>".to_string()),
                    if self.selected_outline.is_empty() {
                        "<none>".to_string()
                    } else {
                        self.selected_outline.clone()
                    }
                )))
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(920.), px(540.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| ListViewsExample {
                    selected_collection: None,
                    selected_table: None,
                    selected_outline: String::new(),
                })
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
