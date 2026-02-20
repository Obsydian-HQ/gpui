use gpui::{
    App, Application, Bounds, Context, NativePopover, NativePopoverAnchor,
    NativePopoverBehavior, NativePopoverCloseEvent, NativePopoverContentItem,
    NativePopoverShowEvent, NativeToolbar, NativeToolbarButton, NativeToolbarClickEvent,
    NativeToolbarDisplayMode, NativeToolbarItem, NativeToolbarSizeMode, Window,
    WindowAppearance, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};

struct ToolbarPopoverExample {
    toolbar_installed: bool,
    popover_visible: bool,
    status_message: String,
    popover_show_count: usize,
}

impl ToolbarPopoverExample {
    fn show_info_popover(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.show_native_popover(
            NativePopover::new(280.0, 180.0)
                .behavior(NativePopoverBehavior::Transient)
                .on_show(cx.listener(
                    |this, _event: &NativePopoverShowEvent, _window, cx| {
                        this.popover_visible = true;
                        this.popover_show_count += 1;
                        this.status_message = format!(
                            "Info popover shown (#{}).",
                            this.popover_show_count
                        );
                        cx.notify();
                    },
                ))
                .on_close(cx.listener(
                    |this, _event: &NativePopoverCloseEvent, _window, cx| {
                        this.popover_visible = false;
                        this.status_message = "Info popover closed.".to_string();
                        cx.notify();
                    },
                ))
                .item(NativePopoverContentItem::heading("Application Info"))
                .item(NativePopoverContentItem::separator())
                .item(NativePopoverContentItem::label("GPUI Native Popover Demo"))
                .item(NativePopoverContentItem::label("Version 1.0.0"))
                .item(NativePopoverContentItem::separator())
                .item(NativePopoverContentItem::button(
                    "Dismiss",
                    |window, _cx| {
                        window.dismiss_native_popover();
                    },
                )),
            NativePopoverAnchor::ToolbarItem("info".into()),
        );
    }

    fn show_branch_popover(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.show_native_popover(
            NativePopover::new(340.0, 440.0)
                .behavior(NativePopoverBehavior::Transient)
                .on_show(cx.listener(
                    |this, _event: &NativePopoverShowEvent, _window, cx| {
                        this.popover_visible = true;
                        this.popover_show_count += 1;
                        this.status_message = format!(
                            "Branch popover shown (#{}).",
                            this.popover_show_count
                        );
                        cx.notify();
                    },
                ))
                .on_close(cx.listener(
                    |this, _event: &NativePopoverCloseEvent, _window, cx| {
                        this.popover_visible = false;
                        this.status_message = "Branch popover closed.".to_string();
                        cx.notify();
                    },
                ))
                .item(NativePopoverContentItem::heading("Current Branch"))
                .item(NativePopoverContentItem::icon_label(
                    "arrow.triangle.branch",
                    "main",
                ))
                .item(NativePopoverContentItem::small_label(
                    "Last commit: Fix auth flow \u{2022} 2 hours ago",
                ))
                .item(NativePopoverContentItem::separator())
                .item(NativePopoverContentItem::heading("Local Branches"))
                .item(NativePopoverContentItem::icon_label(
                    "arrow.triangle.branch",
                    "feature/native-toolbar",
                ))
                .item(NativePopoverContentItem::small_label(
                    "Add NSToolbar support \u{2022} 3 days ago",
                ))
                .item(NativePopoverContentItem::icon_label(
                    "arrow.triangle.branch",
                    "fix/render-loop",
                ))
                .item(NativePopoverContentItem::small_label(
                    "Fix infinite re-render \u{2022} 1 week ago",
                ))
                .item(NativePopoverContentItem::icon_label(
                    "arrow.triangle.branch",
                    "refactor/entity-system",
                ))
                .item(NativePopoverContentItem::small_label(
                    "Simplify entity lifecycle \u{2022} 2 weeks ago",
                ))
                .item(NativePopoverContentItem::separator())
                .item(NativePopoverContentItem::heading("Remote Branches"))
                .item(NativePopoverContentItem::icon_label(
                    "globe",
                    "origin/main",
                ))
                .item(NativePopoverContentItem::small_label(
                    "Merge PR #142 \u{2022} 5 hours ago",
                ))
                .item(NativePopoverContentItem::icon_label(
                    "globe",
                    "origin/develop",
                ))
                .item(NativePopoverContentItem::small_label(
                    "Update dependencies \u{2022} 1 day ago",
                ))
                .item(NativePopoverContentItem::separator())
                .item(NativePopoverContentItem::button(
                    "Create New Branch...",
                    |window, _cx| {
                        window.dismiss_native_popover();
                    },
                ))
                .item(NativePopoverContentItem::button(
                    "Manage Remotes...",
                    |window, _cx| {
                        window.dismiss_native_popover();
                    },
                )),
            NativePopoverAnchor::ToolbarItem("branches".into()),
        );
    }

