use gpui::{
    App, Application, Bounds, Context, Entity, FocusHandle, Focusable, KeyBinding, Menu,
    MenuItem, NativeButtonStyle, NativeButtonTint, NativeSegmentedStyle, OutlineRowSelectEvent,
    SegmentSelectEvent, TextChangeEvent, TextSubmitEvent, Window, WindowAppearance,
    WindowBounds, WindowOptions, actions, div, native_button, native_icon_button,
    native_outline_view, native_sidebar, native_text_field, native_toggle_group, prelude::*, px,
    rgb, size, NativeOutlineNode, NativeOutlineHighlight,
};

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

actions!(
    dual_surface_example,
    [ToggleSidebar]
);

// ---------------------------------------------------------------------------
// Sidebar panel — rendered in the secondary GpuiSurface
// ---------------------------------------------------------------------------

struct SidebarPanel {
    active_tab: usize,
    search_text: String,
    selected_file: String,
    focus_handle: FocusHandle,
}

impl SidebarPanel {
    const TABS: [&str; 3] = ["Explorer", "Search", "Git"];
    const TAB_ICONS: [&str; 3] = ["folder", "magnifyingglass", "arrow.triangle.pull"];

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
                                        NativeOutlineNode::leaf("native_toggle_group.rs"),
                                    ],
                                ),
                                NativeOutlineNode::branch(
                                    "platform/mac",
                                    vec![
                                        NativeOutlineNode::leaf("gpui_surface.rs"),
                                        NativeOutlineNode::leaf("metal_renderer.rs"),
                                        NativeOutlineNode::leaf("native_controls.rs"),
                                        NativeOutlineNode::leaf("window.rs"),
                                    ],
                                ),
                                NativeOutlineNode::leaf("window.rs"),
                            ],
                        ),
                        NativeOutlineNode::branch(
                            "examples",
                            vec![
                                NativeOutlineNode::leaf("native_dual_surface_sidebar.rs"),
                                NativeOutlineNode::leaf("native_glass_dock.rs"),
                                NativeOutlineNode::leaf("native_sidebar.rs"),
                            ],
                        ),
                        NativeOutlineNode::leaf("Cargo.toml"),
                    ],
                ),
                NativeOutlineNode::leaf("README.md"),
            ],
        )]
    }

    fn changes_tree() -> Vec<NativeOutlineNode> {
        vec![
            NativeOutlineNode::branch(
                "Staged",
                vec![
                    NativeOutlineNode::leaf("metal_renderer.rs  [M]"),
                    NativeOutlineNode::leaf("gpui_surface.rs  [A]"),
                ],
            ),
            NativeOutlineNode::branch(
                "Modified",
                vec![
                    NativeOutlineNode::leaf("window.rs  [M]"),
                    NativeOutlineNode::leaf("native_sidebar.rs  [M]"),
                    NativeOutlineNode::leaf("Cargo.toml  [M]"),
                ],
            ),
        ]
    }
}

impl Render for SidebarPanel {
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
            .flex()
            .flex_col()
            .size_full()
            .text_color(fg)
            // Tab switcher
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .pt(px(8.0))
                    .pb(px(4.0))
                    .child(
                        native_toggle_group("sidebar_tabs", &Self::TABS)
                            .sf_symbols(&Self::TAB_ICONS)
                            .selected_index(self.active_tab)
                            .segment_style(NativeSegmentedStyle::Automatic)
                            .on_select(cx.listener(|this, ev: &SegmentSelectEvent, _, cx| {
                                this.active_tab = ev.index;
                                cx.notify();
                            })),
                    ),
            )
            .child(div().h(px(1.0)).w_full().bg(border))
            // Active panel content
            .child(match self.active_tab {
                0 => self.render_explorer(muted, cx).into_any_element(),
                1 => self.render_search(muted, cx).into_any_element(),
                2 => self.render_git(muted, border, cx).into_any_element(),
                _ => div().into_any_element(),
            })
    }
}

