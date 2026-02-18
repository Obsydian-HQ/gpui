use gpui::{
    App, Application, Bounds, Context, FocusHandle, Focusable, KeyBinding, Menu, MenuItem,
    Window, WindowBounds, WindowOptions, actions, div, native_sidebar, prelude::*, px, size,
};

actions!(native_sidebar_example, [ToggleSidebar]);

struct SidebarExample {
    collapsed: bool,
    focus_handle: FocusHandle,
}

impl SidebarExample {
    const ITEMS: [&str; 12] = [
        "Home",
        "Projects",
        "Tasks",
        "Pull Requests",
        "Issues",
        "Discussions",
        "Builds",
        "Deployments",
        "Secrets",
        "Members",
        "Settings",
        "Billing",
    ];
}

impl Render for SidebarExample {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(
                |this: &mut SidebarExample, _: &ToggleSidebar, _, cx| {
                    this.collapsed = !this.collapsed;
                    cx.notify();
                },
            ))
            .child(
                native_sidebar("sidebar", &Self::ITEMS)
                    .selected_index(Some(0))
                    .sidebar_width(260.0)
                    .min_sidebar_width(180.0)
                    .max_sidebar_width(420.0)
                    .collapsed(self.collapsed)
                    .size_full(),
            )
    }
}

impl Focusable for SidebarExample {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.bind_keys([KeyBinding::new("cmd-alt-s", ToggleSidebar, None)]);
        cx.set_menus(vec![Menu {
            name: "View".into(),
            items: vec![MenuItem::action("Toggle Sidebar", ToggleSidebar)],
        }]);

        let bounds = Bounds::centered(None, size(px(1100.), px(760.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                cx.new(|cx| {
                    let focus_handle = cx.focus_handle();
                    focus_handle.focus(window, cx);
                    SidebarExample {
                        collapsed: false,
                        focus_handle,
                    }
                })
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
