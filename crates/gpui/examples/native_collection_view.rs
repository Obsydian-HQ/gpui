use gpui::{
    App, Application, Bounds, CollectionSelectEvent, Context, NativeCollectionItemStyle, Window,
    WindowAppearance, WindowBounds, WindowOptions, div, native_collection_view, prelude::*, px,
    rgb, size,
};

struct CollectionViewExample {
    selected: Option<usize>,
}

impl CollectionViewExample {
    const APPS: [&str; 12] = [
        "Calendar",
        "Mail",
        "Notes",
        "Music",
        "Maps",
        "Photos",
        "Books",
        "TV",
        "Stocks",
        "Weather",
        "Shortcuts",
        "Xcode",
    ];
}

impl Render for CollectionViewExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (bg, fg, muted) = if is_dark {
            (rgb(0x1b1d22), rgb(0xffffff), rgb(0xb5bcc8))
        } else {
            (rgb(0xf4f6fa), rgb(0x1a2230), rgb(0x5d6676))
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .gap_3()
            .p_4()
            .bg(bg)
            .text_color(fg)
            .child(div().text_xl().child("Native CollectionView (Cards)"))
            .child(
                native_collection_view("apps", &Self::APPS)
                    .columns(3)
                    .item_height(72.0)
                    .item_style(NativeCollectionItemStyle::Card)
                    .selected_index(self.selected)
                    .on_select(cx.listener(|this, event: &CollectionSelectEvent, _, cx| {
                        this.selected = Some(event.index);
                        cx.notify();
                    }))
                    .h(px(300.0)),
            )
            .child(div().text_sm().text_color(muted).child(format!(
                "Selected: {}",
                self.selected
                    .map(|i| Self::APPS[i].to_string())
                    .unwrap_or_else(|| "<none>".to_string())
            )))
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(760.), px(520.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| CollectionViewExample { selected: None }),
        )
        .unwrap();

        cx.activate(true);
    });
}
