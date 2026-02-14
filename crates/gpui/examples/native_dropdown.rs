use gpui::{
    App, Application, Bounds, Context, DropdownSelectEvent, Window, WindowAppearance, WindowBounds,
    WindowOptions, div, native_dropdown, prelude::*, px, rgb, size,
};

struct DropdownExample {
    language_index: usize,
    theme_index: usize,
}

impl DropdownExample {
    const LANGUAGES: [&str; 4] = ["Rust", "TypeScript", "Go", "Swift"];
    const THEMES: [&str; 3] = ["System", "Light", "Dark"];
}

impl Render for DropdownExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (bg, fg, muted) = if is_dark {
            (rgb(0x1e1e1e), rgb(0xffffff), rgb(0xbdbdbd))
        } else {
            (rgb(0xf0f0f0), rgb(0x1a1a1a), rgb(0x666666))
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .justify_center()
            .items_center()
            .gap_4()
            .bg(bg)
            .text_color(fg)
            .child(div().text_xl().child("Native Dropdown Demo"))
            .child(
                div()
                    .flex()
                    .gap_3()
                    .items_center()
                    .child("Language:")
                    .child(
                        native_dropdown("language", &Self::LANGUAGES)
                            .selected_index(self.language_index)
                            .on_select(cx.listener(|this, event: &DropdownSelectEvent, _, cx| {
                                this.language_index = event.index;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div().flex().gap_3().items_center().child("Theme:").child(
                    native_dropdown("theme", &Self::THEMES)
                        .selected_index(self.theme_index)
                        .on_select(cx.listener(|this, event: &DropdownSelectEvent, _, cx| {
                            this.theme_index = event.index;
                            cx.notify();
                        })),
                ),
            )
            .child(div().text_sm().text_color(muted).child(format!(
                "Selected: {} / {}",
                Self::LANGUAGES[self.language_index],
                Self::THEMES[self.theme_index]
            )))
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(560.), px(320.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| DropdownExample {
                    language_index: 0,
                    theme_index: 0,
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
