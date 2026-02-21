use gpui::{
    App, Application, Bounds, Context, NativeVisualEffectMaterial, TitlebarOptions, Window,
    WindowAppearance, WindowBounds, WindowOptions, div, native_button, native_image_view,
    native_tracking_view, native_visual_effect_view, prelude::*, px, rgb, size,
};

struct TabInfo {
    title: String,
    icon: &'static str,
}

struct TabBarExample {
    tabs: Vec<TabInfo>,
    selected: usize,
    hovered: Option<usize>,
    next_id: usize,
}

impl TabBarExample {
    fn new() -> Self {
        Self {
            tabs: vec![
                TabInfo {
                    title: "Home".into(),
                    icon: "house.fill",
                },
                TabInfo {
                    title: "Documents".into(),
                    icon: "doc.text.fill",
                },
                TabInfo {
                    title: "Settings".into(),
                    icon: "gearshape.fill",
                },
            ],
            selected: 0,
            hovered: None,
            next_id: 3,
        }
    }
}

impl Render for TabBarExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let fg = if is_dark { rgb(0xffffff) } else { rgb(0x1a1a1a) };
        let muted = if is_dark { rgb(0x999999) } else { rgb(0x666666) };
        let selected_bg = if is_dark {
            rgb(0x444444)
        } else {
            rgb(0xdddddd)
        };
        let hover_bg = if is_dark {
            rgb(0x383838)
        } else {
            rgb(0xe8e8e8)
        };

        let mut tab_items = div().flex().flex_row().gap_1().px_2().items_center();

        for (idx, tab) in self.tabs.iter().enumerate() {
            let is_selected = idx == self.selected;
            let is_hovered = self.hovered == Some(idx);

            let tint = if is_selected {
                (0.0, 0.478, 1.0, 1.0)
            } else if is_dark {
                (0.8, 0.8, 0.8, 1.0)
            } else {
                (0.3, 0.3, 0.3, 1.0)
            };

            let tab_element = div()
                .id(format!("tab-click-{idx}"))
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .px_2()
                .py_1()
                .rounded(px(6.0))
                .cursor_pointer()
                .when(is_selected, |el| el.bg(selected_bg))
                .when(is_hovered && !is_selected, |el| el.bg(hover_bg))
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    this.selected = idx;
                    cx.notify();
                }))
                .child(
                    native_image_view(format!("tab-icon-{idx}"))
                        .sf_symbol(tab.icon)
                        .tint_color(tint.0, tint.1, tint.2, tint.3)
                        .w(px(14.0))
                        .h(px(14.0)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(if is_selected { fg } else { muted })
                        .child(tab.title.clone()),
                )
                .child(
                    native_tracking_view(format!("tab-track-{idx}"))
                        .on_mouse_enter(cx.listener(move |this, _event, _window, cx| {
                            this.hovered = Some(idx);
                            cx.notify();
                        }))
                        .on_mouse_exit(cx.listener(|this, _event, _window, cx| {
                            this.hovered = None;
                            cx.notify();
                        }))
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                )
                .when(is_hovered, |el| {
                    el.child(
                        div()
                            .id(format!("close-{idx}"))
                            .cursor_pointer()
                            .text_xs()
                            .text_color(muted)
                            .ml_1()
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                if this.tabs.len() > 1 {
                                    this.tabs.remove(idx);
                                    if this.selected >= this.tabs.len() {
                                        this.selected = this.tabs.len().saturating_sub(1);
                                    }
                                    this.hovered = None;
                                    cx.notify();
                                }
                            }))
                            .child("x"),
                    )
                });

            tab_items = tab_items.child(tab_element);
        }

        // Add tab button
        tab_items = tab_items.child(
            native_button("add-tab", "+")
                .on_click(cx.listener(|this, _event, _window, cx| {
                    let id = this.next_id;
                    this.next_id += 1;
                    this.tabs.push(TabInfo {
                        title: format!("Tab {id}"),
                        icon: "doc.fill",
                    });
                    this.selected = this.tabs.len() - 1;
                    cx.notify();
                }))
                .w(px(28.0))
                .h(px(20.0)),
        );

        let selected_tab = self.tabs.get(self.selected);
        let content_text = selected_tab
            .map(|t| format!("Content: {}", t.title))
            .unwrap_or_else(|| "No tab selected".into());

        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(38.0))
                    .child(
                        native_visual_effect_view("tab-bar-bg", NativeVisualEffectMaterial::HeaderView)
                            .w_full()
                            .h(px(38.0)),
                    )
                    .child(tab_items.h(px(38.0))),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .justify_center()
                    .items_center()
                    .text_color(fg)
                    .text_xl()
                    .child(content_text),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(700.), px(450.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    appears_transparent: true,
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|_| TabBarExample::new()),
        )
        .unwrap();
        cx.activate(true);
    });
}
