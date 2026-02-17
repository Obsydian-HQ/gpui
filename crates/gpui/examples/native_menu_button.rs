use gpui::{
    App, Application, Bounds, Context, MenuItemSelectEvent, NativeMenuItem, Window,
    WindowAppearance, WindowBounds, WindowOptions, div, native_context_menu, native_menu_button,
    prelude::*, px, rgb, size,
};

struct MenuExample {
    selected_index: Option<usize>,
}

impl MenuExample {
    fn menu() -> Vec<NativeMenuItem> {
        vec![
            NativeMenuItem::action("Open"),
            NativeMenuItem::action("Duplicate"),
            NativeMenuItem::separator(),
            NativeMenuItem::submenu(
                "Export",
                vec![
                    NativeMenuItem::action("PNG"),
                    NativeMenuItem::action("PDF"),
                    NativeMenuItem::action("SVG"),
                ],
            ),
            NativeMenuItem::separator(),
            NativeMenuItem::action("Delete").enabled(false),
        ]
    }
}

impl Render for MenuExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (bg, fg, muted) = if is_dark {
            (rgb(0x171b21), rgb(0xffffff), rgb(0xb1bbc8))
        } else {
            (rgb(0xf4f7fb), rgb(0x1a2434), rgb(0x5f6a7b))
        };

        let menu = Self::menu();

        div()
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .justify_center()
            .gap_4()
            .bg(bg)
            .text_color(fg)
            .child(div().text_xl().child("Native MenuButton + ContextMenu"))
            .child(
                native_menu_button("actions", "Actions", &menu).on_select(cx.listener(
                    |this, event: &MenuItemSelectEvent, _, cx| {
                        this.selected_index = Some(event.index);
                        cx.notify();
                    },
                )),
            )
            .child(
                native_context_menu("context", "Right click me", &menu).on_select(cx.listener(
                    |this, event: &MenuItemSelectEvent, _, cx| {
                        this.selected_index = Some(event.index);
                        cx.notify();
                    },
                )),
            )
            .child(div().text_sm().text_color(muted).child(format!(
                "Selected action index: {}",
                self.selected_index
                    .map(|idx| idx.to_string())
                    .unwrap_or_else(|| "<none>".to_string())
            )))
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(560.), px(360.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| MenuExample {
                    selected_index: None,
                })
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
