/// Native Search Suggestions Example
///
/// Production-ready search suggestion panel using NSPanel anchored below a
/// toolbar search field. Features:
/// - Filter-as-you-type with categorized results (bookmarks, history, web search)
/// - Hover highlight on rows
/// - Keyboard navigation (up/down arrows, enter to select, escape to dismiss)
/// - Auto-dismiss when a suggestion is selected or the panel loses focus
/// - Adaptive dark/light appearance
/// - SF Symbol icons per category
use gpui::{
    App, Application, Bounds, Context, NativePanel, NativePanelAnchor, NativePanelLevel,
    NativePanelMaterial, NativePanelStyle, NativePopoverClickableRow, NativePopoverContentItem,
    NativeToolbar, NativeToolbarButton, NativeToolbarClickEvent, NativeToolbarDisplayMode,
    NativeToolbarItem, NativeToolbarSearchEvent, NativeToolbarSearchField, NativeToolbarSizeMode,
    Window, WindowAppearance, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};

// ── Suggestion data ─────────────────────────────────────────────────────────

#[derive(Clone)]
struct Suggestion {
    icon: &'static str,
    title: &'static str,
    detail: Option<&'static str>,
    url: &'static str,
    category: Category,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Category {
    Bookmark,
    History,
    TopHit,
}

const SUGGESTIONS: &[Suggestion] = &[
    Suggestion { icon: "star.fill", title: "GitHub", detail: Some("github.com"), url: "https://github.com", category: Category::Bookmark },
    Suggestion { icon: "star.fill", title: "Stack Overflow", detail: Some("stackoverflow.com"), url: "https://stackoverflow.com", category: Category::Bookmark },
    Suggestion { icon: "star.fill", title: "Rust Docs", detail: Some("doc.rust-lang.org"), url: "https://doc.rust-lang.org", category: Category::Bookmark },
    Suggestion { icon: "star.fill", title: "crates.io", detail: Some("crates.io"), url: "https://crates.io", category: Category::Bookmark },
    Suggestion { icon: "star.fill", title: "Apple Developer", detail: Some("developer.apple.com"), url: "https://developer.apple.com", category: Category::Bookmark },
    Suggestion { icon: "clock", title: "Rust async/await tutorial", detail: Some("blog.rust-lang.org"), url: "https://blog.rust-lang.org/async-await", category: Category::History },
    Suggestion { icon: "clock", title: "NSPanel documentation", detail: Some("developer.apple.com"), url: "https://developer.apple.com/documentation/appkit/nspanel", category: Category::History },
    Suggestion { icon: "clock", title: "GPUI framework", detail: Some("gpui.rs"), url: "https://gpui.rs", category: Category::History },
    Suggestion { icon: "clock", title: "Zed editor", detail: Some("zed.dev"), url: "https://zed.dev", category: Category::History },
    Suggestion { icon: "clock", title: "Tauri app framework", detail: Some("tauri.app"), url: "https://tauri.app", category: Category::History },
    Suggestion { icon: "globe", title: "Rust Programming Language", detail: Some("rust-lang.org"), url: "https://www.rust-lang.org", category: Category::TopHit },
    Suggestion { icon: "globe", title: "The Rust Book", detail: Some("doc.rust-lang.org/book"), url: "https://doc.rust-lang.org/book/", category: Category::TopHit },
    Suggestion { icon: "globe", title: "Docs.rs", detail: Some("docs.rs"), url: "https://docs.rs", category: Category::TopHit },
];

fn filter_suggestions(query: &str) -> Vec<Suggestion> {
    if query.is_empty() {
        return SUGGESTIONS.to_vec();
    }
    let lower = query.to_lowercase();
    SUGGESTIONS
        .iter()
        .filter(|s| {
            s.title.to_lowercase().contains(&lower)
                || s.detail
                    .map(|d| d.to_lowercase().contains(&lower))
                    .unwrap_or(false)
                || s.url.to_lowercase().contains(&lower)
        })
        .cloned()
        .collect()
}

/// Counts the total number of clickable rows for a given query (used for wrapping navigation).
fn count_rows(query: &str) -> usize {
    let matches = filter_suggestions(query);
    let mut count = 0;
    count += matches.iter().filter(|s| s.category == Category::TopHit).count();
    count += matches.iter().filter(|s| s.category == Category::Bookmark).count();
    count += matches.iter().filter(|s| s.category == Category::History).count();
    if !query.is_empty() {
        count += 1; // web search row
    }
    count
}

// ── App state ───────────────────────────────────────────────────────────────

struct SearchApp {
    toolbar_installed: bool,
    search_text: String,
    current_url: String,
    current_title: String,
    panel_visible: bool,
    selected_index: Option<usize>,
    history: Vec<(String, String)>,
    history_index: Option<usize>,
}

impl SearchApp {
    fn new() -> Self {
        Self {
            toolbar_installed: false,
            search_text: String::new(),
            current_url: String::new(),
            current_title: "New Tab".to_string(),
            panel_visible: false,
            selected_index: None,
            history: Vec::new(),
            history_index: None,
        }
    }

