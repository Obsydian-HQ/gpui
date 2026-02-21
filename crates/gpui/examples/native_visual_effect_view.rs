use gpui::{
    App, Application, Bounds, Context, NativeGlassEffectStyle, NativeVisualEffectBlendingMode,
    NativeVisualEffectMaterial, TitlebarOptions, Window, WindowAppearance, WindowBounds,
    WindowOptions, div, native_glass_effect_view, native_visual_effect_view, prelude::*, px, rgb,
    size,
};

struct VisualEffectExample;

impl VisualEffectExample {
    fn render_material_row(
        prefix: &str,
        blending_mode: NativeVisualEffectBlendingMode,
        materials: &[(&str, NativeVisualEffectMaterial)],
        fg: gpui::Rgba,
        muted: gpui::Rgba,
    ) -> gpui::Div {
        let mut row = div().flex().flex_row().gap_3().justify_center().flex_wrap();
        for (idx, (label, material)) in materials.iter().enumerate() {
            row = row.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .child(
                        native_visual_effect_view(format!("{prefix}-{idx}"), *material)
                            .blending_mode(blending_mode)
                            .corner_radius(8.0)
                            .w(px(90.0))
                            .h(px(90.0)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(fg)
                            .child(label.to_string()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .max_w(px(90.0))
                            .text_center()
                            .child(format!("{blending_mode:?}")),
                    ),
            );
        }
        row
    }
}

impl Render for VisualEffectExample {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let fg = if is_dark { rgb(0xffffff) } else { rgb(0x1a1a1a) };
        let muted = if is_dark { rgb(0x999999) } else { rgb(0x666666) };

        let stripe_colors = [
            rgb(0xe74c3c),
            rgb(0xe67e22),
            rgb(0xf1c40f),
            rgb(0x2ecc71),
            rgb(0x3498db),
            rgb(0x9b59b6),
        ];
        let mut bg_stripes = div().flex().flex_row().size_full().absolute().top_0().left_0();
        for color in &stripe_colors {
            bg_stripes = bg_stripes.child(div().flex_1().h_full().bg(*color));
        }

        let materials: Vec<(&str, NativeVisualEffectMaterial)> = vec![
            ("Sidebar", NativeVisualEffectMaterial::Sidebar),
            ("HeaderView", NativeVisualEffectMaterial::HeaderView),
            ("Menu", NativeVisualEffectMaterial::Menu),
            ("HudWindow", NativeVisualEffectMaterial::HudWindow),
            ("ContentBg", NativeVisualEffectMaterial::ContentBackground),
            ("UnderWindow", NativeVisualEffectMaterial::UnderWindow),
        ];

        // Row 1: WithinWindow blending — blurs content within the window behind these views
        let within_window_row = Self::render_material_row(
            "within",
            NativeVisualEffectBlendingMode::WithinWindow,
            &materials,
            fg,
            muted,
        );

        // Row 2: BehindWindow blending — blurs content behind the window (desktop, other apps)
        let behind_window_row = Self::render_material_row(
            "behind",
            NativeVisualEffectBlendingMode::BehindWindow,
            &materials,
            fg,
            muted,
        );

        // Row 3: Liquid Glass (macOS 26+ NSGlassEffectView)
        let glass_styles = [
            ("Regular", NativeGlassEffectStyle::Regular),
            ("Clear", NativeGlassEffectStyle::Clear),
        ];
        let mut glass_row = div().flex().flex_row().gap_3().justify_center().flex_wrap();
        for (idx, (label, glass_style)) in glass_styles.iter().enumerate() {
            glass_row = glass_row.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .child(
                        native_glass_effect_view(format!("glass-{idx}"), *glass_style)
                            .corner_radius(8.0)
                            .w(px(90.0))
                            .h(px(90.0)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(fg)
                            .child(label.to_string()),
                    ),
            );
        }
        // Tinted glass variants
        let tint_colors = [
            ("Red Tint", gpui::hsla(0.0, 0.8, 0.5, 0.6)),
            ("Blue Tint", gpui::hsla(0.6, 0.8, 0.5, 0.6)),
            ("Green Tint", gpui::hsla(0.33, 0.8, 0.5, 0.6)),
            ("Purple Tint", gpui::hsla(0.75, 0.8, 0.5, 0.6)),
        ];
        for (idx, (label, tint)) in tint_colors.iter().enumerate() {
            glass_row = glass_row.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .child(
                        native_glass_effect_view(
                            format!("glass-tint-{idx}"),
                            NativeGlassEffectStyle::Regular,
                        )
                        .tint_color(*tint)
                        .corner_radius(8.0)
                        .w(px(90.0))
                        .h(px(90.0)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(fg)
                            .child(label.to_string()),
                    ),
            );
        }

        div()
            .size_full()
            .relative()
            .child(bg_stripes)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .size_full()
                    .relative()
                    .items_center()
                    .gap_3()
                    .pt_8()
                    .child(
                        div()
                            .text_lg()
                            .text_color(fg)
                            .child("Native Materials & Liquid Glass"),
                    )
                    // Row 1: WithinWindow
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child("WithinWindow — blurs content inside the window"),
                    )
                    .child(within_window_row)
                    // Row 2: BehindWindow
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child("BehindWindow — blurs desktop/apps behind the window"),
                    )
                    .child(behind_window_row)
                    // Row 3: Liquid Glass
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child("Liquid Glass (macOS 26+) — NSGlassEffectView"),
                    )
                    .child(glass_row),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(900.), px(700.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    appears_transparent: true,
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|_| VisualEffectExample),
        )
        .unwrap();
        cx.activate(true);
    });
}
