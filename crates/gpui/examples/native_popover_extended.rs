use gpui::{
    App, Application, Bounds, Context, NativeColor, NativePopover, NativePopoverAnchor,
    NativePopoverBehavior, NativePopoverCheckbox, NativePopoverClickableRow,
    NativePopoverCloseEvent, NativePopoverColorDot, NativePopoverContentItem,
    NativePopoverProgress, NativePopoverShowEvent, NativePopoverToggle, NativeToolbar,
    NativeToolbarButton, NativeToolbarClickEvent, NativeToolbarDisplayMode, NativeToolbarItem,
    NativeToolbarSizeMode, Window, WindowAppearance, WindowBounds, WindowOptions, div, prelude::*,
    px, rgb, size,
};

struct ExtendedPopoverExample {
    toolbar_installed: bool,
    popover_visible: bool,
    status_message: String,

    // Toggle states
    predictions_enabled: bool,
    privacy_mode: bool,

    // Checkbox states
    eager_mode: bool,
    subtle_mode: bool,

    // Progress
    usage_count: f64,
    usage_max: f64,
}

impl ExtendedPopoverExample {
    fn show_lsp_popover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.show_native_popover(
            NativePopover::new(320.0, 340.0)
                .behavior(NativePopoverBehavior::Transient)
                .on_show(cx.listener(
                    |this, _event: &NativePopoverShowEvent, _window, cx| {
                        this.popover_visible = true;
                        this.status_message = "LSP popover shown.".to_string();
                        cx.notify();
                    },
                ))
                .on_close(cx.listener(
                    |this, _event: &NativePopoverCloseEvent, _window, cx| {
                        this.popover_visible = false;
                        this.status_message = "LSP popover closed.".to_string();
                        cx.notify();
                    },
                ))
                .item(NativePopoverContentItem::heading("Language Servers"))
                .item(NativePopoverContentItem::separator())
                .item(NativePopoverColorDot::new("rust-analyzer", NativeColor::Green)
                    .detail("v0.3.1 \u{2022} 124 MB")
                    .on_click(|window, _cx| {
                        println!("Clicked rust-analyzer");
                        window.dismiss_native_popover();
                    }))
                .item(NativePopoverColorDot::new("typescript-language-server", NativeColor::Green)
                    .detail("v4.3.3 \u{2022} 86 MB")
                    .on_click(|window, _cx| {
                        println!("Clicked typescript-language-server");
                        window.dismiss_native_popover();
                    }))
                .item(NativePopoverColorDot::new("gopls", NativeColor::Yellow)
                    .detail("v0.15.0 \u{2022} Starting...")
                    .on_click(|window, _cx| {
                        println!("Clicked gopls");
                        window.dismiss_native_popover();
                    }))
                .item(NativePopoverColorDot::new("clangd", NativeColor::Red)
                    .detail("Error: binary not found"))
                .item(NativePopoverContentItem::separator())
                .item(NativePopoverClickableRow::new("Restart All Servers")
                    .icon("arrow.clockwise")
                    .on_click(|window, _cx| {
                        println!("Restart all servers");
                        window.dismiss_native_popover();
                    }))
                .item(NativePopoverClickableRow::new("View Logs")
                    .icon("doc.text")
                    .on_click(|window, _cx| {
                        println!("View logs");
                        window.dismiss_native_popover();
                    })),
            NativePopoverAnchor::ToolbarItem("lsp".into()),
        );
    }

    fn show_predictions_popover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let predictions_enabled = self.predictions_enabled;
        let privacy_mode = self.privacy_mode;
        let eager_mode = self.eager_mode;
        let subtle_mode = self.subtle_mode;
        let usage_count = self.usage_count;
        let usage_max = self.usage_max;

        window.show_native_popover(
            NativePopover::new(300.0, 380.0)
                .behavior(NativePopoverBehavior::Transient)
                .on_show(cx.listener(
                    |this, _event: &NativePopoverShowEvent, _window, cx| {
                        this.popover_visible = true;
                        this.status_message = "Predictions popover shown.".to_string();
                        cx.notify();
                    },
                ))
                .on_close(cx.listener(
                    |this, _event: &NativePopoverCloseEvent, _window, cx| {
                        this.popover_visible = false;
                        this.status_message = "Predictions popover closed.".to_string();
                        cx.notify();
                    },
                ))
                .item(NativePopoverContentItem::heading("Edit Predictions"))
                .item(NativePopoverContentItem::separator())
                .item(NativePopoverToggle::new("Show Predictions", predictions_enabled)
                    .on_change(|checked, _window, _cx| {
                        println!("Predictions enabled: {checked}");
                    })
                    .description("Suggest completions as you type"))
                .item(NativePopoverToggle::new("Privacy Mode", privacy_mode)
                    .on_change(|checked, _window, _cx| {
                        println!("Privacy mode: {checked}");
                    }))
                .item(NativePopoverContentItem::separator())
                .item(NativePopoverContentItem::heading("Display Mode"))
                .item(NativePopoverCheckbox::new("Eager Mode", eager_mode)
                    .on_change(|checked, _window, _cx| {
                        println!("Eager mode: {checked}");
                    }))
                .item(NativePopoverCheckbox::new("Subtle Mode", subtle_mode)
                    .on_change(|checked, _window, _cx| {
                        println!("Subtle mode: {checked}");
                    }))
                .item(NativePopoverContentItem::separator())
                .item(NativePopoverContentItem::heading("Usage"))
                .item(NativePopoverProgress::new(usage_count, usage_max)
                    .label(format!("{} / {} predictions used", usage_count as u32, usage_max as u32)))
                .item(NativePopoverContentItem::separator())
                .item(NativePopoverContentItem::button("Manage Settings...", |window, _cx| {
                    println!("Manage settings");
                    window.dismiss_native_popover();
                })),
            NativePopoverAnchor::ToolbarItem("predictions".into()),
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

impl Render for ExtendedPopoverExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.toolbar_installed {
            window.set_native_toolbar(Some(
                NativeToolbar::new("gpui.toolbar.popover.extended")
                    .title("Extended Popover Demo")
                    .display_mode(NativeToolbarDisplayMode::IconAndLabel)
                    .size_mode(NativeToolbarSizeMode::Regular)
                    .item(NativeToolbarItem::Button(
                        NativeToolbarButton::new("lsp", "Language Servers")
                            .icon("server.rack")
                            .tool_tip("Show language server status")
                            .on_click(cx.listener(
                                |this, _event: &NativeToolbarClickEvent, window, cx| {
                                    this.toggle_popover(
                                        Self::show_lsp_popover,
                                        window,
                                        cx,
                                    );
                                },
                            )),
                    ))
                    .item(NativeToolbarItem::FlexibleSpace)
                    .item(NativeToolbarItem::Button(
                        NativeToolbarButton::new("predictions", "Edit Predictions")
                            .icon("sparkles")
                            .tool_tip("Show edit prediction settings")
                            .on_click(cx.listener(
                                |this, _event: &NativeToolbarClickEvent, window, cx| {
                                    this.toggle_popover(
                                        Self::show_predictions_popover,
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
            .child(div().text_xl().child("Extended NativePopover Demo"))
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Click toolbar buttons to show popovers with toggles, checkboxes, progress bars, and color dots."),
            )
            .child(
                div()
                    .mt_4()
                    .text_sm()
                    .text_color(muted)
                    .child(format!("Status: {}", self.status_message)),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(format!(
                        "Predictions: {} | Privacy: {} | Eager: {} | Subtle: {}",
                        if self.predictions_enabled { "ON" } else { "OFF" },
                        if self.privacy_mode { "ON" } else { "OFF" },
                        if self.eager_mode { "ON" } else { "OFF" },
                        if self.subtle_mode { "ON" } else { "OFF" },
                    )),
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
                cx.new(|_| ExtendedPopoverExample {
                    toolbar_installed: false,
                    popover_visible: false,
                    status_message: "Ready.".to_string(),
                    predictions_enabled: true,
                    privacy_mode: false,
                    eager_mode: true,
                    subtle_mode: false,
                    usage_count: 42.0,
                    usage_max: 100.0,
                })
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
