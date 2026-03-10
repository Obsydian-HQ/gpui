use refineable::Refineable as _;
use std::rc::Rc;

use crate::platform::native_controls::{NativeControlState, SidebarViewConfig};
use crate::{
    AbsoluteLength, AnyView, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    HostedContentConfig, Hsla, InspectorElementId, IntoElement, LayoutId, Length, Pixels,
    Render, SharedString, Style, StyleRefinement, Styled, Window, px,
};

#[cfg(target_os = "macos")]
use crate::SurfaceId;

use super::native_element_helpers::schedule_native_callback;

/// Event emitted when a native sidebar item is selected.
#[derive(Clone, Debug)]
pub struct SidebarSelectEvent {
    /// Zero-based selected item index.
    pub index: usize,
    /// Selected item title.
    pub title: SharedString,
}

/// Event emitted when a native sidebar header button is clicked.
#[derive(Clone, Debug)]
pub struct SidebarHeaderClickEvent {
    /// Zero-based button index in the header.
    pub index: usize,
    /// The button's user-defined ID.
    pub id: SharedString,
}

/// A button to display in the native sidebar header.
pub struct NativeSidebarHeaderButton {
    pub(crate) id: SharedString,
    pub(crate) symbol: SharedString,
}

impl NativeSidebarHeaderButton {
    /// Creates a header button with a user-defined ID and an SF Symbol name.
    pub fn new(id: impl Into<SharedString>, symbol: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            symbol: symbol.into(),
        }
    }
}

/// Creates a native macOS-style sidebar control backed by `NSSplitViewController`.
///
/// By default, the sidebar's left pane shows a source-list with the provided `items`.
/// Call `.embed_content_in_sidebar(true)` to replace the source list with the GPUI
/// content view, allowing you to render arbitrary native controls (segmented controls,
/// outline views, text fields, buttons, etc.) in the sidebar pane.
pub fn native_sidebar(id: impl Into<ElementId>, items: &[impl AsRef<str>]) -> NativeSidebar {
    NativeSidebar {
        id: id.into(),
        items: items
            .iter()
            .map(|item| SharedString::from(item.as_ref().to_string()))
            .collect(),
        selected_index: None,
        sidebar_width: 240.0,
        min_sidebar_width: 180.0,
        max_sidebar_width: 420.0,
        collapsed: false,
        embed_in_sidebar: false,
        sidebar_view: None,
        manage_window_chrome: true,
        manage_toolbar: true,
        on_select: None,
        header_title: None,
        header_buttons: Vec::new(),
        on_header_click: None,
        sidebar_background_color: None,
        style: StyleRefinement::default(),
    }
}

/// A native sidebar element with source-list navigation and a detail pane.
pub struct NativeSidebar {
    id: ElementId,
    items: Vec<SharedString>,
    selected_index: Option<usize>,
    sidebar_width: f64,
    min_sidebar_width: f64,
    max_sidebar_width: f64,
    collapsed: bool,
    embed_in_sidebar: bool,
    /// When set, a secondary GpuiSurface renders this view in the sidebar pane
    /// while the main GPUI content view stays in the detail pane.
    sidebar_view: Option<AnyView>,
    /// When `true` (default), GPUI configures native window chrome (toolbar and
    /// titlebar attributes) for AppKit-style sidebar behavior.
    ///
    /// Set this to `false` to preserve host-app window styling while still
    /// using the native sidebar split view container.
    manage_window_chrome: bool,
    /// When `true` (default), the sidebar creates its own NSToolbar with a toggle
    /// button and configures window titlebar attributes. Set to `false` if the
    /// host app already has its own toolbar and titlebar setup.
    manage_toolbar: bool,
    on_select: Option<Box<dyn Fn(&SidebarSelectEvent, &mut Window, &mut App) + 'static>>,
    header_title: Option<SharedString>,
    header_buttons: Vec<NativeSidebarHeaderButton>,
    on_header_click:
        Option<Box<dyn Fn(&SidebarHeaderClickEvent, &mut Window, &mut App) + 'static>>,
    sidebar_background_color: Option<Hsla>,
    style: StyleRefinement,
}

