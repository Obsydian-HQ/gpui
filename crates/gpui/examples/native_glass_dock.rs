use gpui::{
    App, Application, Bounds, Context, FocusHandle, Focusable, KeyBinding, Menu, MenuItem,
    MenuItemSelectEvent, NativeButtonStyle, NativeButtonTint, NativeMenuItem, NativeOutlineNode,
    NativeOutlineHighlight, NativeSegmentedStyle, OutlineRowSelectEvent, SegmentSelectEvent,
    TextChangeEvent,
    TextSubmitEvent, Window, WindowAppearance, WindowBounds, WindowOptions, actions, div,
    native_button, native_icon_button, native_menu_button, native_outline_view, native_sidebar,
    native_text_field, native_toggle_group, prelude::*, px, rgb, size,
};

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

actions!(
    glass_dock,
    [ToggleSidebar, StageAll, CommitChanges, RefreshChanges]
);

// ---------------------------------------------------------------------------
// Panel enum — maps to sidebar source-list rows
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    SourceControl,
    Explorer,
    Search,
    Branches,
    Stashes,
    Settings,
}

impl Panel {
    const ALL: [Panel; 6] = [
        Panel::SourceControl,
        Panel::Explorer,
        Panel::Search,
        Panel::Branches,
        Panel::Stashes,
        Panel::Settings,
    ];

    const LABELS: [&str; 6] = [
        "Source Control",
        "Explorer",
        "Search",
        "Branches",
        "Stashes",
        "Settings",
    ];

    const ICONS: [&str; 6] = [
        "arrow.triangle.pull",
        "folder",
        "magnifyingglass",
        "arrow.triangle.branch",
        "archivebox",
        "gearshape",
    ];

    fn label(self) -> &'static str {
        Self::LABELS[Self::ALL.iter().position(|p| *p == self).unwrap()]
    }
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

struct GlassDock {
    collapsed: bool,
    active_panel: usize,
    focus_handle: FocusHandle,

    // Source control
    view_mode: usize,
    sort_mode: usize,
    selected_file: String,
    commit_message: String,
    status_text: String,

    // Explorer
    explorer_selected: String,
}

// ---------------------------------------------------------------------------
// Data helpers
// ---------------------------------------------------------------------------

impl GlassDock {
    const VIEW_MODES: [&str; 2] = ["Tree", "Flat"];
    const SORT_MODES: [&str; 2] = ["Path", "Status"];

    fn changes_tree() -> Vec<NativeOutlineNode> {
        vec![
            NativeOutlineNode::branch(
                "Tracked",
                vec![
                    NativeOutlineNode::branch(
                        "crates/gpui/src",
                        vec![
                            NativeOutlineNode::leaf("elements/mod.rs  [M]"),
                            NativeOutlineNode::leaf("elements/native_sidebar.rs  [M]"),
                            NativeOutlineNode::leaf("platform/mac/native_controls.rs  [M]"),
                        ],
                    ),
                    NativeOutlineNode::leaf("Cargo.toml  [M]"),
                ],
            ),
            NativeOutlineNode::branch(
                "Untracked",
                vec![
                    NativeOutlineNode::leaf("examples/native_glass_dock.rs  [A]"),
                    NativeOutlineNode::leaf("docs/NATIVE_CONTROLS.md  [A]"),
                ],
            ),
            NativeOutlineNode::branch(
                "Conflicts",
                vec![NativeOutlineNode::leaf("README.md  [C]")],
            ),
        ]
    }

    fn changes_flat() -> Vec<NativeOutlineNode> {
        vec![
            NativeOutlineNode::leaf("Cargo.toml  [M]"),
            NativeOutlineNode::leaf("README.md  [C]"),
            NativeOutlineNode::leaf("examples/native_glass_dock.rs  [A]"),
            NativeOutlineNode::leaf("elements/mod.rs  [M]"),
            NativeOutlineNode::leaf("elements/native_sidebar.rs  [M]"),
            NativeOutlineNode::leaf("platform/mac/native_controls.rs  [M]"),
            NativeOutlineNode::leaf("docs/NATIVE_CONTROLS.md  [A]"),
        ]
    }

