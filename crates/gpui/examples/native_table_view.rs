use gpui::{
    App, Application, Bounds, Context, TableRowSelectEvent, Window, WindowAppearance, WindowBounds,
    WindowOptions, div, native_table_view, prelude::*, px, rgb, size,
};

struct TableViewExample {
    selected: Option<usize>,
}

impl TableViewExample {
    const ROWS: [&str; 14] = [
        "Aptos - Connected",
        "Stripe - Connected",
        "GitHub - Connected",
        "Linear - Connected",
        "Slack - Disconnected",
        "Discord - Connected",
        "Notion - Connected",
        "Vercel - Connected",
        "Sentry - Connected",
        "Datadog - Disconnected",
        "Dropbox - Connected",
        "Figma - Connected",
        "Jira - Connected",
        "Confluence - Connected",
    ];
}

impl Render for TableViewExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (bg, fg, muted) = if is_dark {
            (rgb(0x171b20), rgb(0xffffff), rgb(0xb1b8c3))
        } else {
            (rgb(0xf5f7fb), rgb(0x1a2332), rgb(0x5f6878))
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .gap_3()
            .p_4()
            .bg(bg)
            .text_color(fg)
            .child(div().text_xl().child("Native TableView (Dense Rows)"))
            .child(
                native_table_view("connections", &Self::ROWS)
                    .selected_index(self.selected)
                    .row_height(24.0)
                    .on_select(cx.listener(|this, event: &TableRowSelectEvent, _, cx| {
                        this.selected = Some(event.index);
                        cx.notify();
                    }))
                    .h(px(300.0)),
            )
            .child(div().text_sm().text_color(muted).child(format!(
                "Selected: {}",
                self.selected
                    .map(|i| Self::ROWS[i].to_string())
                    .unwrap_or_else(|| "<none>".to_string())
            )))
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(700.), px(500.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| TableViewExample { selected: None }),
        )
        .unwrap();

        cx.activate(true);
    });
}
