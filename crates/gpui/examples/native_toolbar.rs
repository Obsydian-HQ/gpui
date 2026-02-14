#[cfg(target_os = "macos")]
use gpui::{
    App, Application, Bounds, Context, SharedString, Window, WindowAppearance, WindowBounds,
    WindowOptions, WindowToolbarButtonOptions, WindowToolbarDisplayMode, WindowToolbarGroupItem,
    WindowToolbarGroupOptions, WindowToolbarItem, WindowToolbarItemIdentifier,
    WindowToolbarItemKind, WindowToolbarOptions, WindowToolbarSearchFieldOptions,
    WindowToolbarStyle, WindowToolbarSwitchOptions, WindowToolbarTrackingSeparatorOptions, div,
    prelude::*, px, rgb, size,
};
#[cfg(target_os = "macos")]
use std::ffi::c_void;

#[cfg(target_os = "macos")]
struct NativeToolbarExample;

#[cfg(target_os = "macos")]
impl Render for NativeToolbarExample {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (bg, fg, muted) = if is_dark {
            (rgb(0x1f2228), rgb(0xf0f3f7), rgb(0xaab2bf))
        } else {
            (rgb(0xf7f8fa), rgb(0x1f2b3a), rgb(0x5f6b7a))
        };

        div()
            .size_full()
            .bg(bg)
            .text_color(fg)
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_xl().child("Native macOS Toolbar"))
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Exercises NSButton, NSToolbarItemGroup, NSSearchToolbarItem, NSSwitch."),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Toolbar callbacks are logged to stdout."),
            )
    }
}

#[cfg(target_os = "macos")]
fn custom(id: &str) -> WindowToolbarItemIdentifier {
    WindowToolbarItemIdentifier::Custom(SharedString::from(id))
}

#[cfg(target_os = "macos")]
fn build_toolbar_options(split_view: *mut c_void) -> WindowToolbarOptions {
    let refresh_item = WindowToolbarItem {
        identifier: "refresh".into(),
        label: "Refresh".into(),
        palette_label: Some("Refresh Data".into()),
        tool_tip: Some("Reload current contents".into()),
        kind: WindowToolbarItemKind::Button(WindowToolbarButtonOptions {
            title: Some("Refresh".into()),
            sf_symbol: Some("arrow.clockwise".into()),
            bordered: true,
            on_click: Some(Box::new(|| {
                println!("[native_toolbar] refresh clicked");
            })),
        }),
    };

    let mode_group_item = WindowToolbarItem {
        identifier: "mode_group".into(),
        label: "Mode".into(),
        palette_label: Some("Display Mode".into()),
        tool_tip: Some("Switch display modes".into()),
        kind: WindowToolbarItemKind::Group(WindowToolbarGroupOptions {
            items: vec![
                WindowToolbarGroupItem {
                    label: "List".into(),
                    sf_symbol: Some("list.bullet".into()),
                },
                WindowToolbarGroupItem {
                    label: "Grid".into(),
                    sf_symbol: Some("square.grid.2x2".into()),
                },
                WindowToolbarGroupItem {
                    label: "Gallery".into(),
                    sf_symbol: Some("photo.on.rectangle".into()),
                },
            ],
            selected_index: Some(0),
            on_change: Some(Box::new(|index| {
                println!("[native_toolbar] mode group selected index: {index}");
            })),
            ..Default::default()
        }),
    };

    let search_item = WindowToolbarItem {
        identifier: "search".into(),
        label: "Search".into(),
        palette_label: Some("Search".into()),
        tool_tip: Some("Search files and symbols".into()),
        kind: WindowToolbarItemKind::SearchField(WindowToolbarSearchFieldOptions {
            placeholder: Some("Search".into()),
            preferred_width: Some(px(240.0)),
            on_change: Some(Box::new(|text| {
                println!("[native_toolbar] search change: {text}");
            })),
            on_submit: Some(Box::new(|text| {
                println!("[native_toolbar] search submit: {text}");
            })),
            ..Default::default()
        }),
    };

    let preview_switch_item = WindowToolbarItem {
        identifier: "preview".into(),
        label: "Preview".into(),
        palette_label: Some("Preview".into()),
        tool_tip: Some("Toggle preview".into()),
        kind: WindowToolbarItemKind::Switch(WindowToolbarSwitchOptions {
            title: Some("Preview".into()),
            checked: true,
            on_toggle: Some(Box::new(|enabled| {
                println!("[native_toolbar] preview toggled: {enabled}");
            })),
        }),
    };

    let tracking_separator_item = WindowToolbarItem {
        identifier: "tracking_separator".into(),
        label: "Sidebar Divider".into(),
        palette_label: Some("Sidebar Divider".into()),
        tool_tip: Some("Tracking separator (provide split_view pointer)".into()),
        kind: WindowToolbarItemKind::TrackingSeparator(WindowToolbarTrackingSeparatorOptions {
            split_view,
            divider_index: 0,
        }),
    };

    WindowToolbarOptions {
        identifier: "gpui.native_toolbar.example".into(),
        style: WindowToolbarStyle::Unified,
        display_mode: WindowToolbarDisplayMode::IconAndLabel,
        allows_user_customization: true,
        autosaves_configuration: false,
        shows_baseline_separator: true,
        items: vec![
            refresh_item,
            mode_group_item,
            search_item,
            preview_switch_item,
            tracking_separator_item,
        ],
        default_item_identifiers: vec![
            WindowToolbarItemIdentifier::ToggleSidebar,
            custom("tracking_separator"),
            WindowToolbarItemIdentifier::Separator,
            custom("refresh"),
            WindowToolbarItemIdentifier::Space,
            custom("mode_group"),
            WindowToolbarItemIdentifier::FlexibleSpace,
            custom("search"),
            WindowToolbarItemIdentifier::Space,
            custom("preview"),
        ],
        allowed_item_identifiers: vec![
            custom("refresh"),
            custom("mode_group"),
            custom("search"),
            custom("preview"),
            custom("tracking_separator"),
            WindowToolbarItemIdentifier::Space,
            WindowToolbarItemIdentifier::FlexibleSpace,
            WindowToolbarItemIdentifier::Separator,
            WindowToolbarItemIdentifier::ToggleSidebar,
        ],
        selectable_item_identifiers: vec![custom("mode_group")],
        centered_item_identifier: None,
        selected_item_identifier: Some(custom("mode_group")),
    }
}

#[cfg(target_os = "macos")]
fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(920.), px(620.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let _ = window.install_native_sidebar(
                    px(160.),
                    px(260.),
                    px(420.),
                    Some("gpui.native_toolbar.example.sidebar"),
                );
                let split_view = window.raw_native_sidebar_split_view_ptr();
                window.set_native_toolbar_options(Some(build_toolbar_options(split_view)));
                cx.new(|_| NativeToolbarExample)
            },
        )
        .unwrap();
        cx.activate(true);
    });
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("native_toolbar example is only available on macOS");
}