    fn project_tree() -> Vec<NativeOutlineNode> {
        vec![NativeOutlineNode::branch(
            "gpui",
            vec![
                NativeOutlineNode::branch(
                    "crates/gpui",
                    vec![
                        NativeOutlineNode::branch(
                            "src",
                            vec![
                                NativeOutlineNode::branch(
                                    "elements",
                                    vec![
                                        NativeOutlineNode::leaf("mod.rs"),
                                        NativeOutlineNode::leaf("native_button.rs"),
                                        NativeOutlineNode::leaf("native_sidebar.rs"),
                                        NativeOutlineNode::leaf("native_text_field.rs"),
                                        NativeOutlineNode::leaf("native_toggle_group.rs"),
                                    ],
                                ),
                                NativeOutlineNode::branch(
                                    "platform/mac",
                                    vec![
                                        NativeOutlineNode::leaf("native_controls.rs"),
                                        NativeOutlineNode::leaf("window.rs"),
                                    ],
                                ),
                                NativeOutlineNode::leaf("app.rs"),
                                NativeOutlineNode::leaf("lib.rs"),
                                NativeOutlineNode::leaf("window.rs"),
                            ],
                        ),
                        NativeOutlineNode::branch(
                            "examples",
                            vec![
                                NativeOutlineNode::leaf("native_button.rs"),
                                NativeOutlineNode::leaf("native_glass_dock.rs"),
                                NativeOutlineNode::leaf("native_sidebar.rs"),
                            ],
                        ),
                        NativeOutlineNode::leaf("Cargo.toml"),
                    ],
                ),
                NativeOutlineNode::leaf("Cargo.toml"),
                NativeOutlineNode::leaf("README.md"),
                NativeOutlineNode::leaf(".gitignore"),
            ],
        )]
    }

    fn branches_tree() -> Vec<NativeOutlineNode> {
        vec![
            NativeOutlineNode::branch(
                "Local",
                vec![
                    NativeOutlineNode::leaf("main"),
                    NativeOutlineNode::leaf("feature/native-sidebar"),
                    NativeOutlineNode::leaf("fix/text-field-focus"),
                    NativeOutlineNode::leaf("dev"),
                ],
            ),
            NativeOutlineNode::branch(
                "Remote / origin",
                vec![
                    NativeOutlineNode::leaf("main"),
                    NativeOutlineNode::leaf("feature/native-sidebar"),
                    NativeOutlineNode::leaf("release/v0.1"),
                ],
            ),
        ]
    }

    fn stashes_list() -> Vec<NativeOutlineNode> {
        vec![
            NativeOutlineNode::leaf("stash@{0}: WIP on main: fix layout"),
            NativeOutlineNode::leaf("stash@{1}: WIP on dev: add toolbar"),
            NativeOutlineNode::leaf("stash@{2}: WIP on main: refactor"),
        ]
    }

    fn overflow_menu() -> Vec<NativeMenuItem> {
        vec![
            NativeMenuItem::action("Stash Changes"),
            NativeMenuItem::action("Pop Latest Stash"),
            NativeMenuItem::separator(),
            NativeMenuItem::action("Discard All Changes"),
            NativeMenuItem::action("Clean Untracked Files"),
        ]
    }

    fn commit_menu() -> Vec<NativeMenuItem> {
        vec![
            NativeMenuItem::action("Commit"),
            NativeMenuItem::action("Commit & Push"),
            NativeMenuItem::separator(),
            NativeMenuItem::action("Amend Last Commit"),
            NativeMenuItem::separator(),
            NativeMenuItem::action("Sign-off"),
        ]
    }
}

// ---------------------------------------------------------------------------
// Render — native sidebar with GPUI content in the detail pane
// ---------------------------------------------------------------------------

impl Render for GlassDock {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (fg, muted, border) = if is_dark {
            (rgb(0xffffff), rgb(0x8e8e93), rgb(0x3a3a3c))
        } else {
            (rgb(0x1d1d1f), rgb(0x86868b), rgb(0xd2d2d7))
        };

