use gpui::{
    App, Application, Bounds, ComboBoxChangeEvent, ComboBoxSelectEvent, Context, Window,
    WindowAppearance, WindowBounds, WindowOptions, div, native_combo_box, prelude::*, px, rgb,
    size,
};

struct ComboBoxExample {
    language_index: usize,
    filter_text: String,
}

impl ComboBoxExample {
    const LANGUAGES: [&str; 6] = ["Rust", "TypeScript", "Go", "Swift", "Python", "Kotlin"];
    const COMMANDS: [&str; 6] = [
        "open project",
        "open settings",
        "toggle sidebar",
        "new terminal",
        "run tests",
        "search in files",
    ];
}

impl Render for ComboBoxExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (bg, fg, muted) = if is_dark {
            (rgb(0x1f2127), rgb(0xffffff), rgb(0xb9bec7))
        } else {
            (rgb(0xf4f6f9), rgb(0x1b2230), rgb(0x5f6978))
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
            .child(div().text_xl().child("Native Combo Box Demo"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child("Language:")
                    .child(
                        native_combo_box("language_combo", &Self::LANGUAGES)
                            .selected_index(self.language_index)
                            .on_select(cx.listener(|this, event: &ComboBoxSelectEvent, _, cx| {
                                this.language_index = event.index;
                                cx.notify();
                            }))
                            .w(px(220.0)),
                    ),
            )
            .child(
                div().flex().items_center().gap_3().child("Command:").child(
                    native_combo_box("command_combo", &Self::COMMANDS)
                        .editable(true)
                        .text(self.filter_text.clone())
                        .on_change(cx.listener(|this, event: &ComboBoxChangeEvent, _, cx| {
                            this.filter_text = event.text.clone();
                            cx.notify();
                        }))
                        .w(px(240.0)),
                ),
            )
            .child(div().text_sm().text_color(muted).child(format!(
                "Selected language: {}",
                Self::LANGUAGES[self.language_index]
            )))
            .child(div().text_sm().text_color(muted).child(format!(
                "Editable text: {}",
                if self.filter_text.is_empty() {
                    "<empty>".to_string()
                } else {
                    self.filter_text.clone()
                }
            )))
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(680.), px(360.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| ComboBoxExample {
                    language_index: 0,
                    filter_text: String::new(),
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
