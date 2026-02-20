use gpui::{
    App, Application, Bounds, Context, NativeToolbar, NativeToolbarButton, NativeToolbarClickEvent,
    NativeToolbarDisplayMode, NativeToolbarItem, NativeToolbarSearchEvent,
    NativeToolbarSearchField, NativeToolbarSizeMode, Window, WindowAppearance, WindowBounds,
    WindowOptions, div, prelude::*, px, rgb, size,
};

struct NativeToolbarExample {
    toolbar_installed: bool,
    count: usize,
    query: String,
    submitted: String,
    last_action: String,
}

impl Render for NativeToolbarExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.toolbar_installed {
            window.set_native_toolbar(Some(
                NativeToolbar::new("gpui.native.toolbar.example")
                    .title("GPUI Native Toolbar")
                    .display_mode(NativeToolbarDisplayMode::IconAndLabel)
                    .size_mode(NativeToolbarSizeMode::Regular)
                    .item(NativeToolbarItem::Button(
                        NativeToolbarButton::new("increment", "Increment")
                            .tool_tip("Increase the counter")
                            .icon("plus.circle")
                            .on_click(cx.listener(
                                |this, event: &NativeToolbarClickEvent, _, cx| {
                                    this.count += 1;
                                    this.last_action = format!("clicked {}", event.item_id);
                                    cx.notify();
                                },
                            )),
                    ))
                    .item(NativeToolbarItem::Button(
                        NativeToolbarButton::new("reset", "Reset")
                            .tool_tip("Reset the counter")
                            .icon("arrow.counterclockwise")
                            .on_click(cx.listener(
                                |this, event: &NativeToolbarClickEvent, _, cx| {
                                    this.count = 0;
                                    this.last_action = format!("clicked {}", event.item_id);
                                    cx.notify();
                                },
                            )),
                    ))
                    .item(NativeToolbarItem::Button(
                        NativeToolbarButton::new("avatar", "Avatar")
                            .tool_tip("Image loaded from URL (circular)")
                            .image_url("https://avatars.githubusercontent.com/u/1714999")
                            .image_circular(true)
                            .on_click(cx.listener(
                                |this, _event: &NativeToolbarClickEvent, _, cx| {
                                    this.last_action = "clicked avatar".to_string();
                                    cx.notify();
                                },
                            )),
                    ))
                    .item(NativeToolbarItem::FlexibleSpace)
                    .item(NativeToolbarItem::SearchField(
                        NativeToolbarSearchField::new("toolbar_search")
                            .placeholder("Search the example")
                            .on_change(cx.listener(
                                |this, event: &NativeToolbarSearchEvent, _, cx| {
                                    this.query = event.text.clone();
                                    this.last_action = format!("changed {}", event.item_id);
                                    cx.notify();
                                },
                            ))
                            .on_submit(cx.listener(
                                |this, event: &NativeToolbarSearchEvent, _, cx| {
                                    this.submitted = event.text.clone();
                                    this.last_action = format!("submitted {}", event.item_id);
                                    cx.notify();
                                },
                            )),
                    )),
            ));
            self.toolbar_installed = true;
        }

        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );

        let (bg, fg, muted) = if is_dark {
            (rgb(0x1c1f24), rgb(0xffffff), rgb(0xaeb7c2))
        } else {
            (rgb(0xf4f6fa), rgb(0x17202c), rgb(0x4f5d6d))
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .bg(bg)
            .text_color(fg)
            .child(div().text_xl().child("Native Toolbar Demo"))
            .child(div().text_lg().child(format!("Count: {}", self.count)))
            .child(div().text_sm().text_color(muted).child(format!(
                "Live query: {}",
                if self.query.is_empty() {
                    "<empty>".to_string()
                } else {
                    self.query.clone()
                }
            )))
            .child(div().text_sm().text_color(muted).child(format!(
                "Submitted: {}",
                if self.submitted.is_empty() {
                    "<none>".to_string()
                } else {
                    self.submitted.clone()
                }
            )))
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(format!("Last action: {}", self.last_action)),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Use toolbar buttons and search field above the content area."),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(780.0), px(460.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| NativeToolbarExample {
                    toolbar_installed: false,
                    count: 0,
                    query: String::new(),
                    submitted: String::new(),
                    last_action: "<none>".to_string(),
                })
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
