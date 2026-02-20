use gpui::{
    App, Application, Bounds, Context, NativeToolbar, NativeToolbarButton, NativeToolbarClickEvent,
    NativeToolbarDisplayMode, NativeToolbarItem, NativeToolbarMenuButton,
    NativeToolbarMenuButtonSelectEvent, NativeToolbarMenuItem, NativeToolbarSizeMode, Window,
    WindowAppearance, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};

struct MenuToolbarExample {
    toolbar_installed: bool,
    last_action: String,
    selection_count: usize,
}

impl Render for MenuToolbarExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.toolbar_installed {
            window.set_native_toolbar(Some(
                NativeToolbar::new("gpui.toolbar.menu.example")
                    .title("Toolbar Menu Button Demo")
                    .display_mode(NativeToolbarDisplayMode::IconAndLabel)
                    .size_mode(NativeToolbarSizeMode::Regular)
                    .item(NativeToolbarItem::Button(
                        NativeToolbarButton::new("action", "Action")
                            .tool_tip("A regular toolbar button")
                            .on_click(cx.listener(
                                |this, _event: &NativeToolbarClickEvent, _, cx| {
                                    this.last_action = "Clicked regular button".to_string();
                                    cx.notify();
                                },
                            )),
                    ))
                    .item(NativeToolbarItem::MenuButton(
                        NativeToolbarMenuButton::new(
                            "file_menu",
                            "File",
                            vec![
                                NativeToolbarMenuItem::action("New Document").icon("doc.badge.plus"),
                                NativeToolbarMenuItem::action("Open...").icon("folder"),
                                NativeToolbarMenuItem::separator(),
                                NativeToolbarMenuItem::action("Save").icon("square.and.arrow.down"),
                                NativeToolbarMenuItem::action("Save As..."),
                                NativeToolbarMenuItem::separator(),
                                NativeToolbarMenuItem::submenu(
                                    "Export",
                                    vec![
                                        NativeToolbarMenuItem::action("PDF"),
                                        NativeToolbarMenuItem::action("HTML"),
                                        NativeToolbarMenuItem::action("Markdown"),
                                    ],
                                )
                                .icon("square.and.arrow.up"),
                                NativeToolbarMenuItem::separator(),
                                NativeToolbarMenuItem::action("Close")
                                    .icon("xmark.circle")
                                    .enabled(false),
                            ],
                        )
                        .icon("doc.text")
                        .tool_tip("File operations")
                        .on_select(cx.listener(
                            |this, event: &NativeToolbarMenuButtonSelectEvent, _, cx| {
                                let names = [
                                    "New Document",
                                    "Open...",
                                    "Save",
                                    "Save As...",
                                    "PDF",
                                    "HTML",
                                    "Markdown",
                                    "Close",
                                ];
                                let name = names
                                    .get(event.index)
                                    .copied()
                                    .unwrap_or("Unknown");
                                this.last_action = format!(
                                    "File menu: {} (index {})",
                                    name, event.index
                                );
                                this.selection_count += 1;
                                cx.notify();
                            },
                        )),
                    ))
                    .item(NativeToolbarItem::FlexibleSpace)
                    .item(NativeToolbarItem::MenuButton(
                        NativeToolbarMenuButton::new(
                            "settings_menu",
                            "Settings",
                            vec![
                                NativeToolbarMenuItem::action("Preferences"),
                                NativeToolbarMenuItem::separator(),
                                NativeToolbarMenuItem::submenu(
                                    "Theme",
                                    vec![
                                        NativeToolbarMenuItem::action("Light"),
                                        NativeToolbarMenuItem::action("Dark"),
                                        NativeToolbarMenuItem::action("System"),
                                    ],
                                ),
                                NativeToolbarMenuItem::submenu(
                                    "Font Size",
                                    vec![
                                        NativeToolbarMenuItem::action("Small"),
                                        NativeToolbarMenuItem::action("Medium"),
                                        NativeToolbarMenuItem::action("Large"),
                                    ],
                                ),
                            ],
                        )
                        .icon("gearshape")
                        .tool_tip("Application settings")
                        .shows_indicator(true)
                        .on_select(cx.listener(
                            |this, event: &NativeToolbarMenuButtonSelectEvent, _, cx| {
                                let names = [
                                    "Preferences",
                                    "Light",
                                    "Dark",
                                    "System",
                                    "Small",
                                    "Medium",
                                    "Large",
                                ];
                                let name = names
                                    .get(event.index)
                                    .copied()
                                    .unwrap_or("Unknown");
                                this.last_action = format!(
                                    "Settings menu: {} (index {})",
                                    name, event.index
                                );
                                this.selection_count += 1;
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
            .child(div().text_xl().child("NSMenuToolbarItem Demo"))
            .child(
                div().text_sm().text_color(muted).child(
                    "Toolbar buttons with dropdown menus (File and Settings above).",
                ),
            )
            .child(
                div()
                    .text_lg()
                    .child(format!("Selections: {}", self.selection_count)),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(format!("Last action: {}", self.last_action)),
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
                cx.new(|_| MenuToolbarExample {
                    toolbar_installed: false,
                    last_action: "<none>".to_string(),
                    selection_count: 0,
                })
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