impl NativeSidebar {
    /// Sets the selected sidebar item.
    pub fn selected_index(mut self, selected_index: Option<usize>) -> Self {
        self.selected_index = selected_index;
        self
    }

    /// Sets sidebar width in pixels.
    pub fn sidebar_width(mut self, sidebar_width: f64) -> Self {
        self.sidebar_width = sidebar_width.max(120.0);
        self
    }

    /// Sets minimum sidebar width.
    pub fn min_sidebar_width(mut self, min_sidebar_width: f64) -> Self {
        self.min_sidebar_width = min_sidebar_width.max(120.0);
        if self.max_sidebar_width < self.min_sidebar_width {
            self.max_sidebar_width = self.min_sidebar_width;
        }
        self
    }

    /// Sets maximum sidebar width.
    pub fn max_sidebar_width(mut self, max_sidebar_width: f64) -> Self {
        self.max_sidebar_width = max_sidebar_width.max(self.min_sidebar_width.max(120.0));
        self
    }

    /// Collapses or expands the sidebar pane.
    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    /// When `true`, the GPUI content view is embedded inside the sidebar (left)
    /// pane instead of the detail (right) pane.  This replaces the source-list
    /// table and lets you render arbitrary GPUI elements — segmented controls,
    /// outline views, text fields, buttons — directly inside the sidebar.
    ///
    /// The items / on_select source-list will be skipped in this mode.
    pub fn embed_content_in_sidebar(mut self, embed: bool) -> Self {
        self.embed_in_sidebar = embed;
        self
    }

    /// Sets a view to render in the sidebar pane via a secondary GpuiSurface.
    /// The main GPUI content view stays in the detail (right) pane, while this
    /// view is rendered into its own Metal layer in the sidebar (left) pane.
    ///
    /// This enables dual-surface mode: both panes have independent GPUI-rendered
    /// content. Native controls painted within the sidebar view automatically
    /// parent themselves to the surface's NSView.
    pub fn sidebar_view<V: Render>(mut self, view: crate::Entity<V>) -> Self {
        self.sidebar_view = Some(AnyView::from(view));
        self
    }

    /// Controls whether native sidebar integration is allowed to modify window
    /// toolbar/titlebar chrome. Defaults to `true`.
    pub fn manage_window_chrome(mut self, manage_window_chrome: bool) -> Self {
        self.manage_window_chrome = manage_window_chrome;
        self
    }

    /// Controls whether the sidebar creates its own NSToolbar with a toggle
    /// button and configures titlebar attributes. Defaults to `true`.
    ///
    /// Set to `false` when the host app already manages its own toolbar and
    /// titlebar styling. The NSSplitViewController is still installed as the
    /// window's contentViewController (when `manage_window_chrome` is `true`),
    /// so the sidebar toggle action (`toggleSidebar:`) remains available.
    pub fn manage_toolbar(mut self, manage_toolbar: bool) -> Self {
        self.manage_toolbar = manage_toolbar;
        self
    }

    /// Registers a callback fired when a sidebar row is selected.
    pub fn on_select(
        mut self,
        listener: impl Fn(&SidebarSelectEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Box::new(listener));
        self
    }

    /// Sets a title label displayed in the sidebar header.
    pub fn header_title(mut self, title: impl Into<SharedString>) -> Self {
        self.header_title = Some(title.into());
        self
    }

    /// Adds a button to the sidebar header. Buttons appear right-aligned.
    /// The `button` specifies a user-defined ID and an SF Symbol name.
    pub fn header_button(mut self, button: NativeSidebarHeaderButton) -> Self {
        self.header_buttons.push(button);
        self
    }

