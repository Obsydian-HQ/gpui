/// Native Toolbar Search Example
///
/// Demonstrates NSSearchField in a native toolbar with back/forward navigation.
/// The search field has on_change and on_submit callbacks.
///
/// See `native_search_suggestions.rs` for a version with a suggestion dropdown panel.
use gpui::{
    App, Application, Bounds, Context, NativeToolbar, NativeToolbarButton,
    NativeToolbarClickEvent, NativeToolbarDisplayMode, NativeToolbarItem,
    NativeToolbarSearchEvent, NativeToolbarSearchField, NativeToolbarSizeMode, Window,
    WindowAppearance, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};

const PAGES: &[(&str, &str)] = &[
    ("https://apple.com", "Apple"),
    ("https://developer.apple.com/documentation", "Apple Developer Documentation"),
    ("https://github.com", "GitHub"),
    ("https://github.com/nickel-org/nickel.rs", "Nickel.rs - Web Framework for Rust"),
    ("https://docs.rs", "Docs.rs"),
    ("https://crates.io", "crates.io: Rust Package Registry"),
    ("https://www.rust-lang.org", "Rust Programming Language"),
    ("https://doc.rust-lang.org/book/", "The Rust Programming Language Book"),
    ("https://zed.dev", "Zed - Code Editor"),
    ("https://gpui.rs", "GPUI - GPU-accelerated UI Framework"),
];

struct BrowserExample {
    toolbar_installed: bool,
    current_url: String,
    current_title: String,
    search_text: String,
    history: Vec<(String, String)>,
    history_index: Option<usize>,
}

impl BrowserExample {
    fn new() -> Self {
        Self {
            toolbar_installed: false,
            current_url: String::new(),
            current_title: "New Tab".to_string(),
            search_text: String::new(),
            history: Vec::new(),
            history_index: None,
        }
    }

    fn navigate_to(&mut self, url: &str) {
        let title = PAGES
            .iter()
            .find(|(u, _)| *u == url)
            .map(|(_, t)| t.to_string())
            .unwrap_or_else(|| format!("Page: {}", url));

        self.current_url = url.to_string();
        self.current_title = title.clone();

        if let Some(index) = self.history_index {
            self.history.truncate(index + 1);
        }
        self.history.push((url.to_string(), title));
        self.history_index = Some(self.history.len() - 1);
    }

    fn go_back(&mut self) {
        if let Some(index) = self.history_index {
            if index > 0 {
                let new_index = index - 1;
                self.history_index = Some(new_index);
                let (url, title) = self.history[new_index].clone();
                self.current_url = url;
                self.current_title = title;
            }
        }
    }

    fn go_forward(&mut self) {
        if let Some(index) = self.history_index {
            if index + 1 < self.history.len() {
                let new_index = index + 1;
                self.history_index = Some(new_index);
                let (url, title) = self.history[new_index].clone();
                self.current_url = url;
                self.current_title = title;
            }
        }
    }
}

impl Render for BrowserExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.toolbar_installed {
            let this_for_back = cx.entity().downgrade();
            let this_for_forward = cx.entity().downgrade();
            let this_for_change = cx.entity().downgrade();
            let this_for_submit = cx.entity().downgrade();

            window.set_native_toolbar(Some(
                NativeToolbar::new("browser.toolbar")
                    .title("GPUI Browser Search")
                    .display_mode(NativeToolbarDisplayMode::IconOnly)
                    .size_mode(NativeToolbarSizeMode::Regular)
                    .shows_baseline_separator(true)
                    .item(NativeToolbarItem::Button(
                        NativeToolbarButton::new("back", "Back")
                            .icon("chevron.left")
                            .tool_tip("Go back")
                            .on_click(move |_: &NativeToolbarClickEvent, _window, cx| {
                                this_for_back
                                    .update(cx, |this, cx| {
                                        this.go_back();
                                        cx.notify();
                                    })
                                    .ok();
                            }),
                    ))
                    .item(NativeToolbarItem::Button(
                        NativeToolbarButton::new("forward", "Forward")
                            .icon("chevron.right")
                            .tool_tip("Go forward")
                            .on_click(move |_: &NativeToolbarClickEvent, _window, cx| {
                                this_for_forward
                                    .update(cx, |this, cx| {
                                        this.go_forward();
                                        cx.notify();
                                    })
                                    .ok();
                            }),
                    ))
                    .item(NativeToolbarItem::SearchField(
                        NativeToolbarSearchField::new("search")
                            .placeholder("Search or enter URL...")
                            .min_width(px(300.0))
                            .max_width(px(600.0))
                            .on_change(move |event: &NativeToolbarSearchEvent, _window, cx| {
                                let text = event.text.clone();
                                this_for_change
                                    .update(cx, |this, cx| {
                                        this.search_text = text;
                                        cx.notify();
                                    })
                                    .ok();
                            })
                            .on_submit(move |event: &NativeToolbarSearchEvent, _window, cx| {
                                let text = event.text.clone();
                                this_for_submit
                                    .update(cx, |this, cx| {
                                        if text.starts_with("http://")
                                            || text.starts_with("https://")
                                        {
                                            this.navigate_to(&text);
                                        } else {
                                            let url = format!(
                                                "https://google.com/search?q={}",
                                                text
                                            );
                                            this.navigate_to(&url);
                                        }
                                        cx.notify();
                                    })
                                    .ok();
                            }),
                    )),
            ));
            self.toolbar_installed = true;
        }

        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (bg, fg, muted, accent) = if is_dark {
            (rgb(0x1c1f24), rgb(0xffffff), rgb(0x8b95a3), rgb(0x58a6ff))
        } else {
            (rgb(0xf5f7fa), rgb(0x1b2230), rgb(0x5f6978), rgb(0x0366d6))
        };

        if !self.current_url.is_empty() {
            div()
                .flex()
                .flex_col()
                .size_full()
                .items_center()
                .justify_center()
                .gap_4()
                .bg(bg)
                .child(
                    div()
                        .text_2xl()
                        .text_color(fg)
                        .child(self.current_title.clone()),
                )
                .child(
                    div()
                        .text_base()
                        .text_color(accent)
                        .child(self.current_url.clone()),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(format!("History: {} entries", self.history.len())),
                )
        } else {
            div()
                .flex()
                .flex_col()
                .size_full()
                .items_center()
                .justify_center()
                .gap_3()
                .bg(bg)
                .child(div().text_3xl().text_color(fg).child("New Tab"))
                .child(
                    div()
                        .text_base()
                        .text_color(muted)
                        .child("Type in the search field above"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child("Try: \"rust\", \"apple\", or \"github\""),
                )
        }
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(900.0), px(600.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| BrowserExample::new()),
        )
        .unwrap();
        cx.activate(true);
    });
}
