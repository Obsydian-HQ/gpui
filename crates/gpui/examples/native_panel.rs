/// Native Panel Example
///
/// Demonstrates NSPanel with different styles, levels, and materials.
/// Shows borderless, HUD, utility, and titled panels with content items.
use gpui::{
    App, Application, Bounds, Context, NativeColor, NativePanel, NativePanelAnchor,
    NativePanelLevel, NativePanelMaterial, NativePanelStyle, NativePopoverClickableRow,
    NativePopoverColorDot, NativePopoverContentItem, NativePopoverProgress, NativePopoverToggle,
    Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};

struct PanelExample {
    status: String,
}

impl PanelExample {
    fn new() -> Self {
        Self {
            status: "Click a button to show a panel".to_string(),
        }
    }

    fn show_borderless_panel(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        self.status = "Showing borderless panel".to_string();
        let panel = NativePanel::new(300.0, 200.0)
            .style(NativePanelStyle::Borderless)
            .level(NativePanelLevel::Floating)
            .material(NativePanelMaterial::Popover)
            .corner_radius(12.0)
            .on_close(|_, _, _| {})
            .item(NativePopoverContentItem::heading("Suggestions"))
            .item(
                NativePopoverClickableRow::new("Search the web")
                    .icon("magnifyingglass")
                    .on_click(|_, _| {}),
            )
            .item(
                NativePopoverClickableRow::new("Open recent file")
                    .icon("doc.text")
                    .detail("~/Documents")
                    .on_click(|_, _| {}),
            )
            .item(NativePopoverContentItem::separator())
            .item(NativePopoverContentItem::small_label("Press Esc to dismiss"));
        window.show_native_panel(panel, NativePanelAnchor::Centered);
    }

    fn show_hud_panel(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        self.status = "Showing HUD panel".to_string();
        let panel = NativePanel::new(280.0, 180.0)
            .style(NativePanelStyle::Hud)
            .level(NativePanelLevel::Floating)
            .on_close(|_, _, _| {})
            .item(NativePopoverContentItem::heading("Inspector"))
            .item(NativePopoverToggle::new("Auto-save", true).on_change(|_, _, _| {}))
            .item(NativePopoverToggle::new("Line numbers", true).on_change(|_, _, _| {}))
            .item(NativePopoverToggle::new("Word wrap", false).on_change(|_, _, _| {}));
        window.show_native_panel(panel, NativePanelAnchor::Centered);
    }

    fn show_utility_panel(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        self.status = "Showing utility panel".to_string();
        let panel = NativePanel::new(260.0, 200.0)
            .style(NativePanelStyle::Utility)
            .level(NativePanelLevel::Floating)
            .on_close(|_, _, _| {})
            .item(NativePopoverContentItem::heading("Color Palette"))
            .item(
                NativePopoverColorDot::new("Blue", NativeColor::Blue)
                    .detail("Primary")
                    .on_click(|_, _| {}),
            )
            .item(
                NativePopoverColorDot::new("Green", NativeColor::Green)
                    .detail("Success")
                    .on_click(|_, _| {}),
            )
            .item(
                NativePopoverColorDot::new("Red", NativeColor::Red)
                    .detail("Error")
                    .on_click(|_, _| {}),
            )
            .item(NativePopoverContentItem::separator())
            .item(NativePopoverContentItem::button("Add Color", |_, _| {}));
        window.show_native_panel(panel, NativePanelAnchor::Centered);
    }

    fn show_titled_panel(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        self.status = "Showing titled panel".to_string();
        let panel = NativePanel::new(320.0, 220.0)
            .style(NativePanelStyle::Titled)
            .level(NativePanelLevel::Floating)
            .material(NativePanelMaterial::UnderWindow)
            .on_close(|_, _, _| {})
            .item(NativePopoverContentItem::heading("Build Status"))
            .item(NativePopoverProgress::new(0.75, 1.0).label("75% complete"))
            .item(NativePopoverContentItem::icon_label(
                "checkmark.circle",
                "Tests passed: 42/56",
            ))
            .item(NativePopoverContentItem::icon_label(
                "xmark.circle",
                "Tests failed: 14/56",
            ))
            .item(NativePopoverContentItem::separator())
            .item(NativePopoverContentItem::button("View Details", |_, _| {}));
        window.show_native_panel(panel, NativePanelAnchor::Centered);
    }

    fn show_positioned_panel(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        self.status = "Showing panel at position (100, 400)".to_string();
        let panel = NativePanel::new(240.0, 150.0)
            .style(NativePanelStyle::Borderless)
            .level(NativePanelLevel::PopUpMenu)
            .material(NativePanelMaterial::HudWindow)
            .corner_radius(8.0)
            .on_close(|_, _, _| {})
            .item(NativePopoverContentItem::heading("Quick Actions"))
            .item(
                NativePopoverClickableRow::new("New File")
                    .icon("plus")
                    .detail("\u{2318}N")
                    .on_click(|_, _| {}),
            )
            .item(
                NativePopoverClickableRow::new("New Folder")
                    .icon("folder")
                    .detail("\u{21e7}\u{2318}N")
                    .on_click(|_, _| {}),
            );
        window.show_native_panel(
            panel,
            NativePanelAnchor::Point { x: 100.0, y: 400.0 },
        );
    }

    fn dismiss_panel(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        self.status = "Panel dismissed".to_string();
        window.dismiss_native_panel();
    }
}

impl Render for PanelExample {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1e1e2e))
            .p(px(24.0))
            .gap(px(12.0))
            .child(
                div()
                    .text_color(rgb(0xcdd6f4))
                    .text_xl()
                    .child("Native Panel Example"),
            )
            .child(
                div()
                    .text_color(rgb(0xa6adc8))
                    .child(self.status.clone()),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(make_button(
                        "Borderless Panel (Suggestion Box)",
                        0x45475a,
                        cx.listener(|this, _, window, cx| this.show_borderless_panel(window, cx)),
                    ))
                    .child(make_button(
                        "HUD Panel (Inspector)",
                        0x45475a,
                        cx.listener(|this, _, window, cx| this.show_hud_panel(window, cx)),
                    ))
                    .child(make_button(
                        "Utility Panel (Color Palette)",
                        0x45475a,
                        cx.listener(|this, _, window, cx| this.show_utility_panel(window, cx)),
                    ))
                    .child(make_button(
                        "Titled Panel (Build Status)",
                        0x45475a,
                        cx.listener(|this, _, window, cx| this.show_titled_panel(window, cx)),
                    ))
                    .child(make_button(
                        "Positioned Panel (Quick Actions)",
                        0x45475a,
                        cx.listener(|this, _, window, cx| this.show_positioned_panel(window, cx)),
                    ))
                    .child(make_button(
                        "Dismiss Panel",
                        0xf38ba8,
                        cx.listener(|this, _, window, cx| this.dismiss_panel(window, cx)),
                    )),
            )
    }
}

fn make_button(
    label: &'static str,
    bg_color: u32,
    handler: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(label)
        .px(px(12.0))
        .py(px(8.0))
        .bg(rgb(bg_color))
        .rounded(px(6.0))
        .text_color(rgb(0xcdd6f4))
        .cursor_pointer()
        .on_click(handler)
        .child(label)
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(500.0), px(450.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| PanelExample::new()),
        )
        .ok();
    });
}