impl SidebarPanel {
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
                native_outline_view("sb_explorer", &Self::project_tree())
                    .expand_all(true)
                    .row_height(22.0)
                    .highlight(NativeOutlineHighlight::None)
                    .on_select(cx.listener(|this, ev: &OutlineRowSelectEvent, _, cx| {
                        this.selected_file = ev.title.to_string();
                        cx.notify();
                    }))
                    .flex_grow()
                    .min_h(px(200.0)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(if self.selected_file.is_empty() {
                        "No file selected".to_string()
                    } else {
                        format!("Selected: {}", self.selected_file)
                    }),
            )
    }

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
                native_text_field("sb_search")
                    .placeholder("Search in project...")
                    .on_change(cx.listener(|this, ev: &TextChangeEvent, _, cx| {
                        this.search_text = ev.text.clone();
                        cx.notify();
                    }))
                    .on_submit(cx.listener(|this, ev: &TextSubmitEvent, _, cx| {
                        this.search_text = ev.text.clone();
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
                        native_icon_button("sb_case", "textformat")
                            .tooltip("Match Case"),
                    )
                    .child(
                        native_icon_button("sb_word", "textformat.abc")
                            .tooltip("Whole Word"),
                    )
                    .child(
                        native_icon_button("sb_regex", "number")
                            .tooltip("Use Regex"),
                    ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(if self.search_text.is_empty() {
                        "Type to search...".to_string()
                    } else {
                        format!("Searching: \"{}\"", self.search_text)
                    }),
            )
    }

    fn render_git(
        &mut self,
        muted: gpui::Rgba,
        border: gpui::Rgba,
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
                            .child("Source Control"),
                    )
                    .child(
                        div().text_xs().text_color(muted).child("main"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        native_button("sb_stage", "Stage All")
                            .button_style(NativeButtonStyle::Filled)
                            .tint(NativeButtonTint::Accent),
                    )
                    .child(
                        native_icon_button("sb_refresh", "arrow.clockwise")
                            .tooltip("Refresh"),
                    ),
            )
            .child(div().h(px(1.0)).w_full().bg(border))
            .child(
                native_outline_view("sb_changes", &Self::changes_tree())
                    .expand_all(true)
                    .row_height(22.0)
                    .highlight(NativeOutlineHighlight::None)
                    .on_select(cx.listener(|this, ev: &OutlineRowSelectEvent, _, cx| {
                        this.selected_file = ev.title.to_string();
                        cx.notify();
                    }))
                    .flex_grow()
                    .min_h(px(100.0)),
            )
            .child(div().h(px(1.0)).w_full().bg(border))
            .child(
                native_text_field("sb_commit_msg")
                    .placeholder("Commit message...")
                    .w_full(),
            )
            .child(
                native_button("sb_commit", "Commit")
                    .button_style(NativeButtonStyle::Rounded),
            )
    }
}

impl Focusable for SidebarPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

// ---------------------------------------------------------------------------
// Main app — rendered in the main GPUI surface (detail pane)
// ---------------------------------------------------------------------------

struct DualSurfaceExample {
    collapsed: bool,
    sidebar_panel: Entity<SidebarPanel>,
    editor_tab: usize,
    focus_handle: FocusHandle,
}

impl DualSurfaceExample {
    const EDITOR_TABS: [&str; 3] = ["window.rs", "metal_renderer.rs", "gpui_surface.rs"];
}