        div()
            .size_full()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| {
                this.collapsed = !this.collapsed;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &StageAll, _, cx| {
                this.status_text = "All changes staged.".into();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &CommitChanges, _, cx| {
                if this.commit_message.is_empty() {
                    this.status_text = "Commit message is empty!".into();
                } else {
                    this.status_text = format!("Committed: \"{}\"", this.commit_message);
                    this.commit_message.clear();
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &RefreshChanges, _, cx| {
                this.status_text = "Changes refreshed.".into();
                cx.notify();
            }))
            // The native sidebar embeds the GPUI content view inside the sidebar
            // (left) pane of the NSSplitViewController. All children below render
            // there with the native vibrancy / blur effect behind them.
            .child(
                native_sidebar("dock_sidebar", &Panel::LABELS)
                    .embed_content_in_sidebar(true)
                    .sidebar_width(280.0)
                    .min_sidebar_width(220.0)
                    .max_sidebar_width(400.0)
                    .collapsed(self.collapsed)
                    .size_full(),
            )
            // This renders inside the sidebar pane (left side) thanks to
            // embed_content_in_sidebar(true).
            .child(
                div()
                    .flex()
                    .flex_col()
                    .size_full()
                    .text_color(fg)
                    // Panel switcher — native segmented control
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .pt(px(8.0))
                            .pb(px(4.0))
                            .child(
                                native_toggle_group("panel_switcher", &Panel::LABELS)
                                    .sf_symbols(&Panel::ICONS)
                                    .selected_index(self.active_panel)
                                    .segment_style(NativeSegmentedStyle::Automatic)
                                    .on_select(cx.listener(|this, ev: &SegmentSelectEvent, _, cx| {
                                        this.active_panel = ev.index;
                                        this.status_text = format!("{}", Panel::ALL[ev.index].label());
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(div().h(px(1.0)).w_full().bg(border))
                    .child(self.render_active_panel(muted, border, cx)),
            )
    }
}

// ---------------------------------------------------------------------------
// Panel dispatch + renderers
// ---------------------------------------------------------------------------

impl GlassDock {
    fn render_active_panel(
        &mut self,
        muted: gpui::Rgba,
        border: gpui::Rgba,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        match Panel::ALL[self.active_panel] {
            Panel::SourceControl => self.render_source_control(muted, border, cx).into_any_element(),
            Panel::Explorer => self.render_explorer(muted, cx).into_any_element(),
            Panel::Search => self.render_search(muted, cx).into_any_element(),
            Panel::Branches => self.render_branches(muted, cx).into_any_element(),
            Panel::Stashes => self.render_stashes(muted, cx).into_any_element(),
            Panel::Settings => self.render_settings(muted, cx).into_any_element(),
        }
    }

    // ── Source Control ──

    fn render_source_control(
        &mut self,
        muted: gpui::Rgba,
        border: gpui::Rgba,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let changes = if self.view_mode == 0 {
            Self::changes_tree()
        } else {
            Self::changes_flat()
        };
        let overflow_menu = Self::overflow_menu();
        let commit_menu = Self::commit_menu();

        div()
            .flex()
            .flex_col()
            .size_full()
            .p_3()
            .gap_2()
            // Repo header
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("gpui"),
                    )
                    .child(div().text_xs().text_color(muted).child("main"))
                    .child(div().text_xs().text_color(muted).child("d01d009")),
            )
            // Segmented controls: Tree/Flat and Path/Status
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        native_toggle_group("sc_view", &Self::VIEW_MODES)
                            .selected_index(self.view_mode)
                            .segment_style(NativeSegmentedStyle::RoundRect)
                            .on_select(cx.listener(|this, ev: &SegmentSelectEvent, _, cx| {
                                this.view_mode = ev.index;
                                this.status_text =
                                    format!("View: {}", Self::VIEW_MODES[ev.index]);
                                cx.notify();
                            })),
                    )
                    .child(
                        native_toggle_group("sc_sort", &Self::SORT_MODES)
                            .selected_index(self.sort_mode)
                            .segment_style(NativeSegmentedStyle::RoundRect)
                            .on_select(cx.listener(|this, ev: &SegmentSelectEvent, _, cx| {
                                this.sort_mode = ev.index;
                                this.status_text =
                                    format!("Sort: {}", Self::SORT_MODES[ev.index]);
                                cx.notify();
                            })),
                    ),
            )
            // Action row
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        native_button("sc_changes", "7 Changes")
                            .button_style(NativeButtonStyle::Borderless)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.status_text = "Opening diff view...".into();
                                cx.notify();
                            })),
                    )
                    .child(
                        native_button("sc_stage", "Stage All")
                            .button_style(NativeButtonStyle::Filled)
                            .tint(NativeButtonTint::Accent)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.status_text = "All changes staged.".into();
                                cx.notify();
                            })),
                    )
                    .child(
                        native_icon_button("sc_refresh", "arrow.clockwise")
                            .tooltip("Refresh")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.status_text = "Changes refreshed.".into();
                                cx.notify();
                            })),
                    )
                    .child(
                        native_menu_button("sc_more", "...", &overflow_menu).on_select(
                            cx.listener(|this, ev: &MenuItemSelectEvent, _, cx| {
                                this.status_text = format!("Menu action #{}", ev.index);
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .child(div().h(px(1.0)).w_full().bg(border))
            // Changes outline
            .child(
                native_outline_view("sc_tree", &changes)
                    .expand_all(true)
                    .row_height(22.0)
                    .highlight(NativeOutlineHighlight::None)
                    .on_select(cx.listener(|this, ev: &OutlineRowSelectEvent, _, cx| {
                        this.selected_file = ev.title.to_string();
                        this.status_text = format!("Selected: {}", ev.title);
                        cx.notify();
                    }))
                    .flex_grow()
                    .min_h(px(100.0)),
            )
            .child(div().h(px(1.0)).w_full().bg(border))
            // Commit message footer
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().text_xs().text_color(muted).child("Commit Message"))
                    .child(
                        native_text_field("sc_msg")
                            .placeholder("Enter commit message...")
                            .on_change(cx.listener(|this, ev: &TextChangeEvent, _, cx| {
                                this.commit_message = ev.text.clone();
                                cx.notify();
                            }))
                            .on_submit(cx.listener(|this, _: &TextSubmitEvent, _, cx| {
                                if !this.commit_message.is_empty() {
                                    this.status_text =
                                        format!("Committed: \"{}\"", this.commit_message);
                                    this.commit_message.clear();
                                }
                                cx.notify();
                            }))
                            .w_full()
                            .h(px(22.0)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                native_icon_button("sc_gen", "sparkles")
                                    .tooltip("Generate Commit Message")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.status_text = "Generating message...".into();
                                        cx.notify();
                                    })),
                            )
                            .child(
                                native_icon_button("sc_coauth", "person.2")
                                    .tooltip("Co-Authors")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.status_text = "Co-authors...".into();
                                        cx.notify();
                                    })),
                            )
                            .child(div().flex_grow())
                            .child(
                                native_menu_button("sc_commit", "Commit", &commit_menu)
                                    .on_select(cx.listener(
                                        |this, ev: &MenuItemSelectEvent, _, cx| {
                                            match ev.index {
                                                0 => {
                                                    if !this.commit_message.is_empty() {
                                                        this.status_text = format!(
                                                            "Committed: \"{}\"",
                                                            this.commit_message
                                                        );
                                                        this.commit_message.clear();
                                                    } else {
                                                        this.status_text =
                                                            "Message is empty!".into();
                                                    }
                                                }
                                                1 => {
                                                    this.status_text =
                                                        "Commit & Push...".into();
                                                }
                                                _ => {
                                                    this.status_text =
                                                        format!("Commit action #{}", ev.index);
                                                }
                                            }
                                            cx.notify();
                                        },
                                    )),
                            ),
                    ),
            )
            // Status
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .pt_1()
                    .child(if self.status_text.is_empty() {
                        "Ready".to_string()
                    } else {
                        self.status_text.clone()
                    }),
            )
    }

    // ── Explorer ──

    fn render_explorer(
        &mut self,
        muted: gpui::Rgba,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .p_3()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("Project Files"),
            )
            .child(
                native_outline_view("ex_tree", &Self::project_tree())
                    .expand_all(true)
                    .row_height(22.0)
                    .highlight(NativeOutlineHighlight::None)
                    .on_select(cx.listener(|this, ev: &OutlineRowSelectEvent, _, cx| {
                        this.explorer_selected = ev.title.to_string();
                        this.status_text = format!("Opened: {}", ev.title);
                        cx.notify();
                    }))
                    .flex_grow()
                    .min_h(px(200.0)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(if self.explorer_selected.is_empty() {
                        "No file selected".to_string()
                    } else {
                        format!("Selected: {}", self.explorer_selected)
                    }),
            )
    }

    // ── Search ──

    fn render_search(
        &mut self,
        muted: gpui::Rgba,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .p_3()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("Search"),
            )
            .child(
                native_text_field("se_input")
                    .placeholder("Search in project...")
                    .on_submit(cx.listener(|this, ev: &TextSubmitEvent, _, cx| {
                        this.status_text = format!("Searching: \"{}\"", ev.text);
                        cx.notify();
                    }))
                    .w_full(),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        native_icon_button("se_case", "textformat")
                            .tooltip("Match Case")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.status_text = "Toggle case sensitivity".into();
                                cx.notify();
                            })),
                    )
                    .child(
                        native_icon_button("se_word", "textformat.abc")
                            .tooltip("Whole Word")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.status_text = "Toggle whole word".into();
                                cx.notify();
                            })),
                    )
                    .child(
                        native_icon_button("se_regex", "number")
                            .tooltip("Use Regex")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.status_text = "Toggle regex".into();
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Press Enter to search."),
            )
    }

    // ── Branches ──

    fn render_branches(
        &mut self,
        muted: gpui::Rgba,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .p_3()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Branches"),
                    )
                    .child(
                        native_icon_button("br_new", "plus")
                            .tooltip("New Branch")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.status_text = "Creating branch...".into();
                                cx.notify();
                            })),
                    ),
            )
            .child(
                native_outline_view("br_tree", &Self::branches_tree())
                    .expand_all(true)
                    .row_height(22.0)
                    .highlight(NativeOutlineHighlight::None)
                    .on_select(cx.listener(|this, ev: &OutlineRowSelectEvent, _, cx| {
                        this.status_text = format!("Checkout: {}", ev.title);
                        cx.notify();
                    }))
                    .flex_grow()
                    .min_h(px(200.0)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("Current branch: main"),
            )
    }

    // ── Stashes ──

    fn render_stashes(
        &mut self,
        muted: gpui::Rgba,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .p_3()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Stashes"),
                    )
                    .child(
                        native_button("st_save", "Stash")
                            .button_style(NativeButtonStyle::Rounded)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.status_text = "Stashing...".into();
                                cx.notify();
                            })),
                    ),
            )
            .child(
                native_outline_view("st_list", &Self::stashes_list())
                    .row_height(22.0)
                    .highlight(NativeOutlineHighlight::None)
                    .on_select(cx.listener(|this, ev: &OutlineRowSelectEvent, _, cx| {
                        this.status_text = format!("Selected: {}", ev.title);
                        cx.notify();
                    }))
                    .flex_grow()
                    .min_h(px(100.0)),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        native_button("st_pop", "Pop")
                            .button_style(NativeButtonStyle::Rounded)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.status_text = "Popping stash...".into();
                                cx.notify();
                            })),
                    )
                    .child(
                        native_button("st_drop", "Drop")
                            .button_style(NativeButtonStyle::Rounded)
                            .tint(NativeButtonTint::Destructive)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.status_text = "Dropping stash...".into();
                                cx.notify();
                            })),
                    ),
            )
            .child(div().text_xs().text_color(muted).child("3 stashes"))
    }

    // ── Settings ──

    fn render_settings(
        &mut self,
        muted: gpui::Rgba,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let menu = vec![
            NativeMenuItem::action("Open Settings File"),
            NativeMenuItem::action("Open Keybindings"),
            NativeMenuItem::separator(),
            NativeMenuItem::action("Reset to Defaults"),
        ];

        div()
            .flex()
            .flex_col()
            .size_full()
            .p_3()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("Settings"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        native_button("cfg_signin", "Sign In to GitHub")
                            .button_style(NativeButtonStyle::Filled)
                            .tint(NativeButtonTint::Accent)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.status_text = "GitHub auth...".into();
                                cx.notify();
                            })),
                    )
                    .child(
                        native_menu_button("cfg_actions", "Quick Actions", &menu).on_select(
                            cx.listener(|this, ev: &MenuItemSelectEvent, _, cx| {
                                this.status_text = format!("Settings #{}", ev.index);
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("GPUI Native Controls v0.1"),
            )
    }
}

// ---------------------------------------------------------------------------
// Focusable
// ---------------------------------------------------------------------------

impl Focusable for GlassDock {
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
            KeyBinding::new("cmd-alt-s", ToggleSidebar, None),
            KeyBinding::new("cmd-shift-a", StageAll, None),
            KeyBinding::new("cmd-enter", CommitChanges, None),
            KeyBinding::new("cmd-shift-r", RefreshChanges, None),
        ]);

        cx.set_menus(vec![
            Menu {
                name: "View".into(),
                items: vec![MenuItem::action("Toggle Sidebar", ToggleSidebar)],
            },
            Menu {
                name: "Git".into(),
                items: vec![
                    MenuItem::action("Stage All", StageAll),
                    MenuItem::action("Commit", CommitChanges),
                    MenuItem::action("Refresh", RefreshChanges),
                ],
            },
        ]);

        let bounds = Bounds::centered(None, size(px(1100.), px(720.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                cx.new(|cx| {
                    let focus_handle = cx.focus_handle();
                    focus_handle.focus(window, cx);
                    GlassDock {
                        collapsed: false,
                        active_panel: 0,
                        focus_handle,
                        view_mode: 0,
                        sort_mode: 0,
                        selected_file: String::new(),
                        commit_message: String::new(),
                        status_text: String::new(),
                        explorer_selected: String::new(),
                    }
                })
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
