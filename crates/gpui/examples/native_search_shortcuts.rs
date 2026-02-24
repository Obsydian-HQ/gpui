use gpui::{
    App, Application, Bounds, Context, FocusHandle, Focusable, KeyBinding, Menu, MenuItem,
    NativeSearchFieldTarget, NativeToolbar, NativeToolbarButton, NativeToolbarClickEvent,
    NativeToolbarDisplayMode, NativeToolbarItem, NativeToolbarSearchEvent,
    NativeToolbarSearchField, NativeToolbarSizeMode, SearchChangeEvent, SearchSubmitEvent, Window,
    WindowAppearance, WindowBounds, WindowOptions, actions, div, native_search_field,
    native_sidebar, prelude::*, px, rgb, size,
};

actions!(
    native_search_shortcuts_example,
    [FocusToolbarSearch, FocusContentSearch, ToggleSidebar]
);

const TOOLBAR_SEARCH_ID: &str = "search.shortcuts.toolbar";
const CONTENT_SEARCH_ID: &str = "search.shortcuts.content";

struct NativeSearchShortcutsExample {
    focus_handle: FocusHandle,
    toolbar_installed: bool,
    sidebar_collapsed: bool,
    toolbar_search_text: String,
    content_search_text: String,
    submitted_text: String,
    last_focus_target: String,
}

impl NativeSearchShortcutsExample {
    const SIDEBAR_ITEMS: [&str; 10] = [
        "New Tab",
        "Bookmarks",
        "History",
        "Downloads",
        "Pinned Tabs",
        "Profiles",
        "Workspace",
        "Extensions",
        "Settings",
        "Help",
    ];
}

impl Render for NativeSearchShortcutsExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.toolbar_installed {
            let this_for_back = cx.entity().downgrade();
            let this_for_forward = cx.entity().downgrade();
            let this_for_change = cx.entity().downgrade();
            let this_for_submit = cx.entity().downgrade();

            window.set_native_toolbar(Some(
                NativeToolbar::new("gpui.native.search.shortcuts")
                    .title("Search Shortcut Focus Demo")
                    .display_mode(NativeToolbarDisplayMode::IconOnly)
                    .size_mode(NativeToolbarSizeMode::Regular)
                    .item(NativeToolbarItem::Button(
                        NativeToolbarButton::new("back", "")
                            .icon("chevron.left")
                            .tool_tip("Back")
                            .on_click(move |_: &NativeToolbarClickEvent, _, cx| {
                                this_for_back
                                    .update(cx, |this, cx| {
                                        this.last_focus_target = "back button".to_string();
                                        cx.notify();
                                    })
                                    .ok();
                            }),
                    ))
                    .item(NativeToolbarItem::Button(
                        NativeToolbarButton::new("forward", "")
                            .icon("chevron.right")
                            .tool_tip("Forward")
                            .on_click(move |_: &NativeToolbarClickEvent, _, cx| {
                                this_for_forward
                                    .update(cx, |this, cx| {
                                        this.last_focus_target = "forward button".to_string();
                                        cx.notify();
                                    })
                                    .ok();
                            }),
                    ))
                    .item(NativeToolbarItem::FlexibleSpace)
                    .item(NativeToolbarItem::SearchField(
                        NativeToolbarSearchField::new(TOOLBAR_SEARCH_ID)
                            .placeholder("Toolbar search (Cmd+L)")
                            .text(self.toolbar_search_text.clone())
                            .min_width(px(280.0))
                            .max_width(px(580.0))
                            .on_change(move |event: &NativeToolbarSearchEvent, _, cx| {
                                let text = event.text.clone();
                                this_for_change
                                    .update(cx, |this, cx| {
                                        this.toolbar_search_text = text;
                                        cx.notify();
                                    })
                                    .ok();
                            })
                            .on_submit(move |event: &NativeToolbarSearchEvent, _, cx| {
                                let text = event.text.clone();
                                this_for_submit
                                    .update(cx, |this, cx| {
                                        this.submitted_text = text;
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
        let (bg, fg, muted, border) = if is_dark {
            (rgb(0x171b22), rgb(0xf0f6fc), rgb(0x8b949e), rgb(0x30363d))
        } else {
            (rgb(0xf6f8fa), rgb(0x24292f), rgb(0x57606a), rgb(0xd0d7de))
        };

        div()
            .size_full()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &FocusToolbarSearch, window, cx| {
                window.focus_native_search_field(
                    NativeSearchFieldTarget::ToolbarItem(TOOLBAR_SEARCH_ID.into()),
                    true,
                );
                this.last_focus_target = "toolbar search (cmd-l)".to_string();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FocusContentSearch, window, cx| {
                window.focus_native_search_field(
                    NativeSearchFieldTarget::ContentElement(CONTENT_SEARCH_ID.into()),
                    true,
                );
                this.last_focus_target = "content search (cmd-shift-l)".to_string();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| {
                this.sidebar_collapsed = !this.sidebar_collapsed;
                cx.notify();
            }))
            .child(
                native_sidebar("search-shortcuts-sidebar", &Self::SIDEBAR_ITEMS)
                    .manage_toolbar(false)
                    .sidebar_width(240.0)
                    .min_sidebar_width(180.0)
                    .max_sidebar_width(360.0)
                    .collapsed(self.sidebar_collapsed)
                    .selected_index(Some(0))
                    .size_full(),
            )
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_4()
                    .bg(bg)
                    .child(
                        div()
                            .text_xl()
                            .text_color(fg)
                            .child("Glass-Like Search Focus Testbed"),
                    )
                    .child(
                        div().text_sm().text_color(muted).child(
                            "Cmd+L focuses toolbar search, Cmd+Shift+L focuses content search",
                        ),
                    )
                    .child(
                        native_search_field(CONTENT_SEARCH_ID)
                            .placeholder("Content search field (new-tab style)")
                            .value(self.content_search_text.clone())
                            .on_change(cx.listener(|this, event: &SearchChangeEvent, _, cx| {
                                this.content_search_text = event.text.clone();
                                cx.notify();
                            }))
                            .on_submit(cx.listener(|this, event: &SearchSubmitEvent, _, cx| {
                                this.submitted_text = event.text.clone();
                                cx.notify();
                            }))
                            .w(px(520.0)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(format!("Submitted: {}", self.submitted_text)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(format!("Last focus target: {}", self.last_focus_target)),
                    )
                    .child(div().w(px(520.0)).h(px(1.0)).bg(border))
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child("Use this to validate shortcut routing before migrating Glass."),
                    ),
            )
    }
}

impl Focusable for NativeSearchShortcutsExample {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("cmd-l", FocusToolbarSearch, None),
            KeyBinding::new("cmd-shift-l", FocusContentSearch, None),
            KeyBinding::new("cmd-alt-s", ToggleSidebar, None),
        ]);
        cx.set_menus(vec![Menu {
            name: "View".into(),
            items: vec![MenuItem::action("Toggle Sidebar", ToggleSidebar)],
        }]);

        let bounds = Bounds::centered(None, size(px(1260.0), px(820.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                cx.new(|cx| {
                    let focus_handle = cx.focus_handle();
                    focus_handle.focus(window, cx);
                    NativeSearchShortcutsExample {
                        focus_handle,
                        toolbar_installed: false,
                        sidebar_collapsed: false,
                        toolbar_search_text: String::new(),
                        content_search_text: String::new(),
                        submitted_text: "<none>".to_string(),
                        last_focus_target: "<none>".to_string(),
                    }
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
