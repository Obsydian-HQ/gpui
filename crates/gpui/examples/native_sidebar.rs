use gpui::{
    App, Application, Bounds, Context, Window, WindowBounds, WindowOptions, native_sidebar,
    prelude::*, px, size,
};

struct SidebarExample;

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
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        native_sidebar("sidebar", &Self::ITEMS)
            .selected_index(Some(0))
            .sidebar_width(260.0)
            .min_sidebar_width(180.0)
            .max_sidebar_width(420.0)
            .size_full()
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1100.), px(760.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| SidebarExample),
        )
        .unwrap();

        cx.activate(true);
    });
}
