use refineable::Refineable as _;
use std::ffi::c_void;
use std::rc::Rc;

use crate::{
    AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, SharedString, Style,
    StyleRefinement, Styled, Window, px,
};

use super::native_element_helpers::schedule_native_callback;

/// Event emitted when combo-box text changes.
#[derive(Clone, Debug)]
pub struct ComboBoxChangeEvent {
    /// The current editable text.
    pub text: String,
}

/// Event emitted when a combo-box item is selected.
#[derive(Clone, Debug)]
pub struct ComboBoxSelectEvent {
    /// The selected item index.
    pub index: usize,
}

/// Creates a native combo-box (NSComboBox on macOS).
pub fn native_combo_box(id: impl Into<ElementId>, items: &[impl AsRef<str>]) -> NativeComboBox {
    NativeComboBox {
        id: id.into(),
        items: items
            .iter()
            .map(|item| SharedString::from(item.as_ref().to_string()))
            .collect(),
        selected_index: 0,
        text: SharedString::default(),
        editable: false,
        completes: true,
        on_change: None,
        on_select: None,
        disabled: false,
        style: StyleRefinement::default(),
    }
}

/// A native combo-box element positioned by GPUI's Taffy layout.
pub struct NativeComboBox {
    id: ElementId,
    items: Vec<SharedString>,
    selected_index: usize,
    text: SharedString,
    editable: bool,
    completes: bool,
    on_change: Option<Box<dyn Fn(&ComboBoxChangeEvent, &mut Window, &mut App) + 'static>>,
    on_select: Option<Box<dyn Fn(&ComboBoxSelectEvent, &mut Window, &mut App) + 'static>>,
    disabled: bool,
    style: StyleRefinement,
}

impl NativeComboBox {
    /// Sets item list.
    pub fn items(mut self, items: &[impl AsRef<str>]) -> Self {
        self.items = items
            .iter()
            .map(|item| SharedString::from(item.as_ref().to_string()))
            .collect();
        self
    }

    /// Sets selected item index.
    pub fn selected_index(mut self, index: usize) -> Self {
        self.selected_index = index;
        self
    }

    /// Sets current text value (used by editable combo-boxes).
    pub fn text(mut self, text: impl Into<SharedString>) -> Self {
        self.text = text.into();
        self
    }

    /// Sets whether combo-box is editable.
    pub fn editable(mut self, editable: bool) -> Self {
        self.editable = editable;
        self
    }

    /// Sets whether editable combo-box autocompletes.
    pub fn completes(mut self, completes: bool) -> Self {
        self.completes = completes;
        self
    }

    /// Registers callback for text changes.
    pub fn on_change(
        mut self,
        listener: impl Fn(&ComboBoxChangeEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Box::new(listener));
        self
    }

    /// Registers callback for item selection changes.
    pub fn on_select(
        mut self,
        listener: impl Fn(&ComboBoxSelectEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Box::new(listener));
        self
    }

