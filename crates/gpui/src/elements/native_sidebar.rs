use refineable::Refineable as _;
use std::ffi::c_void;
use std::rc::Rc;

use crate::{
    AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, SharedString, Style,
    StyleRefinement, Styled, Window, px,
};

use super::native_element_helpers::schedule_native_callback;

/// Event emitted when a native sidebar item is selected.
#[derive(Clone, Debug)]
pub struct SidebarSelectEvent {
    /// Zero-based selected item index.
    pub index: usize,
    /// Selected item title.
    pub title: SharedString,
}

/// Creates a native macOS-style sidebar control backed by `NSSplitViewController`.
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
        on_select: None,
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
    on_select: Option<Box<dyn Fn(&SidebarSelectEvent, &mut Window, &mut App) + 'static>>,
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

    /// Registers a callback fired when a sidebar row is selected.
    pub fn on_select(
        mut self,
        listener: impl Fn(&SidebarSelectEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Box::new(listener));
        self
    }
}

struct NativeSidebarState {
    control_ptr: *mut c_void,
    target_ptr: *mut c_void,
    current_items: Vec<SharedString>,
    current_selected: Option<usize>,
    current_sidebar_width: f64,
    current_min_sidebar_width: f64,
    current_max_sidebar_width: f64,
    current_collapsed: bool,
    attached: bool,
}

impl Drop for NativeSidebarState {
    fn drop(&mut self) {
        if self.attached {
            #[cfg(target_os = "macos")]
            unsafe {
                use crate::platform::native_controls;
                super::native_element_helpers::cleanup_native_control(
                    self.control_ptr,
                    self.target_ptr,
                    native_controls::release_native_sidebar_target,
                    native_controls::release_native_sidebar_view,
                );
            }
        }
    }
}

unsafe impl Send for NativeSidebarState {}

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

        if matches!(style.size.width, Length::Auto) {
            style.size.width =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(760.0))));
        }
        if matches!(style.size.height, Length::Auto) {
            style.size.height =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(420.0))));
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
        #[cfg(target_os = "macos")]
        {
            use crate::platform::native_controls;

            let native_view = window.raw_native_view_ptr();
            if native_view.is_null() {
                return;
            }

            let mut on_select = self.on_select.take();
            let items = self.items.clone();
            let selected_index = self.selected_index;
            let sidebar_width = self.sidebar_width.max(120.0);
            let min_sidebar_width = self.min_sidebar_width.max(120.0);
            let max_sidebar_width = self.max_sidebar_width.max(min_sidebar_width);
            let collapsed = self.collapsed;

            let next_frame_callbacks = window.next_frame_callbacks.clone();
            let invalidator = window.invalidator.clone();

            window.with_optional_element_state::<NativeSidebarState, _>(
                id,
                |prev_state, _window| {
                    let state = if let Some(Some(mut state)) = prev_state {
                        unsafe {
                            native_controls::configure_native_sidebar_window(
                                state.control_ptr as cocoa::base::id,
                                native_view as cocoa::base::id,
                            );
                        }

                        let min_max_changed = state.current_min_sidebar_width != min_sidebar_width
                            || state.current_max_sidebar_width != max_sidebar_width;
                        let width_changed = state.current_sidebar_width != sidebar_width;

                        if width_changed || min_max_changed {
                            if !collapsed {
                                unsafe {
                                    native_controls::set_native_sidebar_width(
                                        state.control_ptr as cocoa::base::id,
                                        sidebar_width,
                                        min_sidebar_width,
                                        max_sidebar_width,
                                    );
                                }
                            }
                            state.current_sidebar_width = sidebar_width;
                            state.current_min_sidebar_width = min_sidebar_width;
                            state.current_max_sidebar_width = max_sidebar_width;
                        }

                        if state.current_collapsed != collapsed {
                            unsafe {
                                native_controls::set_native_sidebar_collapsed(
                                    state.control_ptr as cocoa::base::id,
                                    collapsed,
                                    sidebar_width,
                                    min_sidebar_width,
                                    max_sidebar_width,
                                );
                            }
                            state.current_collapsed = collapsed;
                        }

                        let needs_rebind = state.current_items != items
                            || state.current_selected != selected_index
                            || on_select.is_some()
                            || min_max_changed;

                        if needs_rebind {
                            unsafe {
                                native_controls::release_native_sidebar_target(state.target_ptr);
                            }

                            let callback = on_select.take().map(|handler| {
                                let nfc = next_frame_callbacks.clone();
                                let inv = invalidator.clone();
                                let handler = Rc::new(handler);
                                schedule_native_callback(
                                    handler,
                                    |(index, title): (usize, String)| SidebarSelectEvent {
                                        index,
                                        title: SharedString::from(title),
                                    },
                                    nfc,
                                    inv,
                                )
                            });

                            let item_strs: Vec<&str> =
                                items.iter().map(|item| item.as_ref()).collect();
                            unsafe {
                                state.target_ptr = native_controls::set_native_sidebar_items(
                                    state.control_ptr as cocoa::base::id,
                                    &item_strs,
                                    selected_index,
                                    min_sidebar_width,
                                    max_sidebar_width,
                                    callback,
                                );
                            }
                            state.current_items = items.clone();
                            state.current_selected = selected_index;
                        }

                        state
                    } else {
                        let callback = on_select.take().map(|handler| {
                            let nfc = next_frame_callbacks.clone();
                            let inv = invalidator.clone();
                            let handler = Rc::new(handler);
                            schedule_native_callback(
                                handler,
                                |(index, title): (usize, String)| SidebarSelectEvent {
                                    index,
                                    title: SharedString::from(title),
                                },
                                nfc,
                                inv,
                            )
                        });

                        let (control_ptr, target_ptr) = unsafe {
                            let control = native_controls::create_native_sidebar_view(
                                sidebar_width,
                                min_sidebar_width,
                                max_sidebar_width,
                            );

                            let item_strs: Vec<&str> =
                                items.iter().map(|item| item.as_ref()).collect();
                            let target = native_controls::set_native_sidebar_items(
                                control,
                                &item_strs,
                                selected_index,
                                min_sidebar_width,
                                max_sidebar_width,
                                callback,
                            );

                            if collapsed {
                                native_controls::set_native_sidebar_collapsed(
                                    control,
                                    true,
                                    sidebar_width,
                                    min_sidebar_width,
                                    max_sidebar_width,
                                );
                            }

                            native_controls::configure_native_sidebar_window(
                                control,
                                native_view as cocoa::base::id,
                            );

                            (control as *mut c_void, target)
                        };

                        NativeSidebarState {
                            control_ptr,
                            target_ptr,
                            current_items: items,
                            current_selected: selected_index,
                            current_sidebar_width: sidebar_width,
                            current_min_sidebar_width: min_sidebar_width,
                            current_max_sidebar_width: max_sidebar_width,
                            current_collapsed: collapsed,
                            attached: true,
                        }
                    };

                    ((), Some(state))
                },
            );
        }
    }
}

impl Styled for NativeSidebar {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
