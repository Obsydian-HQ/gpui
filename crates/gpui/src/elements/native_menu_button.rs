use refineable::Refineable as _;
use std::ffi::c_void;
use std::rc::Rc;

use crate::{
    AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, SharedString, Style,
    StyleRefinement, Styled, Window, px,
};

use super::native_element_helpers::schedule_native_callback;

/// A declarative native menu item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeMenuItem {
    /// A clickable action item.
    Action {
        /// Visible title.
        title: SharedString,
        /// Whether this item is enabled.
        enabled: bool,
    },
    /// A submenu containing more menu items.
    Submenu {
        /// Visible title.
        title: SharedString,
        /// Whether this submenu is enabled.
        enabled: bool,
        /// Nested menu items.
        items: Vec<NativeMenuItem>,
    },
    /// A visual separator.
    Separator,
}

impl NativeMenuItem {
    /// Creates an enabled action item.
    pub fn action(title: impl Into<SharedString>) -> Self {
        Self::Action {
            title: title.into(),
            enabled: true,
        }
    }

    /// Creates an enabled submenu.
    pub fn submenu(title: impl Into<SharedString>, items: Vec<NativeMenuItem>) -> Self {
        Self::Submenu {
            title: title.into(),
            enabled: true,
            items,
        }
    }

    /// Creates a separator item.
    pub fn separator() -> Self {
        Self::Separator
    }

    /// Sets enabled state on action and submenu items.
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

/// Event emitted when a menu action item is selected.
#[derive(Clone, Debug)]
pub struct MenuItemSelectEvent {
    /// Zero-based action index across all action items (depth-first order).
    pub index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeMenuKind {
    Button,
    Context,
}

/// Creates a native menu button (NSButton + NSMenu/NSMenuItem).
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

/// Creates a native context-menu trigger button.
///
/// The menu opens on left click and right click.
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

/// A native menu button/context menu element.
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
    /// Registers a callback for action item selection.
    pub fn on_select(
        mut self,
        listener: impl Fn(&MenuItemSelectEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Box::new(listener));
        self
    }

    /// Sets whether the trigger control is disabled.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

struct NativeMenuButtonState {
    control_ptr: *mut c_void,
    target_ptr: *mut c_void,
    current_label: SharedString,
    current_items: Vec<NativeMenuItem>,
    attached: bool,
}

impl Drop for NativeMenuButtonState {
    fn drop(&mut self) {
        if self.attached {
            #[cfg(target_os = "macos")]
            unsafe {
                use crate::platform::native_controls;
                super::native_element_helpers::cleanup_native_control(
                    self.control_ptr,
                    self.target_ptr,
                    native_controls::release_native_menu_button_target,
                    native_controls::release_native_menu_button,
                );
            }
        }
    }
}

unsafe impl Send for NativeMenuButtonState {}

#[cfg(target_os = "macos")]
fn map_items(
    items: &[NativeMenuItem],
) -> Vec<crate::platform::native_controls::NativeMenuItemData> {
    fn convert(item: &NativeMenuItem) -> crate::platform::native_controls::NativeMenuItemData {
        match item {
            NativeMenuItem::Action { title, enabled } => {
                crate::platform::native_controls::NativeMenuItemData::Action {
                    title: title.to_string(),
                    enabled: *enabled,
                    icon: None,
                }
            }
            NativeMenuItem::Submenu {
                title,
                enabled,
                items,
            } => crate::platform::native_controls::NativeMenuItemData::Submenu {
                title: title.to_string(),
                enabled: *enabled,
                icon: None,
                items: items.iter().map(convert).collect(),
            },
            NativeMenuItem::Separator => {
                crate::platform::native_controls::NativeMenuItemData::Separator
            }
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
        #[cfg(target_os = "macos")]
        {
            use crate::platform::native_controls;

            let native_view = window.raw_native_view_ptr();
            if native_view.is_null() {
                return;
            }

            let mut on_select = self.on_select.take();
            let label = self.label.clone();
            let items = self.items.clone();
            let disabled = self.disabled;
            let kind = self.kind;

            let next_frame_callbacks = window.next_frame_callbacks.clone();
            let invalidator = window.invalidator.clone();

            window.with_optional_element_state::<NativeMenuButtonState, _>(
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

                            native_controls::set_native_control_enabled(
                                state.control_ptr as cocoa::base::id,
                                !disabled,
                            );
                        }

                        if state.current_label != label {
                            unsafe {
                                native_controls::set_native_menu_button_title(
                                    state.control_ptr as cocoa::base::id,
                                    &label,
                                );
                            }
                            state.current_label = label.clone();
                        }

                        if state.current_items != items || on_select.is_some() {
                            unsafe {
                                native_controls::release_native_menu_button_target(
                                    state.target_ptr,
                                );
                            }

                            let callback = on_select.take().map(|handler| {
                                let nfc = next_frame_callbacks.clone();
                                let inv = invalidator.clone();
                                let handler = Rc::new(handler);
                                schedule_native_callback(
                                    handler,
                                    |index| MenuItemSelectEvent { index },
                                    nfc,
                                    inv,
                                )
                            });

                            let mapped = map_items(&items);
                            unsafe {
                                state.target_ptr = native_controls::set_native_menu_button_items(
                                    state.control_ptr as cocoa::base::id,
                                    &mapped,
                                    callback,
                                );
                            }
                            state.current_items = items.clone();
                        }

                        state
                    } else {
                        let callback = on_select.take().map(|handler| {
                            let nfc = next_frame_callbacks.clone();
                            let inv = invalidator.clone();
                            let handler = Rc::new(handler);
                            schedule_native_callback(
                                handler,
                                |index| MenuItemSelectEvent { index },
                                nfc,
                                inv,
                            )
                        });

                        let mapped = map_items(&items);
                        let (control_ptr, target_ptr) = unsafe {
                            let control = match kind {
                                NativeMenuKind::Button => {
                                    native_controls::create_native_menu_button(&label)
                                }
                                NativeMenuKind::Context => {
                                    native_controls::create_native_context_menu_button(&label)
                                }
                            };

                            native_controls::set_native_control_enabled(control, !disabled);
                            let target = native_controls::set_native_menu_button_items(
                                control, &mapped, callback,
                            );

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

                        NativeMenuButtonState {
                            control_ptr,
                            target_ptr,
                            current_label: label,
                            current_items: items,
                            attached: true,
                        }
                    };

                    ((), Some(state))
                },
            );
        }
    }
}

impl Styled for NativeMenuButton {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