    /// Sets whether combo-box is disabled.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

struct NativeComboBoxElementState {
    combo_box_ptr: *mut c_void,
    delegate_ptr: *mut c_void,
    current_items: Vec<SharedString>,
    current_selected: usize,
    current_text: SharedString,
    current_editable: bool,
    current_completes: bool,
    attached: bool,
}

impl Drop for NativeComboBoxElementState {
    fn drop(&mut self) {
        if self.attached {
            #[cfg(target_os = "macos")]
            unsafe {
                use crate::platform::native_controls;
                super::native_element_helpers::cleanup_native_control(
                    self.combo_box_ptr,
                    self.delegate_ptr,
                    native_controls::release_native_combo_box_delegate,
                    native_controls::release_native_combo_box,
                );
            }
        }
    }
}

unsafe impl Send for NativeComboBoxElementState {}

fn clamp_selected_index(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { index.min(len - 1) }
}

impl IntoElement for NativeComboBox {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeComboBox {
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
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(220.0))));
        }
        if matches!(style.size.height, Length::Auto) {
            style.size.height =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(26.0))));
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

            let on_change = self.on_change.take();
            let on_select = self.on_select.take();
            let items = self.items.clone();
            let selected_index = clamp_selected_index(self.selected_index, items.len());
            let text = self.text.clone();
            let editable = self.editable;
            let completes = self.completes;
            let disabled = self.disabled;

            let next_frame_callbacks = window.next_frame_callbacks.clone();
            let invalidator = window.invalidator.clone();

            window.with_optional_element_state::<NativeComboBoxElementState, _>(
                id,
                |prev_state, window| {
                    let state = if let Some(Some(mut state)) = prev_state {
                        unsafe {
                            native_controls::set_native_view_frame(
                                state.combo_box_ptr as cocoa::base::id,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );

                            if state.current_items != items {
                                let item_strs: Vec<&str> =
                                    items.iter().map(|item| item.as_ref()).collect();
                                native_controls::set_native_combo_box_items(
                                    state.combo_box_ptr as cocoa::base::id,
                                    &item_strs,
                                );
                                state.current_items = items.clone();
                            }

                            if state.current_selected != selected_index && !items.is_empty() {
                                native_controls::set_native_combo_box_selected(
                                    state.combo_box_ptr as cocoa::base::id,
                                    selected_index,
                                );
                                state.current_selected = selected_index;
                            }

                            if state.current_editable != editable {
                                native_controls::set_native_combo_box_editable(
                                    state.combo_box_ptr as cocoa::base::id,
                                    editable,
                                );
                                state.current_editable = editable;
                            }

                            if state.current_completes != completes {
                                native_controls::set_native_combo_box_completes(
                                    state.combo_box_ptr as cocoa::base::id,
                                    completes,
                                );
                                state.current_completes = completes;
                            }

                            if editable && state.current_text != text {
                                native_controls::set_native_combo_box_string_value(
                                    state.combo_box_ptr as cocoa::base::id,
                                    &text,
                                );
                                state.current_text = text.clone();
                            }

                            native_controls::set_native_control_enabled(
                                state.combo_box_ptr as cocoa::base::id,
                                !disabled,
                            );
                        }

                        unsafe {
                            native_controls::release_native_combo_box_delegate(state.delegate_ptr);
                        }
                        let callbacks = build_combo_box_callbacks(
                            on_change,
                            on_select,
                            next_frame_callbacks,
                            invalidator,
                        );
                        unsafe {
                            state.delegate_ptr = native_controls::set_native_combo_box_delegate(
                                state.combo_box_ptr as cocoa::base::id,
                                callbacks,
                            );
                        }

                        state
                    } else {
                        let (combo_box_ptr, delegate_ptr, initial_text) = unsafe {
                            let item_strs: Vec<&str> =
                                items.iter().map(|item| item.as_ref()).collect();
                            let combo = native_controls::create_native_combo_box(
                                &item_strs,
                                selected_index,
                                editable,
                            );
                            native_controls::set_native_combo_box_completes(combo, completes);

                            if editable && !text.is_empty() {
                                native_controls::set_native_combo_box_string_value(combo, &text);
                            }

                            native_controls::set_native_control_enabled(combo, !disabled);
                            native_controls::attach_native_view_to_parent(
                                combo,
                                native_view as cocoa::base::id,
                            );
                            native_controls::set_native_view_frame(
                                combo,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );

                            let callbacks = build_combo_box_callbacks(
                                on_change,
                                on_select,
                                next_frame_callbacks,
                                invalidator,
                            );
                            let delegate =
                                native_controls::set_native_combo_box_delegate(combo, callbacks);

                            let initial_text = if editable {
                                if text.is_empty() {
                                    SharedString::from(
                                        native_controls::get_native_combo_box_string_value(combo),
                                    )
                                } else {
                                    text.clone()
                                }
                            } else {
                                SharedString::default()
                            };

                            (combo as *mut c_void, delegate, initial_text)
                        };

                        NativeComboBoxElementState {
                            combo_box_ptr,
                            delegate_ptr,
                            current_items: items,
                            current_selected: selected_index,
                            current_text: initial_text,
                            current_editable: editable,
                            current_completes: completes,
                            attached: true,
                        }
                    };

                    ((), Some(state))
                },
            );
        }
    }
}

#[cfg(target_os = "macos")]
fn build_combo_box_callbacks(
    on_change: Option<Box<dyn Fn(&ComboBoxChangeEvent, &mut Window, &mut App) + 'static>>,
    on_select: Option<Box<dyn Fn(&ComboBoxSelectEvent, &mut Window, &mut App) + 'static>>,
    next_frame_callbacks: Rc<std::cell::RefCell<Vec<super::native_element_helpers::FrameCallback>>>,
    invalidator: crate::WindowInvalidator,
) -> crate::platform::native_controls::ComboBoxCallbacks {
    let change_cb = on_change.map(|handler| {
        schedule_native_callback(
            Rc::new(handler),
            |text| ComboBoxChangeEvent { text },
            next_frame_callbacks.clone(),
            invalidator.clone(),
        )
    });

    let select_cb = on_select.map(|handler| {
        schedule_native_callback(
            Rc::new(handler),
            |index| ComboBoxSelectEvent { index },
            next_frame_callbacks,
            invalidator,
        )
    });

    crate::platform::native_controls::ComboBoxCallbacks {
        on_select: select_cb,
        on_change: change_cb,
    }
}

impl Styled for NativeComboBox {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
