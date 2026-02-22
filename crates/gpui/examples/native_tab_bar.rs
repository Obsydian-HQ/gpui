use gpui::{
    actions, App, Application, Bounds, Context, Entity, FocusHandle, Focusable, KeyBinding, Menu,
    MenuItem, MouseButton, NativeMenuItem, NativeVisualEffectBlendingMode,
    NativeVisualEffectMaterial, SharedString, Subscription, TitlebarOptions, WeakEntity, Window,
    WindowAppearance, WindowBounds, WindowOptions, div, native_image_view, native_sidebar,
    native_tracking_view, native_visual_effect_view, prelude::*, px, rgb, show_native_popup_menu,
    size,
};

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

actions!(
    tab_bar_example,
    [NewTab, CloseTab, NextTab, PreviousTab, ToggleSidebar, ReopenClosedTab]
);

// ---------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct TabInfo {
    title: String,
    icon: &'static str,
}

#[derive(Clone, Copy, PartialEq)]
enum TabBarMode {
    Horizontal,
    Sidebar,
}

// ---------------------------------------------------------------------------
// Main view
// ---------------------------------------------------------------------------

struct TabBarExample {
    tabs: Vec<TabInfo>,
    selected: usize,
    hovered: Option<usize>,
    close_hovered: Option<usize>,
    next_id: usize,
    mode: TabBarMode,
    closed_tabs: Vec<TabInfo>,
    sidebar_panel: Entity<SidebarTabPanel>,
    focus_handle: FocusHandle,
}

impl TabBarExample {
    fn new(sidebar_panel: Entity<SidebarTabPanel>, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        Self {
            tabs: vec![
                TabInfo { title: "Home".into(), icon: "house.fill" },
                TabInfo { title: "Documents".into(), icon: "doc.text.fill" },
                TabInfo { title: "Settings".into(), icon: "gearshape.fill" },
            ],
            selected: 0,
            hovered: None,
            close_hovered: None,
            next_id: 3,
            mode: TabBarMode::Horizontal,
            closed_tabs: Vec::new(),
            sidebar_panel,
            focus_handle,
        }
    }

    fn add_tab(&mut self, cx: &mut Context<Self>) {
        let id = self.next_id;
        self.next_id += 1;
        self.tabs.push(TabInfo {
            title: format!("Tab {id}"),
            icon: "doc.fill",
        });
        self.selected = self.tabs.len() - 1;
        cx.notify();
    }

    fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.tabs.len() <= 1 {
            return;
        }
        let removed = self.tabs.remove(index);
        self.closed_tabs.push(removed);
        if self.closed_tabs.len() > 20 {
            self.closed_tabs.remove(0);
        }
        if self.selected >= self.tabs.len() {
            self.selected = self.tabs.len().saturating_sub(1);
        }
        self.hovered = None;
        self.close_hovered = None;
        cx.notify();
    }

    fn close_other_tabs(&mut self, keep_index: usize, cx: &mut Context<Self>) {
        let mut removed = Vec::new();
        let mut kept = Vec::new();
        for (i, tab) in self.tabs.drain(..).enumerate() {
            if i == keep_index {
                kept.push(tab);
            } else {
                removed.push(tab);
            }
        }
        self.tabs = kept;
        self.closed_tabs.extend(removed);
        if self.closed_tabs.len() > 20 {
            let excess = self.closed_tabs.len() - 20;
            self.closed_tabs.drain(..excess);
        }
        self.selected = 0;
        self.hovered = None;
        self.close_hovered = None;
        cx.notify();
    }

    fn close_active_tab(&mut self, cx: &mut Context<Self>) {
        let idx = self.selected;
        self.close_tab(idx, cx);
    }

    fn next_tab(&mut self, cx: &mut Context<Self>) {
        if !self.tabs.is_empty() {
            self.selected = (self.selected + 1) % self.tabs.len();
            cx.notify();
        }
    }

    fn previous_tab(&mut self, cx: &mut Context<Self>) {
        if !self.tabs.is_empty() {
            self.selected = if self.selected == 0 {
                self.tabs.len() - 1
            } else {
                self.selected - 1
            };
            cx.notify();
        }
    }

    fn reopen_closed_tab(&mut self, cx: &mut Context<Self>) {
        if let Some(tab) = self.closed_tabs.pop() {
            self.tabs.push(tab);
            self.selected = self.tabs.len() - 1;
            cx.notify();
        }
    }

    fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.mode = match self.mode {
            TabBarMode::Horizontal => TabBarMode::Sidebar,
            TabBarMode::Sidebar => TabBarMode::Horizontal,
        };
        self.hovered = None;
        self.close_hovered = None;
        cx.notify();
    }
}

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

