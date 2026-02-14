use gpui::{
    App, Application, Bounds, CheckboxChangeEvent, Context, Window, WindowAppearance, WindowBounds,
    WindowOptions, div, native_checkbox, prelude::*, px, rgb, size,
};

struct CheckboxExample {
    auto_update: bool,
    share_analytics: bool,
}

impl Render for CheckboxExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (bg, fg, muted) = if is_dark {
            (rgb(0x1f1f1f), rgb(0xffffff), rgb(0xbdbdbd))
        } else {
            (rgb(0xf2f2f2), rgb(0x1a1a1a), rgb(0x5a5a5a))
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .justify_center()
            .items_center()
            .gap_3()
            .bg(bg)
            .text_color(fg)
            .child(div().text_xl().child("Native Checkbox Demo"))
            .child(
                native_checkbox("auto_update", "Enable automatic updates")
                    .checked(self.auto_update)
                    .on_change(cx.listener(|this, event: &CheckboxChangeEvent, _, cx| {
                        this.auto_update = event.checked;
                        cx.notify();
                    })),
            )
            .child(
                native_checkbox("share_analytics", "Share anonymous analytics")
                    .checked(self.share_analytics)
                    .on_change(cx.listener(|this, event: &CheckboxChangeEvent, _, cx| {
                        this.share_analytics = event.checked;
                        cx.notify();
                    })),
            )
            .child(div().text_sm().text_color(muted).child(format!(
                "Auto update: {} | Analytics: {}",
                self.auto_update, self.share_analytics
            )))
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(520.), px(280.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| CheckboxExample {
                    auto_update: true,
                    share_analytics: false,
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
