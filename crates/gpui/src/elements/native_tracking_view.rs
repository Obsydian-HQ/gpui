use refineable::Refineable as _;
use std::cell::RefCell;
use std::rc::Rc;

use crate::platform::native_controls::{
    NativeControlState, TrackingViewCallbacks, TrackingViewConfig,
};
use crate::{
    AbsoluteLength, App, Bounds, DefiniteLength, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Length, Pixels, Point, Style, StyleRefinement,
    Styled, Window, px,
};

use super::native_element_helpers::{schedule_native_callback_no_args, FrameCallback};

/// Event emitted when the mouse enters a tracking view.
#[derive(Clone, Debug)]
pub struct TrackingMouseEnterEvent;

/// Event emitted when the mouse exits a tracking view.
#[derive(Clone, Debug)]
pub struct TrackingMouseExitEvent;

/// Event emitted when the mouse moves within a tracking view.
#[derive(Clone, Debug)]
pub struct TrackingMouseMoveEvent {
    /// The mouse position in the view's local coordinates.
    pub position: Point<Pixels>,
}

/// Creates a native tracking view element that reports mouse enter/exit/move events.
pub fn native_tracking_view(id: impl Into<ElementId>) -> NativeTrackingView {
    NativeTrackingView {
        id: id.into(),
        on_mouse_enter: None,
        on_mouse_exit: None,
        on_mouse_move: None,
        style: StyleRefinement::default(),
    }
}

/// A GPUI element wrapping a custom NSView subclass with NSTrackingArea
/// for detecting mouse enter, exit, and move events.
pub struct NativeTrackingView {
    id: ElementId,
    on_mouse_enter:
        Option<Box<dyn Fn(&TrackingMouseEnterEvent, &mut Window, &mut App) + 'static>>,
    on_mouse_exit:
        Option<Box<dyn Fn(&TrackingMouseExitEvent, &mut Window, &mut App) + 'static>>,
    on_mouse_move:
        Option<Box<dyn Fn(&TrackingMouseMoveEvent, &mut Window, &mut App) + 'static>>,
    style: StyleRefinement,
}

impl NativeTrackingView {
    /// Registers a callback for mouse enter events.
    pub fn on_mouse_enter(
        mut self,
        handler: impl Fn(&TrackingMouseEnterEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_mouse_enter = Some(Box::new(handler));
        self
    }

    /// Registers a callback for mouse exit events.
    pub fn on_mouse_exit(
        mut self,
        handler: impl Fn(&TrackingMouseExitEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_mouse_exit = Some(Box::new(handler));
        self
    }

    /// Registers a callback for mouse move events.
    pub fn on_mouse_move(
        mut self,
        handler: impl Fn(&TrackingMouseMoveEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_mouse_move = Some(Box::new(handler));
        self
    }
}

impl IntoElement for NativeTrackingView {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeTrackingView {
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
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(100.0))));
        }
        if matches!(style.size.height, Length::Auto) {
            style.size.height =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(100.0))));
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

        let on_enter = self.on_mouse_enter.take();
        let on_exit = self.on_mouse_exit.take();
        let on_move = self.on_mouse_move.take();

        let next_frame_callbacks = window.next_frame_callbacks.clone();
        let invalidator = window.invalidator.clone();

        window.with_optional_element_state::<NativeControlState, _>(id, |prev_state, window| {
            let mut state = prev_state.flatten().unwrap_or_default();

            let enter_fn = on_enter.map(|handler| {
                let handler = Rc::new(handler);
                schedule_native_callback_no_args(
                    handler,
                    || TrackingMouseEnterEvent,
                    next_frame_callbacks.clone(),
                    invalidator.clone(),
                )
            });

            let exit_fn = on_exit.map(|handler| {
                let handler = Rc::new(handler);
                schedule_native_callback_no_args(
                    handler,
                    || TrackingMouseExitEvent,
                    next_frame_callbacks.clone(),
                    invalidator.clone(),
                )
            });

            let move_fn: Option<Box<dyn Fn(f64, f64)>> = on_move.map(|handler| {
                let handler = Rc::new(handler);
                let nfc = next_frame_callbacks.clone();
                let inv = invalidator.clone();
                let closure: Box<dyn Fn(f64, f64)> = Box::new(move |x, y| {
                    let handler = handler.clone();
                    let event = TrackingMouseMoveEvent {
                        position: Point {
                            x: px(x as f32),
                            y: px(y as f32),
                        },
                    };
                    let callback: FrameCallback = Box::new(move |window, cx| {
                        handler(&event, window, cx);
                    });
                    RefCell::borrow_mut(&nfc).push(callback);
                    inv.set_dirty(true);
                });
                closure
            });

            let scale = window.scale_factor();
            let nc = window.native_controls();
            nc.update_tracking_view(
                &mut state,
                parent,
                bounds,
                scale,
                TrackingViewConfig {
                    callbacks: TrackingViewCallbacks {
                        on_enter: enter_fn,
                        on_exit: exit_fn,
                        on_move: move_fn,
                    },
                },
            );

            ((), Some(state))
        });
    }
}

impl Styled for NativeTrackingView {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