struct TabColors {
    fg: gpui::Rgba,
    muted: gpui::Rgba,
    selected_bg: gpui::Rgba,
    hover_bg: gpui::Rgba,
    close_hover_bg: gpui::Rgba,
    is_dark: bool,
}

impl TabColors {
    fn from_appearance(appearance: WindowAppearance) -> Self {
        let is_dark = matches!(
            appearance,
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        Self {
            fg: if is_dark { rgb(0xffffff) } else { rgb(0x1a1a1a) },
            muted: if is_dark { rgb(0x999999) } else { rgb(0x666666) },
            selected_bg: if is_dark { rgb(0x444444) } else { rgb(0xdddddd) },
            hover_bg: if is_dark { rgb(0x383838) } else { rgb(0xe8e8e8) },
            close_hover_bg: if is_dark { rgb(0x555555) } else { rgb(0xcccccc) },
            is_dark,
        }
    }
}

// ---------------------------------------------------------------------------
// Horizontal tab rendering (with native elements — works in primary surface)
// ---------------------------------------------------------------------------

fn render_tab_item(
    idx: usize,
    tab: &TabInfo,
    is_selected: bool,
    is_hovered: bool,
    is_close_hovered: bool,
    colors: &TabColors,
    on_select: impl Fn(&mut Window, &mut App) + 'static,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
    on_hover_enter: impl Fn(&mut Window, &mut App) + 'static,
    on_hover_exit: impl Fn(&mut Window, &mut App) + 'static,
    on_close_hover_enter: impl Fn(&mut Window, &mut App) + 'static,
    on_close_hover_exit: impl Fn(&mut Window, &mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let tint = if is_selected {
        (0.0, 0.478, 1.0, 1.0)
    } else if colors.is_dark {
        (0.8, 0.8, 0.8, 1.0)
    } else {
        (0.3, 0.3, 0.3, 1.0)
    };

    let close_icon_tint = if is_close_hovered {
        if colors.is_dark {
            (1.0, 1.0, 1.0, 1.0)
        } else {
            (0.1, 0.1, 0.1, 1.0)
        }
    } else if colors.is_dark {
        (0.6, 0.6, 0.6, 1.0)
    } else {
        (0.5, 0.5, 0.5, 1.0)
    };

    div()
        .id(SharedString::from(format!("tb-tab-{idx}")))
        .relative()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .px_2()
        .py_1()
        .rounded(px(6.0))
        .cursor_pointer()
        .when(is_selected, |el| el.bg(colors.selected_bg))
        .when(is_hovered && !is_selected, |el| el.bg(colors.hover_bg))
        .on_click(move |_event, window, cx| {
            on_select(window, cx);
        })
        .child(
            native_image_view(format!("tb-tab-icon-{idx}"))
                .sf_symbol(tab.icon)
                .tint_color(tint.0, tint.1, tint.2, tint.3)
                .w(px(14.0))
                .h(px(14.0)),
        )
        .child(
            div()
                .flex_grow()
                .text_xs()
                .text_color(if is_selected { colors.fg } else { colors.muted })
                .child(tab.title.clone()),
        )
        .child(
            native_tracking_view(format!("tb-tab-track-{idx}"))
                .on_mouse_enter(move |_event, window, cx| {
                    on_hover_enter(window, cx);
                })
                .on_mouse_exit(move |_event, window, cx| {
                    on_hover_exit(window, cx);
                })
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
        )
        .when(is_hovered, |el| {
            el.child(
                div()
                    .id(SharedString::from(format!("tb-close-{idx}")))
                    .relative()
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(16.0))
                    .h(px(16.0))
                    .rounded(px(4.0))
                    .cursor_pointer()
                    .when(is_close_hovered, |el| el.bg(colors.close_hover_bg))
                    .on_click(move |_event, window, cx| {
                        on_close(window, cx);
                    })
                    .child(
                        native_image_view(format!("tb-close-icon-{idx}"))
                            .sf_symbol("xmark")
                            .tint_color(
                                close_icon_tint.0,
                                close_icon_tint.1,
                                close_icon_tint.2,
                                close_icon_tint.3,
                            )
                            .w(px(8.0))
                            .h(px(8.0)),
                    )
                    .child(
                        native_tracking_view(format!("tb-close-track-{idx}"))
                            .on_mouse_enter(move |_event, window, cx| {
                                on_close_hover_enter(window, cx);
                            })
                            .on_mouse_exit(move |_event, window, cx| {
                                on_close_hover_exit(window, cx);
                            })
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full(),
                    ),
            )
        })
}

// ---------------------------------------------------------------------------
// Sidebar tab rendering (pure GPUI — no native subviews to avoid dual-surface
// coordinate issues)
// ---------------------------------------------------------------------------

fn render_sidebar_tab_item(
    idx: usize,
    tab: &TabInfo,
    is_selected: bool,
    is_hovered: bool,
    is_close_hovered: bool,
    colors: &TabColors,
    on_select: impl Fn(&mut Window, &mut App) + 'static,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let fg = if is_selected { colors.fg } else { colors.muted };
    let close_fg = if is_close_hovered {
        colors.fg
    } else {
        colors.muted
    };

    let icon_char = match tab.icon {
        "house.fill" => "\u{1F3E0}",
        "doc.text.fill" => "\u{1F4C4}",
        "gearshape.fill" => "\u{2699}\u{FE0F}",
        "doc.fill" => "\u{1F4C3}",
        _ => "\u{1F4C1}",
    };

    div()
        .id(SharedString::from(format!("sb-tab-{idx}")))
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .w_full()
        .px_2()
        .py_1()
        .rounded(px(6.0))
        .cursor_pointer()
        .when(is_selected, |el| el.bg(colors.selected_bg))
        .when(is_hovered && !is_selected, |el| el.bg(colors.hover_bg))
        .on_click(move |_event, window, cx| {
            on_select(window, cx);
        })
        .child(
            div()
                .text_xs()
                .child(icon_char.to_string()),
        )
        .child(
            div()
                .flex_grow()
                .text_xs()
                .text_color(fg)
                .child(tab.title.clone()),
        )
        .when(is_hovered, |el| {
            el.child(
                div()
                    .id(SharedString::from(format!("sb-close-{idx}")))
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(16.0))
                    .h(px(16.0))
                    .rounded(px(4.0))
                    .cursor_pointer()
                    .when(is_close_hovered, |el| el.bg(colors.close_hover_bg))
                    .hover(|style| style.bg(colors.close_hover_bg))
                    .on_click(move |_event, window, cx| {
                        on_close(window, cx);
                    })
                    .child(
                        div()
                            .text_xs()
                            .text_color(close_fg)
                            .child("\u{2715}"),
                    ),
            )
        })
}

