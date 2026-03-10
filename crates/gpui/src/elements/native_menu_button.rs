use refineable::Refineable as _;
use std::rc::Rc;

use crate::platform::native_controls::{MenuButtonConfig, NativeControlState, NativeMenuItemData};
use crate::{
    px, AbsoluteLength, AnyWindowHandle, App, AsyncApp, Bounds, DefiniteLength, Element,
    ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId, Length, Pixels, Point,
    SharedString, Style, StyleRefinement, Styled, Window,
};

use super::native_element_helpers::schedule_native_callback;

pub fn show_native_popup_menu(
    items: &[NativeMenuItem],
    position: Point<Pixels>,
    window: &Window,
    cx: &App,
    on_select: impl FnOnce(usize, &mut Window, &mut App) + 'static,
) {
    let native_view = window.raw_native_view_ptr();
    if native_view.is_null() {
        return;
    }

    let nc = window.native_controls();
    let mapped = map_items(items);
    let async_app = cx.to_async();
    let window_handle = window.window_handle();

    nc.show_context_menu(
        &mapped,
        native_view,
        position.x.0 as f64,
        position.y.0 as f64,
        Box::new(move |result| {
            if let Some(index) = result {
                deferred_update(async_app, window_handle, move |window, cx| {
                    on_select(index, window, cx);
                });
            }
        }),
    );
}

fn deferred_update(
    async_app: AsyncApp,
    window_handle: AnyWindowHandle,
    f: impl FnOnce(&mut Window, &mut App) + 'static,
) {
    async_app.update(|cx| {
        window_handle
            .update(cx, |_, window, cx| f(window, cx))
            .ok();
    });
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeMenuItem {
    Action {
        title: SharedString,
        enabled: bool,
    },
    Submenu {
        title: SharedString,
        enabled: bool,
        items: Vec<NativeMenuItem>,
    },
    Separator,
}

impl NativeMenuItem {
    pub fn action(title: impl Into<SharedString>) -> Self {
        Self::Action {
            title: title.into(),
            enabled: true,
        }
    }

    pub fn submenu(title: impl Into<SharedString>, items: Vec<NativeMenuItem>) -> Self {
        Self::Submenu {
            title: title.into(),
            enabled: true,
            items,
        }
    }

    pub fn separator() -> Self {
        Self::Separator
    }

    pub fn enabled(self, enabled: bool) -> Self {
        match self {
            Self::Action { title, .. } => Self::Action { title, enabled },
            Self::Submenu { title, items, .. } => Self::Submenu {
                title,
                enabled,
                items,
            },
            Self::Separator => Self::Separator,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MenuItemSelectEvent {
    pub index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeMenuKind {
    Button,
    Context,
}

pub fn native_menu_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    items: &[NativeMenuItem],
) -> NativeMenuButton {
    NativeMenuButton {
        id: id.into(),
        label: label.into(),
        items: items.to_vec(),
        on_select: None,
        disabled: false,
        kind: NativeMenuKind::Button,
        style: StyleRefinement::default(),
    }
}

pub fn native_context_menu(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    items: &[NativeMenuItem],
) -> NativeMenuButton {
    NativeMenuButton {
        id: id.into(),
        label: label.into(),
        items: items.to_vec(),
        on_select: None,
        disabled: false,
        kind: NativeMenuKind::Context,
        style: StyleRefinement::default(),
    }
}

pub struct NativeMenuButton {
    id: ElementId,
    label: SharedString,
    items: Vec<NativeMenuItem>,
    on_select: Option<Box<dyn Fn(&MenuItemSelectEvent, &mut Window, &mut App) + 'static>>,
    disabled: bool,
    kind: NativeMenuKind,
    style: StyleRefinement,
}

impl NativeMenuButton {
    pub fn on_select(
        mut self,
        listener: impl Fn(&MenuItemSelectEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Box::new(listener));
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

fn map_items(items: &[NativeMenuItem]) -> Vec<NativeMenuItemData> {
    fn convert(item: &NativeMenuItem) -> NativeMenuItemData {
        match item {
            NativeMenuItem::Action { title, enabled } => NativeMenuItemData::Action {
                title: title.to_string(),
                enabled: *enabled,
                icon: None,
            },
            NativeMenuItem::Submenu {
                title,
                enabled,
                items,
            } => NativeMenuItemData::Submenu {
                title: title.to_string(),
                enabled: *enabled,
                icon: None,
                items: items.iter().map(convert).collect(),
            },
            NativeMenuItem::Separator => NativeMenuItemData::Separator,
        }
    }

    items.iter().map(convert).collect()
}

impl IntoElement for NativeMenuButton {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeMenuButton {
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
            let width = (self.label.len() as f32 * 8.0 + 40.0).max(140.0);
            style.size.width =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(width))));
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

        let on_select = self.on_select.take();
        let label = self.label.clone();
        let items = self.items.clone();
        let disabled = self.disabled;
        let kind = self.kind;

        let nfc = window.next_frame_callbacks.clone();
        let inv = window.invalidator.clone();

        window.with_optional_element_state::<NativeControlState, _>(id, |prev_state, window| {
            let mut state = prev_state.flatten().unwrap_or_default();

            let on_select_fn = on_select.map(|handler| {
                let handler = Rc::new(handler);
                schedule_native_callback(
                    handler,
                    |index| MenuItemSelectEvent { index },
                    nfc.clone(),
                    inv.clone(),
                )
            });

            let mapped = map_items(&items);

            let scale = window.scale_factor();
            let nc = window.native_controls();
            nc.update_menu_button(
                &mut state,
                parent,
                bounds,
                scale,
                MenuButtonConfig {
                    title: &label,
                    context_menu: kind == NativeMenuKind::Context,
                    items: &mapped,
                    enabled: !disabled,
                    on_select: on_select_fn,
                },
            );

            ((), Some(state))
        });
    }
}

impl Styled for NativeMenuButton {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