    /// Registers a callback fired when a sidebar header button is clicked.
    pub fn on_header_click(
        mut self,
        listener: impl Fn(&SidebarHeaderClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_header_click = Some(Box::new(listener));
        self
    }

    /// Sets a solid background color for the sidebar's titlebar area — the region
    /// above the safe-area that is not covered by the GPUI surface view. Pass
    /// `None` to restore native glass or vibrancy (default).
    pub fn sidebar_background_color(mut self, color: Option<Hsla>) -> Self {
        self.sidebar_background_color = color;
        self
    }
}

/// Extra state tracked alongside the NativeControlState for sidebar-specific
/// bookkeeping (surface ID for dual-surface mode and previous config values
/// used to avoid redundant AppKit updates during paint).
struct SidebarExtraState {
    native: NativeControlState,
    #[cfg(target_os = "macos")]
    surface_id: Option<SurfaceId>,
    // Tracked config values for state diffing — only call into the platform
    // layer when these actually change. Unconditional updates trigger heavy
    // AppKit layout work during paint that causes reentrancy and hangs.
    prev_items: Vec<SharedString>,
    prev_selected: Option<usize>,
    prev_sidebar_width: f64,
    prev_min_width: f64,
    prev_max_width: f64,
    prev_collapsed: bool,
    prev_embed_in_host: bool,
    prev_header_title: Option<SharedString>,
    prev_header_button_symbols: Vec<SharedString>,
    prev_background_color: Option<(f64, f64, f64, f64)>,
}

impl Default for SidebarExtraState {
    fn default() -> Self {
        Self {
            native: NativeControlState::default(),
            #[cfg(target_os = "macos")]
            surface_id: None,
            prev_items: Vec::new(),
            prev_selected: None,
            prev_sidebar_width: 0.0,
            prev_min_width: 0.0,
            prev_max_width: 0.0,
            prev_collapsed: false,
            prev_embed_in_host: false,
            prev_header_title: None,
            prev_header_button_symbols: Vec::new(),
            prev_background_color: None,
        }
    }
}

unsafe impl Send for SidebarExtraState {}

impl IntoElement for NativeSidebar {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeSidebar {
    type RequestLayoutState = ();
    type PrepaintState = Bounds<Pixels>;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.refine(&self.style);

