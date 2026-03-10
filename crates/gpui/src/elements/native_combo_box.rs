use refineable::Refineable as _;
use std::rc::Rc;

use crate::platform::native_controls::{ComboBoxCallbacks, ComboBoxConfig, NativeControlState};
use crate::{
    px, AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, SharedString, Style,
    StyleRefinement, Styled, Window,
};

use super::native_element_helpers::schedule_native_callback;

#[derive(Clone, Debug)]
pub struct ComboBoxChangeEvent {
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct ComboBoxSelectEvent {
    pub index: usize,
}

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
    pub fn items(mut self, items: &[impl AsRef<str>]) -> Self {
        self.items = items
            .iter()
            .map(|item| SharedString::from(item.as_ref().to_string()))
            .collect();
        self
    }

    pub fn selected_index(mut self, index: usize) -> Self {
        self.selected_index = index;
        self
    }

    pub fn text(mut self, text: impl Into<SharedString>) -> Self {
        self.text = text.into();
        self
    }

    pub fn editable(mut self, editable: bool) -> Self {
        self.editable = editable;
        self
    }

    pub fn completes(mut self, completes: bool) -> Self {
        self.completes = completes;
        self
    }

    pub fn on_change(
        mut self,
        listener: impl Fn(&ComboBoxChangeEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Box::new(listener));
        self
    }

    pub fn on_select(
        mut self,
        listener: impl Fn(&ComboBoxSelectEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Box::new(listener));
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

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
        let parent = window.raw_native_view_ptr();
        if parent.is_null() {
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

        let nfc = window.next_frame_callbacks.clone();
        let inv = window.invalidator.clone();

        window.with_optional_element_state::<NativeControlState, _>(id, |prev_state, window| {
            let mut state = prev_state.flatten().unwrap_or_default();

            let callbacks = build_combo_box_callbacks(on_change, on_select, nfc, inv);

            let item_strs: Vec<&str> = items.iter().map(|s| s.as_ref()).collect();
            let value = if editable {
                Some(text.as_ref())
            } else {
                None
            };

            let scale = window.scale_factor();
            let nc = window.native_controls();
            nc.update_combo_box(
                &mut state,
                parent,
                bounds,
                scale,
                ComboBoxConfig {
                    items: &item_strs,
                    selected_index,
                    editable,
                    completes,
                    value,
                    enabled: !disabled,
                    callbacks,
                },
            );

            ((), Some(state))
        });
    }
}

fn build_combo_box_callbacks(
    on_change: Option<Box<dyn Fn(&ComboBoxChangeEvent, &mut Window, &mut App) + 'static>>,
    on_select: Option<Box<dyn Fn(&ComboBoxSelectEvent, &mut Window, &mut App) + 'static>>,
    next_frame_callbacks: Rc<std::cell::RefCell<Vec<super::native_element_helpers::FrameCallback>>>,
    invalidator: crate::WindowInvalidator,
) -> ComboBoxCallbacks {
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

    ComboBoxCallbacks {
        on_select: select_cb,
        on_change: change_cb,
        on_submit: None,
    }
}

impl Styled for NativeComboBox {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