    fn show_settings_popover(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.show_native_popover(
            NativePopover::new(300.0, 200.0)
                .behavior(NativePopoverBehavior::Semitransient)
                .on_close(cx.listener(
                    |this, _event: &NativePopoverCloseEvent, _window, cx| {
                        this.popover_visible = false;
                        this.status_message = "Settings popover closed.".to_string();
                        cx.notify();
                    },
                ))
                .item(NativePopoverContentItem::heading("Settings"))
                .item(NativePopoverContentItem::separator())
                .item(NativePopoverContentItem::button(
                    "Reset All Preferences",
                    |window, _cx| {
                        window.dismiss_native_popover();
                    },
                ))
                .item(NativePopoverContentItem::button(
                    "Clear Cache",
                    |window, _cx| {
                        window.dismiss_native_popover();
                    },
                ))
                .item(NativePopoverContentItem::separator())
                .item(NativePopoverContentItem::small_label(
                    "Changes take effect immediately.",
                )),
            NativePopoverAnchor::ToolbarItem("settings".into()),
        );
        self.popover_visible = true;
        self.popover_show_count += 1;
        self.status_message = format!(
            "Settings popover shown (#{}).",
            self.popover_show_count
        );
    }

    fn toggle_popover(
        &mut self,
        show_fn: fn(&mut Self, &mut Window, &mut Context<Self>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.popover_visible {
            window.dismiss_native_popover();
            self.popover_visible = false;
            cx.notify();
            return;
        }
        show_fn(self, window, cx);
        cx.notify();
    }
}

impl Render for ToolbarPopoverExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.toolbar_installed {
            window.set_native_toolbar(Some(
                NativeToolbar::new("gpui.toolbar.popover.example")
                    .title("Toolbar Popover Demo")
                    .display_mode(NativeToolbarDisplayMode::IconAndLabel)
                    .size_mode(NativeToolbarSizeMode::Regular)
                    .item(NativeToolbarItem::Button(
                        NativeToolbarButton::new("info", "Info")
                            .tool_tip("Show info popover")
                            .on_click(cx.listener(
                                |this, _event: &NativeToolbarClickEvent, window, cx| {
                                    this.toggle_popover(
                                        Self::show_info_popover,
                                        window,
                                        cx,
                                    );
                                },
                            )),
                    ))
                    .item(NativeToolbarItem::Button(
                        NativeToolbarButton::new("branches", "Branches")
                            .tool_tip("Show branch switcher")
                            .on_click(cx.listener(
                                |this, _event: &NativeToolbarClickEvent, window, cx| {
                                    this.toggle_popover(
                                        Self::show_branch_popover,
                                        window,
                                        cx,
                                    );
                                },
                            )),
                    ))
                    .item(NativeToolbarItem::FlexibleSpace)
                    .item(NativeToolbarItem::Button(
                        NativeToolbarButton::new("settings", "Settings")
                            .tool_tip("Show settings popover")
                            .on_click(cx.listener(
                                |this, _event: &NativeToolbarClickEvent, window, cx| {
                                    this.toggle_popover(
                                        Self::show_settings_popover,
                                        window,
                                        cx,
                                    );
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
            .child(div().text_xl().child("NSPopover + NSToolbar Demo"))
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Click toolbar buttons to show native popovers."),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("\"Branches\" shows a complex branch-switcher-style popover."),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Uses showRelativeToToolbarItem: (macOS 14+)."),
            )
            .child(
                div()
                    .mt_4()
                    .text_lg()
                    .child(format!("Popover opens: {}", self.popover_show_count)),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(format!("Status: {}", self.status_message)),
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
                cx.new(|_| ToolbarPopoverExample {
                    toolbar_installed: false,
                    popover_visible: false,
                    status_message: "Ready.".to_string(),
                    popover_show_count: 0,
                })
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
