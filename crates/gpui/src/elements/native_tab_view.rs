use refineable::Refineable as _;
use std::ffi::c_void;
use std::rc::Rc;

use crate::{
    AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, SharedString, Style,
    StyleRefinement, Styled, Window, px,
};

use super::native_element_helpers::schedule_native_callback;

/// Event emitted when a tab is selected in `NativeTabView`.
#[derive(Clone, Debug)]
pub struct TabSelectEvent {
    /// Zero-based selected tab index.
    pub index: usize,
}

/// Creates a native tab view (NSTabView) for simple content tabs.
pub fn native_tab_view(id: impl Into<ElementId>, labels: &[impl AsRef<str>]) -> NativeTabView {
    NativeTabView {
        id: id.into(),
        labels: labels
            .iter()
            .map(|label| SharedString::from(label.as_ref().to_string()))
            .collect(),
        selected_index: 0,
        on_select: None,
        style: StyleRefinement::default(),
    }
}

/// A native NSTabView wrapper for simple tab navigation.
pub struct NativeTabView {
    id: ElementId,
    labels: Vec<SharedString>,
    selected_index: usize,
    on_select: Option<Box<dyn Fn(&TabSelectEvent, &mut Window, &mut App) + 'static>>,
    style: StyleRefinement,
}

impl NativeTabView {
    /// Sets the selected tab index.
    pub fn selected_index(mut self, selected_index: usize) -> Self {
        self.selected_index = selected_index;
        self
    }

    /// Registers a callback fired when tab selection changes.
    pub fn on_select(
        mut self,
        listener: impl Fn(&TabSelectEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Box::new(listener));
        self
    }
}

struct NativeTabViewState {
    control_ptr: *mut c_void,
    target_ptr: *mut c_void,
    current_labels: Vec<SharedString>,
    current_selected: usize,
    attached: bool,
}

impl Drop for NativeTabViewState {
    fn drop(&mut self) {
        if self.attached {
            #[cfg(target_os = "macos")]
            unsafe {
                use crate::platform::native_controls;
                super::native_element_helpers::cleanup_native_control(
                    self.control_ptr,
                    self.target_ptr,
                    native_controls::release_native_tab_view_target,
                    native_controls::release_native_tab_view,
                );
            }
        }
    }
}

unsafe impl Send for NativeTabViewState {}

impl IntoElement for NativeTabView {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeTabView {
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
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(420.0))));
        }
        if matches!(style.size.height, Length::Auto) {
            style.size.height =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(280.0))));
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
        bounds: Bounds<Pixels>,
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
            let labels = self.labels.clone();
            let selected_index = self.selected_index;

            let next_frame_callbacks = window.next_frame_callbacks.clone();
            let invalidator = window.invalidator.clone();

            window.with_optional_element_state::<NativeTabViewState, _>(
                id,
                |prev_state, window| {
                    let state = if let Some(Some(mut state)) = prev_state {
                        unsafe {
                            native_controls::set_native_view_frame(
                                state.control_ptr as cocoa::base::id,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );
                        }

                        if state.current_labels != labels {
                            let label_strs: Vec<&str> =
                                labels.iter().map(|label| label.as_ref()).collect();
                            unsafe {
                                native_controls::set_native_tab_view_items(
                                    state.control_ptr as cocoa::base::id,
                                    &label_strs,
                                    selected_index,
                                );
                            }
                            state.current_labels = labels.clone();
                            state.current_selected = selected_index;
                        } else if state.current_selected != selected_index {
                            unsafe {
                                native_controls::set_native_tab_view_selected(
                                    state.control_ptr as cocoa::base::id,
                                    selected_index,
                                );
                            }
                            state.current_selected = selected_index;
                        }

                        if on_select.is_some() {
                            unsafe {
                                native_controls::release_native_tab_view_target(state.target_ptr);
                            }

                            let callback = on_select.take().map(|handler| {
                                let nfc = next_frame_callbacks.clone();
                                let inv = invalidator.clone();
                                let handler = Rc::new(handler);
                                schedule_native_callback(
                                    handler,
                                    |index| TabSelectEvent { index },
                                    nfc,
                                    inv,
                                )
                            });

                            unsafe {
                                state.target_ptr = native_controls::set_native_tab_view_action(
                                    state.control_ptr as cocoa::base::id,
                                    callback,
                                );
                            }
                        }

                        state
                    } else {
                        let callback = on_select.take().map(|handler| {
                            let nfc = next_frame_callbacks.clone();
                            let inv = invalidator.clone();
                            let handler = Rc::new(handler);
                            schedule_native_callback(
                                handler,
                                |index| TabSelectEvent { index },
                                nfc,
                                inv,
                            )
                        });

                        let (control_ptr, target_ptr) = unsafe {
                            let control = native_controls::create_native_tab_view();

                            let label_strs: Vec<&str> =
                                labels.iter().map(|label| label.as_ref()).collect();
                            native_controls::set_native_tab_view_items(
                                control,
                                &label_strs,
                                selected_index,
                            );

                            let target =
                                native_controls::set_native_tab_view_action(control, callback);

                            native_controls::attach_native_view_to_parent(
                                control,
                                native_view as cocoa::base::id,
                            );
                            native_controls::set_native_view_frame(
                                control,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );

                            (control as *mut c_void, target)
                        };

                        NativeTabViewState {
                            control_ptr,
                            target_ptr,
                            current_labels: labels,
                            current_selected: selected_index,
                            attached: true,
                        }
                    };

                    ((), Some(state))
                },
            );
        }
    }
}

impl Styled for NativeTabView {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