        if self.embed_in_sidebar || self.sidebar_view.is_some() {
            style.size.width =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(0.0))));
            style.size.height =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(0.0))));
        } else {
            if matches!(style.size.width, Length::Auto) {
                style.size.width =
                    Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(760.0))));
            }
            if matches!(style.size.height, Length::Auto) {
                style.size.height =
                    Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(420.0))));
            }
        }

        let layout_id = window.request_layout(style, [], cx);
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Bounds<Pixels> {
        bounds
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        _cx: &mut App,
    ) {
        let parent = window.raw_native_view_ptr();
        if parent.is_null() {
            return;
        }

        let on_select = self.on_select.take();
        let sidebar_view = self.sidebar_view.take();
        let has_sidebar_view = sidebar_view.is_some();
        let items = self.items.clone();
        let selected_index = self.selected_index;
        let sidebar_width = self.sidebar_width.max(120.0);
        let min_sidebar_width = self.min_sidebar_width.max(120.0);
        let max_sidebar_width = self.max_sidebar_width.max(min_sidebar_width);
        let collapsed = self.collapsed;
        let embed_in_sidebar = self.embed_in_sidebar;
        let manage_window_chrome = self.manage_window_chrome;
        let manage_toolbar = self.manage_toolbar;
        let header_title = self.header_title.take();
        let header_buttons = std::mem::take(&mut self.header_buttons);
        let on_header_click = self.on_header_click.take();
        let header_button_symbols: Vec<SharedString> =
            header_buttons.iter().map(|b| b.symbol.clone()).collect();
        let sidebar_background_color = self.sidebar_background_color.map(|color| {
            let rgba = color.to_rgb();
            (rgba.r as f64, rgba.g as f64, rgba.b as f64, rgba.a as f64)
        });

        let next_frame_callbacks = window.next_frame_callbacks.clone();
        let invalidator = window.invalidator.clone();

        window.with_optional_element_state::<SidebarExtraState, _>(id, |prev_state, window| {
            let mut state = prev_state.flatten().unwrap_or_default();

            let on_select_fn = on_select.map(|handler| {
                let handler = Rc::new(handler);
                schedule_native_callback(
                    handler,
                    |(index, title): (usize, String)| SidebarSelectEvent {
                        index,
                        title: SharedString::from(title),
                    },
                    next_frame_callbacks.clone(),
                    invalidator.clone(),
                )
            });

            let on_header_button_fn = on_header_click.map(|handler| {
                let handler = Rc::new(handler);
                let button_ids: Vec<SharedString> =
                    header_buttons.iter().map(|b| b.id.clone()).collect();
                schedule_native_callback(
                    handler,
                    move |index: usize| SidebarHeaderClickEvent {
                        index,
                        id: button_ids.get(index).cloned().unwrap_or_default(),
                    },
                    next_frame_callbacks.clone(),
                    invalidator.clone(),
                )
            });

            let effective_embed = embed_in_sidebar || has_sidebar_view;

            // Only call update_sidebar when the sidebar hasn't been created
            // yet, or when config values actually changed. Unconditional
            // updates trigger heavy AppKit layout/table-reload work during
            // paint that causes RefCell reentrancy and render-loop hangs.
            let needs_update = !state.native.is_initialized()
                || state.prev_items != items
                || state.prev_selected != selected_index
                || state.prev_sidebar_width != sidebar_width
                || state.prev_min_width != min_sidebar_width
                || state.prev_max_width != max_sidebar_width
                || state.prev_collapsed != collapsed
                || state.prev_embed_in_host != effective_embed
                || state.prev_header_title != header_title
                || state.prev_header_button_symbols != header_button_symbols
                || state.prev_background_color != sidebar_background_color
                || on_select_fn.is_some()
                || on_header_button_fn.is_some();

            if needs_update {
                let item_strs: Vec<&str> = items.iter().map(|item| item.as_ref()).collect();
                let symbol_strs: Vec<&str> =
                    header_button_symbols.iter().map(|s| s.as_ref()).collect();

                let scale = window.scale_factor();
                let nc = window.native_controls();
                nc.update_sidebar(
                    &mut state.native,
                    parent,
                    Bounds::default(),
                    scale,
                    SidebarViewConfig {
                        sidebar_width,
                        min_width: min_sidebar_width,
                        max_width: max_sidebar_width,
                        collapsed,
                        expanded_width: sidebar_width,
                        embed_in_host: effective_embed,
                        items: &item_strs,
                        selected_index,
                        header_title: header_title.as_deref().map(|v| &**v),
                        header_button_symbols: &symbol_strs,
                        background_color: sidebar_background_color,
                        on_select: on_select_fn,
                        on_header_button: on_header_button_fn,
                    },
                );

                state.prev_items = items;
                state.prev_selected = selected_index;
                state.prev_sidebar_width = sidebar_width;
                state.prev_min_width = min_sidebar_width;
                state.prev_max_width = max_sidebar_width;
                state.prev_collapsed = collapsed;
                state.prev_embed_in_host = effective_embed;
                state.prev_header_title = header_title;
                state.prev_header_button_symbols = header_button_symbols;
                state.prev_background_color = sidebar_background_color;
            }

            // Deferred to the next run loop iteration to avoid AppKit
            // callbacks (setFrameSize:) re-entering GPUI during paint.
            window.configure_hosted_content(
                state.native.view(),
                parent,
                HostedContentConfig {
                    embed_in_host: embed_in_sidebar && !has_sidebar_view,
                    manage_window_chrome,
                    manage_toolbar,
                },
            );

            // Register or update the sidebar surface for dual-surface mode
            #[cfg(target_os = "macos")]
            if let Some(view) = sidebar_view {
                if let Some(surface_id) = state.surface_id {
                    window.update_surface_root_view(surface_id, view);
                } else {
                    let handle = window.register_surface(view);
                    window.attach_hosted_surface(state.native.view(), handle.native_view_ptr);
                    state.surface_id = Some(handle.id);
                }
            }

            #[cfg(not(target_os = "macos"))]
            {
                let _ = sidebar_view;
            }

            ((), Some(state))
        });

        if embed_in_sidebar || has_sidebar_view {
            let new_size = window.platform_window.content_size();
            if window.viewport_size != new_size {
                window.viewport_size = new_size;
                window.invalidator.set_dirty(true);
            }
        }
    }
}

impl Styled for NativeSidebar {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