fn build_tab_context_menu(tab_count: usize) -> Vec<NativeMenuItem> {
    let mut items = vec![NativeMenuItem::action("Close Tab")];
    if tab_count > 1 {
        items.push(NativeMenuItem::action("Close Other Tabs"));
    }
    items.push(NativeMenuItem::separator());
    items.push(NativeMenuItem::action("New Tab"));
    items
}

// ---------------------------------------------------------------------------
// Main view render
// ---------------------------------------------------------------------------

impl Render for TabBarExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = TabColors::from_appearance(window.appearance());
        let is_sidebar = self.mode == TabBarMode::Sidebar;

        let mut root = div()
            .size_full()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &NewTab, _, cx| this.add_tab(cx)))
            .on_action(cx.listener(|this, _: &CloseTab, _, cx| this.close_active_tab(cx)))
            .on_action(cx.listener(|this, _: &NextTab, _, cx| this.next_tab(cx)))
            .on_action(cx.listener(|this, _: &PreviousTab, _, cx| this.previous_tab(cx)))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| this.toggle_sidebar(cx)))
            .on_action(cx.listener(|this, _: &ReopenClosedTab, _, cx| {
                this.reopen_closed_tab(cx);
            }));

        if is_sidebar {
            root = root.child(
                native_sidebar("tab-sidebar", &[""; 0])
                    .sidebar_view(self.sidebar_panel.clone())
                    .sidebar_width(220.0)
                    .min_sidebar_width(180.0)
                    .max_sidebar_width(350.0)
                    .size_full(),
            );
        }

        root = root.child(
            div()
                .flex()
                .flex_col()
                .size_full()
                .when(!is_sidebar, |el| {
                    el.child(self.render_horizontal_tab_bar(cx, &colors))
                })
                .child(self.render_content(&colors)),
        );

        root
    }
}