impl Render for DualSurfaceExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (fg, muted, border, bg) = if is_dark {
            (rgb(0xffffff), rgb(0x8e8e93), rgb(0x3a3a3c), rgb(0x1e1e1e))
        } else {
            (rgb(0x1d1d1f), rgb(0x86868b), rgb(0xd2d2d7), rgb(0xffffff))
        };

        div()
            .size_full()
            .bg(bg)
            .text_color(fg)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| {
                this.collapsed = !this.collapsed;
                cx.notify();
            }))
            // The native sidebar with a GPUI surface in the sidebar pane.
            // Items are empty since we use sidebar_view instead of source list.
            .child(
                native_sidebar("dual_sidebar", &[""; 0])
                    .sidebar_view(self.sidebar_panel.clone())
                    .sidebar_width(280.0)
                    .min_sidebar_width(200.0)
                    .max_sidebar_width(420.0)
                    .collapsed(self.collapsed)
                    .size_full(),
            )
            // Detail pane content — rendered in the main GPUI surface
            .child(
                div()
                    .flex()
                    .flex_col()
                    .size_full()
                    // Tab bar
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .pt(px(8.0))
                            .pb(px(4.0))
                            .px_3()
                            .gap_2()
                            .child(
                                native_toggle_group("editor_tabs", &Self::EDITOR_TABS)
                                    .selected_index(self.editor_tab)
                                    .segment_style(NativeSegmentedStyle::Automatic)
                                    .on_select(cx.listener(
                                        |this, ev: &SegmentSelectEvent, _, cx| {
                                            this.editor_tab = ev.index;
                                            cx.notify();
                                        },
                                    )),
                            ),
                    )
                    .child(div().h(px(1.0)).w_full().bg(border))
                    // Editor content area
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_grow()
                            .p_4()
                            .gap_3()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child(Self::EDITOR_TABS[self.editor_tab]),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(muted)
                                    .child("This content is rendered in the main GPUI surface (detail pane)."),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(muted)
                                    .child("The sidebar has its own independent Metal rendering surface."),
                            )
                            .child(div().h(px(1.0)).w_full().bg(border))
                            .child(self.render_mock_editor(muted, border)),
                    )
                    // Status bar
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .h(px(24.0))
                            .px_3()
                            .bg(border)
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child("Dual Surface Mode — Sidebar + Detail both render via GPUI"),
                            ),
                    ),
            )
    }
}

impl DualSurfaceExample {
    fn render_mock_editor(
        &self,
        muted: gpui::Rgba,
        border: gpui::Rgba,
    ) -> impl IntoElement {
        let lines = match self.editor_tab {
            0 => vec![
                "pub struct Window {",
                "    // ... fields ...",
                "    native_view_override_stack: Vec<*mut c_void>,",
                "    #[cfg(target_os = \"macos\")]",
                "    surfaces: FxHashMap<SurfaceId, SurfaceState>,",
                "}",
            ],
            1 => vec![
                "pub(crate) struct SharedRenderResources {",
                "    pub device: metal::Device,",
                "    pub command_queue: CommandQueue,",
                "    pub sprite_atlas: Arc<MetalAtlas>,",
                "    // ... pipeline states ...",
                "}",
            ],
            _ => vec![
                "pub(crate) struct GpuiSurface {",
                "    renderer: SurfaceRenderer,",
                "    native_view: id,",
                "}",
                "",
                "impl GpuiSurface {",
                "    pub fn new(shared: Arc<SharedRenderResources>,",
                "               transparent: bool) -> Self { ... }",
                "}",
            ],
        };

        div()
            .flex()
            .flex_col()
            .gap_0()
            .p_2()
            .rounded(px(6.0))
            .border_1()
            .border_color(border)
            .children(lines.into_iter().enumerate().map(move |(i, line)| {
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .h(px(20.0))
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .w(px(24.0))
                            .text_right()
                            .child(format!("{}", i + 1)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_family("Menlo")
                            .child(line.to_string()),
                    )
            }))
    }
}

impl Focusable for DualSurfaceExample {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.bind_keys([KeyBinding::new("cmd-alt-s", ToggleSidebar, None)]);
        cx.set_menus(vec![Menu {
            name: "View".into(),
            items: vec![MenuItem::action("Toggle Sidebar", ToggleSidebar)],
        }]);

        let bounds = Bounds::centered(None, size(px(1200.), px(760.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let sidebar_panel = cx.new(|cx| SidebarPanel {
                    active_tab: 0,
                    search_text: String::new(),
                    selected_file: String::new(),
                    focus_handle: cx.focus_handle(),
                });

                cx.new(|cx| {
                    let focus_handle = cx.focus_handle();
                    focus_handle.focus(window, cx);
                    DualSurfaceExample {
                        collapsed: false,
                        sidebar_panel,
                        editor_tab: 0,
                        focus_handle,
                    }
                })
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
