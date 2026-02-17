use gpui::{
    App, Application, Bounds, CheckboxChangeEvent, Context, DropdownSelectEvent,
    NativeTableGridMask, NativeTableRowSizeStyle, NativeTableSelectionHighlightStyle,
    NativeTableStyle, TableRowSelectEvent, Window, WindowAppearance, WindowBounds, WindowOptions,
    div, native_checkbox, native_dropdown, native_table_view, prelude::*, px, rgb, size,
};

struct TableStylesExample {
    style_index: usize,
    row_size_index: usize,
    show_header: bool,
    alternating_rows: bool,
    selection_highlight: bool,
    vertical_grid: bool,
    horizontal_grid: bool,
    dashed_horizontal_grid: bool,
    selected: Option<usize>,
}

impl TableStylesExample {
    const STYLES: [&str; 5] = ["Automatic", "FullWidth", "Inset", "SourceList", "Plain"];
    const ROW_SIZES: [&str; 5] = ["Default", "Custom", "Small", "Medium", "Large"];
    const ROWS: [&str; 14] = [
        "Calendar",
        "Mail",
        "Notes",
        "Music",
        "Maps",
        "Photos",
        "Books",
        "TV",
        "Stocks",
        "Weather",
        "Shortcuts",
        "Xcode",
        "Terminal",
        "Activity Monitor",
    ];

    fn table_style(&self) -> NativeTableStyle {
        match self.style_index {
            1 => NativeTableStyle::FullWidth,
            2 => NativeTableStyle::Inset,
            3 => NativeTableStyle::SourceList,
            4 => NativeTableStyle::Plain,
            _ => NativeTableStyle::Automatic,
        }
    }

    fn row_size_style(&self) -> NativeTableRowSizeStyle {
        match self.row_size_index {
            0 => NativeTableRowSizeStyle::Default,
            1 => NativeTableRowSizeStyle::Custom,
            2 => NativeTableRowSizeStyle::Small,
            3 => NativeTableRowSizeStyle::Medium,
            4 => NativeTableRowSizeStyle::Large,
            _ => NativeTableRowSizeStyle::Custom,
        }
    }

    fn grid_mask(&self) -> NativeTableGridMask {
        let mut mask = NativeTableGridMask::NONE;
        if self.vertical_grid {
            mask = mask.union(NativeTableGridMask::SOLID_VERTICAL);
        }
        if self.horizontal_grid {
            mask = mask.union(NativeTableGridMask::SOLID_HORIZONTAL);
        }
        if self.dashed_horizontal_grid {
            mask = mask.union(NativeTableGridMask::DASHED_HORIZONTAL);
        }
        mask
    }
}

impl Render for TableStylesExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (bg, fg, muted) = if is_dark {
            (rgb(0x191d24), rgb(0xffffff), rgb(0xb5bdcb))
        } else {
            (rgb(0xf4f7fb), rgb(0x1a2434), rgb(0x5f6a7b))
        };

        let table_style = self.table_style();
        let row_size_style = self.row_size_style();
        let selection_highlight_style = if self.selection_highlight {
            NativeTableSelectionHighlightStyle::Regular
        } else {
            NativeTableSelectionHighlightStyle::None
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .gap_3()
            .p_4()
            .bg(bg)
            .text_color(fg)
            .child(div().text_xl().child("Native TableView Styles"))
            .child(
                div()
                    .flex()
                    .gap_3()
                    .items_center()
                    .child("Table style")
                    .child(
                        native_dropdown("style", &Self::STYLES)
                            .selected_index(self.style_index)
                            .on_select(cx.listener(|this, event: &DropdownSelectEvent, _, cx| {
                                this.style_index = event.index;
                                cx.notify();
                            }))
                            .w(px(180.0)),
                    )
                    .child("Row size")
                    .child(
                        native_dropdown("row_size", &Self::ROW_SIZES)
                            .selected_index(self.row_size_index)
                            .on_select(cx.listener(|this, event: &DropdownSelectEvent, _, cx| {
                                this.row_size_index = event.index;
                                cx.notify();
                            }))
                            .w(px(160.0)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_3()
                    .child(
                        native_checkbox("show_header", "Show header")
                            .checked(self.show_header)
                            .on_change(cx.listener(|this, event: &CheckboxChangeEvent, _, cx| {
                                this.show_header = event.checked;
                                cx.notify();
                            })),
                    )
                    .child(
                        native_checkbox("alternating", "Alternating rows")
                            .checked(self.alternating_rows)
                            .on_change(cx.listener(|this, event: &CheckboxChangeEvent, _, cx| {
                                this.alternating_rows = event.checked;
                                cx.notify();
                            })),
                    )
                    .child(
                        native_checkbox("highlight", "Selection highlight")
                            .checked(self.selection_highlight)
                            .on_change(cx.listener(|this, event: &CheckboxChangeEvent, _, cx| {
                                this.selection_highlight = event.checked;
                                cx.notify();
                            })),
                    )
                    .child(
                        native_checkbox("grid_v", "Vertical grid")
                            .checked(self.vertical_grid)
                            .on_change(cx.listener(|this, event: &CheckboxChangeEvent, _, cx| {
                                this.vertical_grid = event.checked;
                                cx.notify();
                            })),
                    )
                    .child(
                        native_checkbox("grid_h", "Horizontal grid")
                            .checked(self.horizontal_grid)
                            .on_change(cx.listener(|this, event: &CheckboxChangeEvent, _, cx| {
                                this.horizontal_grid = event.checked;
                                cx.notify();
                            })),
                    )
                    .child(
                        native_checkbox("grid_dh", "Dashed horizontal")
                            .checked(self.dashed_horizontal_grid)
                            .on_change(cx.listener(|this, event: &CheckboxChangeEvent, _, cx| {
                                this.dashed_horizontal_grid = event.checked;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                native_table_view("table", &Self::ROWS)
                    .table_style(table_style)
                    .row_size_style(row_size_style)
                    .selection_highlight_style(selection_highlight_style)
                    .column_title("Application")
                    .show_header(self.show_header)
                    .alternating_rows(self.alternating_rows)
                    .grid_mask(self.grid_mask())
                    .selected_index(self.selected)
                    .row_height(24.0)
                    .on_select(cx.listener(|this, event: &TableRowSelectEvent, _, cx| {
                        this.selected = Some(event.index);
                        cx.notify();
                    }))
                    .h(px(320.0)),
            )
            .child(div().text_sm().text_color(muted).child(format!(
                "Selected: {}",
                self.selected
                    .map(|idx| Self::ROWS[idx].to_string())
                    .unwrap_or_else(|| "<none>".to_string())
            )))
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(920.), px(680.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| TableStylesExample {
                    style_index: 2,
                    row_size_index: 1,
                    show_header: true,
                    alternating_rows: true,
                    selection_highlight: true,
                    vertical_grid: false,
                    horizontal_grid: false,
                    dashed_horizontal_grid: false,
                    selected: None,
                })
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