impl TabBarExample {
    fn render_horizontal_tab_bar(
        &self,
        cx: &mut Context<Self>,
        colors: &TabColors,
    ) -> impl IntoElement {
        let weak = cx.entity().downgrade();
        let mut tab_items = div().flex().flex_row().gap_1().px_2().items_center();
        let tab_count = self.tabs.len();

        for (idx, tab) in self.tabs.iter().enumerate() {
            let is_selected = idx == self.selected;
            let is_hovered = self.hovered == Some(idx);
            let is_close_hovered = self.close_hovered == Some(idx);

            let w1 = weak.clone();
            let w2 = weak.clone();
            let w3 = weak.clone();
            let w4 = weak.clone();
            let w5 = weak.clone();
            let w6 = weak.clone();
            let w_ctx = weak.clone();

            let tab_item = render_tab_item(
                idx,
                tab,
                is_selected,
                is_hovered,
                is_close_hovered,
                colors,
                move |_window, cx| {
                    w1.update(cx, |this, cx| { this.selected = idx; cx.notify(); }).ok();
                },
                move |_window, cx| {
                    w2.update(cx, |this, cx| { this.close_tab(idx, cx); }).ok();
                },
                move |_window, cx| {
                    w3.update(cx, |this, cx| { this.hovered = Some(idx); cx.notify(); }).ok();
                },
                move |_window, cx| {
                    w4.update(cx, |this, cx| { this.hovered = None; cx.notify(); }).ok();
                },
                move |_window, cx| {
                    w5.update(cx, |this, cx| { this.close_hovered = Some(idx); cx.notify(); }).ok();
                },
                move |_window, cx| {
                    w6.update(cx, |this, cx| { this.close_hovered = None; cx.notify(); }).ok();
                },
            )
            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                let menu_items = build_tab_context_menu(tab_count);
                let close_others_idx = if tab_count > 1 { Some(1) } else { None };
                let new_tab_idx = if tab_count > 1 { 2 } else { 1 };
                let w = w_ctx.clone();
                show_native_popup_menu(
                    &menu_items,
                    event.position,
                    window,
                    cx,
                    move |action_idx, _window, cx| {
                        if action_idx == 0 {
                            w.update(cx, |this, cx| this.close_tab(idx, cx)).ok();
                        } else if close_others_idx == Some(action_idx) {
                            w.update(cx, |this, cx| this.close_other_tabs(idx, cx)).ok();
                        } else if action_idx == new_tab_idx {
                            w.update(cx, |this, cx| this.add_tab(cx)).ok();
                        }
                    },
                );
            });

