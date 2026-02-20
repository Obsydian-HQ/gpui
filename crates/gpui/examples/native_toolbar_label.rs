use gpui::{
    App, Application, Bounds, Context, NativeToolbar, NativeToolbarButton, NativeToolbarClickEvent,
    NativeToolbarDisplayMode, NativeToolbarItem, NativeToolbarLabel, NativeToolbarMenuButton,
    NativeToolbarMenuButtonSelectEvent, NativeToolbarMenuItem, NativeToolbarSizeMode, Window,
    WindowAppearance, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};

struct NativeToolbarLabelExample {
    toolbar_installed: bool,
    last_action: String,
}

impl Render for NativeToolbarLabelExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.toolbar_installed {
            window.set_native_toolbar(Some(
                NativeToolbar::new("gpui.native.toolbar.label.example")
                    .title("GPUI Label Example")
                    .display_mode(NativeToolbarDisplayMode::IconAndLabel)
                    .size_mode(NativeToolbarSizeMode::Regular)
                    .item(NativeToolbarItem::Button(
                        NativeToolbarButton::new("action", "Action")
                            .icon("play.circle")
                            .on_click(cx.listener(
                                |this, _: &NativeToolbarClickEvent, _, cx| {
                                    this.last_action = "clicked Action button".to_string();
                                    cx.notify();
                                },
                            )),
                    ))
                    .item(NativeToolbarItem::Label(
                        NativeToolbarLabel::new("image_info", "1920x1080 \u{2022} 2.4 MB \u{2022} PNG"),
                    ))
                    .item(NativeToolbarItem::FlexibleSpace)
                    .item(NativeToolbarItem::Label(
                        NativeToolbarLabel::new("status", "Ready"),
                    ))
                    .item(NativeToolbarItem::MenuButton(
                        NativeToolbarMenuButton::new(
                            "options",
                            "Options",
                            vec![
                                NativeToolbarMenuItem::action("Option A"),
                                NativeToolbarMenuItem::action("Option B"),
                                NativeToolbarMenuItem::separator(),
                                NativeToolbarMenuItem::action("Option C"),
                            ],
                        )
                        .icon("ellipsis.circle")
                        .on_select(cx.listener(
                            |this, event: &NativeToolbarMenuButtonSelectEvent, _, cx| {
                                this.last_action = format!("selected menu index {}", event.index);
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
            .child(div().text_xl().child("Native Toolbar Label Demo"))
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("The toolbar above includes Label items (plain text, no button chrome)."),
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
                cx.new(|_| NativeToolbarLabelExample {
                    toolbar_installed: false,
                    last_action: "<none>".to_string(),
                })
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