    fn navigate_to(&mut self, url: &str, title: &str) {
        self.current_url = url.to_string();
        self.current_title = title.to_string();
        if let Some(index) = self.history_index {
            self.history.truncate(index + 1);
        }
        self.history.push((url.to_string(), title.to_string()));
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

impl Render for SearchApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.toolbar_installed {
            let weak_back = cx.entity().downgrade();
            let weak_forward = cx.entity().downgrade();
            let weak_change = cx.entity().downgrade();
            let weak_submit = cx.entity().downgrade();
            let weak_move_up = cx.entity().downgrade();
            let weak_move_down = cx.entity().downgrade();
            let weak_cancel = cx.entity().downgrade();
            let weak_end_editing = cx.entity().downgrade();

            window.set_native_toolbar(Some(
                NativeToolbar::new("search_suggestions.toolbar")
                    .title("Search Suggestions Demo")
                    .display_mode(NativeToolbarDisplayMode::IconOnly)
                    .size_mode(NativeToolbarSizeMode::Regular)
                    .shows_baseline_separator(true)
                    .item(NativeToolbarItem::Button(
                        NativeToolbarButton::new("back", "Back")
                            .icon("chevron.left")
                            .tool_tip("Go back")
                            .on_click(move |_: &NativeToolbarClickEvent, _window, cx| {
                                weak_back
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
                                weak_forward
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
                            .on_change(move |event: &NativeToolbarSearchEvent, window, cx| {
                                let text = event.text.clone();
                                weak_change
                                    .update(cx, |this, cx| {
                                        this.search_text = text.clone();
                                        this.panel_visible = !text.is_empty();
                                        this.selected_index = None;
                                        cx.notify();
                                    })
                                    .ok();
                                if text.is_empty() {
                                    window.dismiss_native_panel();
                                } else {
                                    let matches = filter_suggestions(&text);
                                    if matches.is_empty() {
                                        window.dismiss_native_panel();
                                    } else {
                                        show_suggestion_panel(&text, None, window);
                                    }
                                }
                            })
                            .on_submit(move |event: &NativeToolbarSearchEvent, window, cx| {
                                let text = event.text.clone();
                                window.dismiss_native_panel();
                                weak_submit
                                    .update(cx, |this, cx| {
                                        // If a row is selected via keyboard, navigate to it
                                        if let Some(selected) = this.selected_index {
                                            if let Some(url) = url_for_selected_row(&this.search_text, selected) {
                                                this.navigate_to(&url, &url);
                                                this.panel_visible = false;
                                                this.selected_index = None;
                                                cx.notify();
                                                return;
                                            }
                                        }
                                        this.panel_visible = false;
                                        this.selected_index = None;
                                        if text.starts_with("http://")
                                            || text.starts_with("https://")
                                        {
                                            this.navigate_to(&text, &text);
                                        } else {
                                            let url = format!(
                                                "https://google.com/search?q={}",
                                                text
                                            );
                                            this.navigate_to(&url, &format!("Search: {}", text));
                                        }
                                        cx.notify();
                                    })
                                    .ok();
                            })
                            .on_move_down(move |_event: &NativeToolbarSearchEvent, window, cx| {
                                weak_move_down
                                    .update(cx, |this, cx| {
                                        let total = count_rows(&this.search_text);
                                        if total == 0 {
                                            return;
                                        }
                                        this.selected_index = Some(match this.selected_index {
                                            Some(i) => (i + 1) % total,
                                            None => 0,
                                        });
                                        cx.notify();
                                    })
                                    .ok();
                                // Re-read state to show updated panel
                                if let Ok(text) = weak_move_down.read_with(cx, |this, _| this.search_text.clone()) {
                                    let selected = weak_move_down.read_with(cx, |this, _| this.selected_index).ok().flatten();
                                    if !text.is_empty() {
                                        show_suggestion_panel(&text, selected, window);
                                    }
                                }
                            })
                            .on_move_up(move |_event: &NativeToolbarSearchEvent, window, cx| {
                                weak_move_up
                                    .update(cx, |this, cx| {
                                        let total = count_rows(&this.search_text);
                                        if total == 0 {
                                            return;
                                        }
                                        this.selected_index = Some(match this.selected_index {
                                            Some(0) | None => total.saturating_sub(1),
                                            Some(i) => i - 1,
                                        });
                                        cx.notify();
                                    })
                                    .ok();
                                if let Ok(text) = weak_move_up.read_with(cx, |this, _| this.search_text.clone()) {
                                    let selected = weak_move_up.read_with(cx, |this, _| this.selected_index).ok().flatten();
                                    if !text.is_empty() {
                                        show_suggestion_panel(&text, selected, window);
                                    }
                                }
                            })
                            .on_cancel(move |_event: &NativeToolbarSearchEvent, window, cx| {
                                window.dismiss_native_panel();
                                weak_cancel
                                    .update(cx, |this, cx| {
                                        this.panel_visible = false;
                                        this.selected_index = None;
                                        cx.notify();
                                    })
                                    .ok();
                            })
                            .on_end_editing(move |_event: &NativeToolbarSearchEvent, window, cx| {
                                window.dismiss_native_panel();
                                weak_end_editing
                                    .update(cx, |this, cx| {
                                        this.panel_visible = false;
                                        this.selected_index = None;
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
                .child(div().text_2xl().text_color(fg).child(self.current_title.clone()))
                .child(div().text_base().text_color(accent).child(self.current_url.clone()))
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
                        .child("Type in the search field to see suggestions"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child("Try: \"rust\", \"apple\", \"github\", \"zed\""),
                )
        }
    }
}

/// Returns the URL for a given selected row index, or None if out of bounds.
/// Rows are ordered: search row (if query non-empty), top hits, bookmarks, history.
fn url_for_selected_row(query: &str, index: usize) -> Option<String> {
    let matches = filter_suggestions(query);
    let mut current = 0;

    // Web search row first
    if !query.is_empty() {
        if current == index {
            return Some(format!("https://google.com/search?q={}", query));
        }
        current += 1;
    }

    for suggestion in matches.iter().filter(|s| s.category == Category::TopHit) {
        if current == index {
            return Some(suggestion.url.to_string());
        }
        current += 1;
    }
    for suggestion in matches.iter().filter(|s| s.category == Category::Bookmark) {
        if current == index {
            return Some(suggestion.url.to_string());
        }
        current += 1;
    }
    for suggestion in matches.iter().filter(|s| s.category == Category::History) {
        if current == index {
            return Some(suggestion.url.to_string());
        }
        current += 1;
    }

    None
}

/// Builds and shows the suggestion panel from the toolbar callback context.
fn show_suggestion_panel(query: &str, selected_index: Option<usize>, window: &mut Window) {
    let matches = filter_suggestions(query);
    if matches.is_empty() && query.is_empty() {
        window.dismiss_native_panel();
        return;
    }

    let bookmarks: Vec<_> = matches
        .iter()
        .filter(|s| s.category == Category::Bookmark)
        .collect();
    let history: Vec<_> = matches
        .iter()
        .filter(|s| s.category == Category::History)
        .collect();
    let top_hits: Vec<_> = matches
        .iter()
        .filter(|s| s.category == Category::TopHit)
        .collect();

    let mut items: Vec<NativePopoverContentItem> = Vec::new();
    let mut row_count = 0usize;
    let mut row_index = 0usize;

    // "Search Google" row first
    if !query.is_empty() {
        let search_label = format!("Search \"{}\"", query);
        items.push(
            NativePopoverClickableRow::new(search_label)
                .icon("magnifyingglass")
                .detail("Google Search")
                .selected(selected_index == Some(row_index))
                .on_click(|window, _cx| {
                    window.dismiss_native_panel();
                })
                .into(),
        );
        row_count += 1;
        row_index += 1;
    }

    if !top_hits.is_empty() {
        if !items.is_empty() {
            items.push(NativePopoverContentItem::separator());
        }
        items.push(NativePopoverContentItem::heading("Top Hits"));
        for suggestion in &top_hits {
            items.push(
                NativePopoverClickableRow::new(suggestion.title)
                    .icon(suggestion.icon)
                    .detail(suggestion.detail.unwrap_or(""))
                    .selected(selected_index == Some(row_index))
                    .on_click(|window, _cx| {
                        window.dismiss_native_panel();
                    })
                    .into(),
            );
            row_count += 1;
            row_index += 1;
        }
    }

    if !bookmarks.is_empty() {
        if !items.is_empty() {
            items.push(NativePopoverContentItem::separator());
        }
        items.push(NativePopoverContentItem::heading("Bookmarks"));
        for suggestion in &bookmarks {
            items.push(
                NativePopoverClickableRow::new(suggestion.title)
                    .icon(suggestion.icon)
                    .detail(suggestion.detail.unwrap_or(""))
                    .selected(selected_index == Some(row_index))
                    .on_click(|window, _cx| {
                        window.dismiss_native_panel();
                    })
                    .into(),
            );
            row_count += 1;
            row_index += 1;
        }
    }

    if !history.is_empty() {
        if !items.is_empty() {
            items.push(NativePopoverContentItem::separator());
        }
        items.push(NativePopoverContentItem::heading("History"));
        for suggestion in &history {
            items.push(
                NativePopoverClickableRow::new(suggestion.title)
                    .icon(suggestion.icon)
                    .detail(suggestion.detail.unwrap_or(""))
                    .selected(selected_index == Some(row_index))
                    .on_click(|window, _cx| {
                        window.dismiss_native_panel();
                    })
                    .into(),
            );
            row_count += 1;
            row_index += 1;
        }
    }

    let heading_count = [!top_hits.is_empty(), !bookmarks.is_empty(), !history.is_empty()]
        .iter()
        .filter(|&&b| b)
        .count();
    let separator_count = heading_count.saturating_sub(1)
        + if !query.is_empty() && heading_count > 0 {
            1
        } else {
            0
        };

    let padding = 16.0;
    let row_height = 28.0;
    let heading_height = 28.0;
    let separator_height = 12.0;
    // Add separator for the search row if there are other sections
    let search_separator = if !query.is_empty() && (!top_hits.is_empty() || !bookmarks.is_empty() || !history.is_empty()) {
        separator_height
    } else {
        0.0
    };
    let content_height = (row_count as f64 * row_height)
        + (heading_count as f64 * heading_height)
        + (separator_count as f64 * separator_height)
        + search_separator
        + padding * 2.0;
    let panel_height = content_height.min(400.0);

    let panel = NativePanel::new(400.0, panel_height)
        .style(NativePanelStyle::Borderless)
        .level(NativePanelLevel::PopUpMenu)
        .non_activating(true)
        .has_shadow(true)
        .corner_radius(10.0)
        .material(NativePanelMaterial::Popover)
        .on_close(|_, _, _| {})
        .items(items);

    window.show_native_panel(panel, NativePanelAnchor::ToolbarItem("search".into()));
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(900.0), px(600.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| SearchApp::new()),
        )
        .ok();
        cx.activate(true);
    });
}