            tab_items = tab_items.child(tab_item);
        }

        // Add tab button
        let w_add = weak.clone();
        tab_items = tab_items.child(
            div()
                .id("add-tab-btn")
                .flex()
                .items_center()
                .justify_center()
                .w(px(22.0))
                .h(px(22.0))
                .rounded(px(4.0))
                .cursor_pointer()
                .hover(|style| style.bg(colors.hover_bg))
                .on_click(move |_event, _window, cx| {
                    w_add.update(cx, |this, cx| { this.add_tab(cx); }).ok();
                })
                .child(
                    native_image_view("add-tab-icon")
                        .sf_symbol("plus")
                        .tint_color(
                            if colors.is_dark { 0.7 } else { 0.4 },
                            if colors.is_dark { 0.7 } else { 0.4 },
                            if colors.is_dark { 0.7 } else { 0.4 },
                            1.0,
                        )
                        .w(px(10.0))
                        .h(px(10.0)),
                ),
        );

        div()
            .relative()
            .w_full()
            .h(px(38.0))
            .child(
                native_visual_effect_view(
                    "tab-bar-bg",
                    NativeVisualEffectMaterial::HeaderView,
                )
                .blending_mode(NativeVisualEffectBlendingMode::WithinWindow)
                .w_full()
                .h(px(38.0)),
            )
            .child(tab_items.h(px(38.0)))
    }

    fn render_content(&self, colors: &TabColors) -> impl IntoElement {
        let selected_tab = self.tabs.get(self.selected);
        let content_text = selected_tab
            .map(|t| format!("Content: {}", t.title))
            .unwrap_or_else(|| "No tab selected".into());

        let mode_label = match self.mode {
            TabBarMode::Horizontal => "Horizontal Tab Bar",
            TabBarMode::Sidebar => "Sidebar Tab Bar",
        };

        div()
            .flex_1()
            .flex()
            .flex_col()
            .justify_center()
            .items_center()
            .gap_3()
            .text_color(colors.fg)
            .child(div().text_xl().child(content_text))
            .child(
                div()
                    .text_xs()
                    .text_color(colors.muted)
                    .child(format!("Mode: {mode_label}")),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(colors.muted)
                    .child("Cmd+T: New  |  Cmd+W: Close  |  Cmd+Shift+]/[: Switch  |  Cmd+Alt+S: Sidebar  |  Right-click: Menu"),
            )
    }
}

impl Focusable for TabBarExample {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

// ---------------------------------------------------------------------------
// Sidebar panel — rendered in the native sidebar's secondary GPUI surface.
// Uses pure GPUI elements (no native NSView subviews) to avoid dual-surface
// coordinate issues.
// ---------------------------------------------------------------------------

struct SidebarTabPanel {
    main_view: Option<WeakEntity<TabBarExample>>,
    hovered: Option<usize>,
    close_hovered: Option<usize>,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl SidebarTabPanel {
    fn bind_main_view(
        &mut self,
        main_view: WeakEntity<TabBarExample>,
        cx: &mut Context<Self>,
    ) {
        if let Some(entity) = main_view.upgrade() {
            let subscription = cx.observe(&entity, |_this, _entity, cx| {
                cx.notify();
            });
            self._subscriptions.push(subscription);
        }
        self.main_view = Some(main_view);
    }
}

impl Render for SidebarTabPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = TabColors::from_appearance(window.appearance());

        let Some(weak) = &self.main_view else {
            return div().size_full().into_any_element();
        };
        let Some(main_entity) = weak.upgrade() else {
            return div().size_full().into_any_element();
        };

        let main = main_entity.read(cx);
        let tabs = main.tabs.clone();
        let selected = main.selected;
        let tab_count = tabs.len();
        let weak_main = weak.clone();
        let weak_self = cx.entity().downgrade();

        let mut tab_list = div()
            .id("sidebar-tab-list")
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .w_full()
            .flex_1()
            .overflow_y_scroll();

        for (idx, tab) in tabs.iter().enumerate() {
            let is_selected = idx == selected;
            let is_hovered = self.hovered == Some(idx);
            let is_close_hovered = self.close_hovered == Some(idx);

            let wm1 = weak_main.clone();
            let wm2 = weak_main.clone();
            let wm_ctx = weak_main.clone();
            let ws = weak_self.clone();

            let tab_item = render_sidebar_tab_item(
                idx,
                tab,
                is_selected,
                is_hovered,
                is_close_hovered,
                &colors,
                move |_window, cx| {
                    wm1.update(cx, |this, cx| { this.selected = idx; cx.notify(); }).ok();
                },
                move |_window, cx| {
                    wm2.update(cx, |this, cx| { this.close_tab(idx, cx); }).ok();
                },
            )
            .on_mouse_move(move |_event, _window, cx| {
                ws.update(cx, |this, cx| {
                    if this.hovered != Some(idx) {
                        this.hovered = Some(idx);
                        cx.notify();
                    }
                }).ok();
            })
            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                let menu_items = build_tab_context_menu(tab_count);
                let close_others_idx = if tab_count > 1 { Some(1) } else { None };
                let new_tab_idx = if tab_count > 1 { 2 } else { 1 };
                let w = wm_ctx.clone();
                show_native_popup_menu(
                    &menu_items,
                    event.position,
                    window,
                    cx,
                    move |action_idx, _window, cx| {
                        if action_idx == 0 {
                            w.update(cx, |this, cx| this.close_tab(idx, cx)).ok();
                        } else if close_others_idx == Some(action_idx) {
                            w.update(cx, |this, cx| this.close_other_tabs(idx, cx)).ok();
                        } else if action_idx == new_tab_idx {
                            w.update(cx, |this, cx| this.add_tab(cx)).ok();
                        }
                    },
                );
            });

            tab_list = tab_list.child(tab_item);
        }

        // Add tab button at bottom
        let wm_add = weak_main.clone();
        tab_list = tab_list.child(
            div()
                .id("sb-add-tab-btn")
                .flex()
                .items_center()
                .justify_center()
                .w_full()
                .h(px(28.0))
                .mt_1()
                .rounded(px(6.0))
                .cursor_pointer()
                .hover(|style| style.bg(colors.hover_bg))
                .on_click(move |_event, _window, cx| {
                    wm_add.update(cx, |this, cx| { this.add_tab(cx); }).ok();
                })
                .child(
                    div()
                        .text_sm()
                        .text_color(colors.muted)
                        .child("+"),
                ),
        );

        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(colors.muted)
                    .p_2()
                    .child("TABS"),
            )
            .child(tab_list)
            .into_any_element()
    }
}

impl Focusable for SidebarTabPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("cmd-t", NewTab, None),
            KeyBinding::new("cmd-w", CloseTab, None),
            KeyBinding::new("cmd-shift-]", NextTab, None),
            KeyBinding::new("cmd-shift-[", PreviousTab, None),
            KeyBinding::new("cmd-alt-s", ToggleSidebar, None),
            KeyBinding::new("cmd-shift-t", ReopenClosedTab, None),
        ]);
        cx.set_menus(vec![
            Menu {
                name: "File".into(),
                items: vec![
                    MenuItem::action("New Tab", NewTab),
                    MenuItem::action("Close Tab", CloseTab),
                    MenuItem::action("Reopen Closed Tab", ReopenClosedTab),
                    MenuItem::separator(),
                ],
            },
            Menu {
                name: "View".into(),
                items: vec![
                    MenuItem::action("Toggle Sidebar", ToggleSidebar),
                    MenuItem::separator(),
                    MenuItem::action("Next Tab", NextTab),
                    MenuItem::action("Previous Tab", PreviousTab),
                ],
            },
        ]);

        let bounds = Bounds::centered(None, size(px(800.), px(500.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    appears_transparent: true,
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                let sidebar_panel = cx.new(|cx| SidebarTabPanel {
                    main_view: None,
                    hovered: None,
                    close_hovered: None,
                    focus_handle: cx.focus_handle(),
                    _subscriptions: Vec::new(),
                });

                let main_view = cx.new(|cx| {
                    TabBarExample::new(sidebar_panel.clone(), cx)
                });

                sidebar_panel.update(cx, |panel, cx| {
                    panel.bind_main_view(main_view.downgrade(), cx);
                });

                main_view.update(cx, |this, cx| {
                    this.focus_handle.focus(window, cx);
                });

                main_view
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
