#[cfg(any(feature = "inspector", debug_assertions))]
use crate::Inspector;
use crate::{
    Action, AnyDrag, AnyElement, AnyImageCache, AnyTooltip, AnyView, App, AppContext, Arena, Asset,
    AsyncWindowContext, AvailableSpace, Background, BorderStyle, Bounds, BoxShadow, Capslock,
    Context, Corners, CursorStyle, Decorations, DevicePixels, DispatchActionListener,
    DispatchNodeId, DispatchTree, DisplayId, Edges, Effect, Entity, EntityId, EventEmitter,
    FileDropEvent, FontId, Global, GlobalElementId, GlyphId, GpuSpecs, Hsla, InputHandler, IsZero,
    KeyBinding, KeyContext, KeyDownEvent, KeyEvent, Keystroke, KeystrokeEvent, LayoutId,
    LineLayoutIndex, Modifiers, ModifiersChangedEvent, MonochromeSprite, MouseButton, MouseEvent,
    MouseMoveEvent, MouseUpEvent, Path, Pixels, PlatformAtlas, PlatformDisplay, PlatformInput,
    PlatformInputHandler, PlatformNativeSearchFieldTarget, PlatformNativeToolbar,
    PlatformNativeToolbarButtonItem,
    PlatformNativeToolbarComboBoxItem, PlatformNativeToolbarDisplayMode, PlatformNativeToolbarItem,
    PlatformNativeToolbarLabelItem, PlatformNativeToolbarMenuButtonItem,
    PlatformNativeToolbarMenuItemData,
    PlatformNativeToolbarPopUpItem, PlatformNativeToolbarSearchFieldItem,
    PlatformNativeToolbarSegmentedItem, PlatformNativeToolbarSizeMode,
    PlatformNativeAlertStyle, PlatformNativeAlert,
    PlatformNativeColor, PlatformNativePanel, PlatformNativePanelAnchor,
    PlatformNativePanelLevel, PlatformNativePanelMaterial, PlatformNativePanelStyle,
    PlatformNativePopover, PlatformNativePopoverAnchor,
    PlatformNativePopoverBehavior, PlatformNativePopoverContentItem, PlatformWindow, Point,
    PolychromeSprite, Priority, PromptButton, PromptLevel, Quad, Render, RenderGlyphParams,
    RenderImage, RenderImageParams, RenderSvgParams, Replay, ResizeEdge, SMOOTH_SVG_SCALE_FACTOR,
    SUBPIXEL_VARIANTS_X, SUBPIXEL_VARIANTS_Y, ScaledPixels, Scene, Shadow, SharedString, Size,
    StrikethroughStyle, Style, SubpixelSprite, SubscriberSet, Subscription, SystemWindowTab,
    SystemWindowTabController, TabStopMap, TaffyLayoutEngine, Task, TextRenderingMode, TextStyle,
    TextStyleRefinement, ThermalState, TransformationMatrix, Underline, UnderlineStyle,
    WindowAppearance, WindowBackgroundAppearance, WindowBounds, WindowControls, WindowDecorations,
    WindowOptions, WindowParams, WindowTextSystem, point, prelude::*, px, rems, size,
    transparent_black,
};
use anyhow::{Context as _, Result, anyhow};
use collections::{FxHashMap, FxHashSet};
#[cfg(target_os = "macos")]
use core_video::pixel_buffer::CVPixelBuffer;
use derive_more::{Deref, DerefMut};
use futures::FutureExt;
use futures::channel::oneshot;
use itertools::FoldWhile::{Continue, Done};
use itertools::Itertools;
use parking_lot::RwLock;
use raw_window_handle::{HandleError, HasDisplayHandle, HasWindowHandle};
use refineable::Refineable;
use slotmap::SlotMap;
use smallvec::SmallVec;
use std::{
    any::{Any, TypeId},
    borrow::Cow,
    cell::{Cell, RefCell},
    cmp,
    fmt::{Debug, Display},
    hash::{Hash, Hasher},
    marker::PhantomData,
    mem,
    ops::{DerefMut, Range},
    rc::Rc,
    sync::{
        Arc, Weak,
        atomic::{AtomicUsize, Ordering::SeqCst},
    },
    time::{Duration, Instant},
};
use util::post_inc;
use util::{ResultExt, measure};
use uuid::Uuid;

mod prompts;

use crate::util::atomic_incr_if_not_zero;
pub use prompts::*;

/// An identifier for a secondary GPUI rendering surface within a window.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct SurfaceId(pub(crate) u32);

static NEXT_SURFACE_ID: AtomicUsize = AtomicUsize::new(0);

impl SurfaceId {
    /// Allocate a new unique SurfaceId.
    pub fn new() -> Self {
        Self(NEXT_SURFACE_ID.fetch_add(1, SeqCst) as u32)
    }
}

/// A handle returned when registering a surface, providing the native view pointer
/// for embedding in a container NSView.
pub struct GpuiSurfaceHandle {
    /// The SurfaceId for this surface.
    pub id: SurfaceId,
    /// A raw pointer to the surface's NSView (GPUISurfaceView).
    pub native_view_ptr: *mut std::ffi::c_void,
}

/// State for a registered secondary rendering surface.
#[cfg(target_os = "macos")]
pub(crate) struct SurfaceState {
    pub gpui_surface: crate::platform::gpui_surface::GpuiSurface,
    pub root_view: AnyView,
    pub scene: Scene,
    pub layout_engine: Option<TaffyLayoutEngine>,
    pub dirty: bool,
    pub mouse_listeners: Vec<Option<AnyMouseListener>>,
    pub hitboxes: Vec<Hitbox>,
    pub mouse_position: Point<Pixels>,
    pub mouse_hit_test: HitTest,
}

pub(crate) const DEFAULT_WINDOW_SIZE: Size<Pixels> = size(px(1536.), px(864.));

/// A 6:5 aspect ratio minimum window size to be used for functional,
/// additional-to-main-Zed windows, like the settings and rules library windows.
pub const DEFAULT_ADDITIONAL_WINDOW_SIZE: Size<Pixels> = Size {
    width: Pixels(900.),
    height: Pixels(750.),
};

/// Represents the two different phases when dispatching events.
#[derive(Default, Copy, Clone, Debug, Eq, PartialEq)]
pub enum DispatchPhase {
    /// After the capture phase comes the bubble phase, in which mouse event listeners are
    /// invoked front to back and keyboard event listeners are invoked from the focused element
    /// to the root of the element tree. This is the phase you'll most commonly want to use when
    /// registering event listeners.
    #[default]
    Bubble,
    /// During the initial capture phase, mouse event listeners are invoked back to front, and keyboard
    /// listeners are invoked from the root of the tree downward toward the focused element. This phase
    /// is used for special purposes such as clearing the "pressed" state for click events. If
    /// you stop event propagation during this phase, you need to know what you're doing. Handlers
    /// outside of the immediate region may rely on detecting non-local events during this phase.
    Capture,
}

impl DispatchPhase {
    /// Returns true if this represents the "bubble" phase.
    #[inline]
    pub fn bubble(self) -> bool {
        self == DispatchPhase::Bubble
    }

    /// Returns true if this represents the "capture" phase.
    #[inline]
    pub fn capture(self) -> bool {
        self == DispatchPhase::Capture
    }
}

struct WindowInvalidatorInner {
    pub dirty: bool,
    pub draw_phase: DrawPhase,
    pub dirty_views: FxHashSet<EntityId>,
}

#[derive(Clone)]
pub(crate) struct WindowInvalidator {
    inner: Rc<RefCell<WindowInvalidatorInner>>,
}

impl WindowInvalidator {
    pub fn new() -> Self {
        WindowInvalidator {
            inner: Rc::new(RefCell::new(WindowInvalidatorInner {
                dirty: true,
                draw_phase: DrawPhase::None,
                dirty_views: FxHashSet::default(),
            })),
        }
    }

    pub fn invalidate_view(&self, entity: EntityId, cx: &mut App) -> bool {
        let mut inner = self.inner.borrow_mut();
        inner.dirty_views.insert(entity);
        if inner.draw_phase == DrawPhase::None {
            inner.dirty = true;
            cx.push_effect(Effect::Notify { emitter: entity });
            true
        } else {
            false
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.inner.borrow().dirty
    }

    pub fn set_dirty(&self, dirty: bool) {
        self.inner.borrow_mut().dirty = dirty
    }

    pub fn set_phase(&self, phase: DrawPhase) {
        self.inner.borrow_mut().draw_phase = phase
    }

    pub fn take_views(&self) -> FxHashSet<EntityId> {
        mem::take(&mut self.inner.borrow_mut().dirty_views)
    }

    pub fn replace_views(&self, views: FxHashSet<EntityId>) {
        self.inner.borrow_mut().dirty_views = views;
    }

    pub fn not_drawing(&self) -> bool {
        self.inner.borrow().draw_phase == DrawPhase::None
    }

    #[track_caller]
    pub fn debug_assert_paint(&self) {
        debug_assert!(
            matches!(self.inner.borrow().draw_phase, DrawPhase::Paint),
            "this method can only be called during paint"
        );
    }

    #[track_caller]
    pub fn debug_assert_prepaint(&self) {
        debug_assert!(
            matches!(self.inner.borrow().draw_phase, DrawPhase::Prepaint),
            "this method can only be called during request_layout, or prepaint"
        );
    }

    #[track_caller]
    pub fn debug_assert_paint_or_prepaint(&self) {
        debug_assert!(
            matches!(
                self.inner.borrow().draw_phase,
                DrawPhase::Paint | DrawPhase::Prepaint
            ),
            "this method can only be called during request_layout, prepaint, or paint"
        );
    }
}

type AnyObserver = Box<dyn FnMut(&mut Window, &mut App) -> bool + 'static>;

pub(crate) type AnyWindowFocusListener =
    Box<dyn FnMut(&WindowFocusEvent, &mut Window, &mut App) -> bool + 'static>;

pub(crate) struct WindowFocusEvent {
    pub(crate) previous_focus_path: SmallVec<[FocusId; 8]>,
    pub(crate) current_focus_path: SmallVec<[FocusId; 8]>,
}

impl WindowFocusEvent {
    pub fn is_focus_in(&self, focus_id: FocusId) -> bool {
        !self.previous_focus_path.contains(&focus_id) && self.current_focus_path.contains(&focus_id)
    }

    pub fn is_focus_out(&self, focus_id: FocusId) -> bool {
        self.previous_focus_path.contains(&focus_id) && !self.current_focus_path.contains(&focus_id)
    }
}

/// This is provided when subscribing for `Context::on_focus_out` events.
pub struct FocusOutEvent {
    /// A weak focus handle representing what was blurred.
    pub blurred: WeakFocusHandle,
}

slotmap::new_key_type! {
    /// A globally unique identifier for a focusable element.
    pub struct FocusId;
}

thread_local! {
    /// Fallback arena used when no app-specific arena is active.
    /// In production, each window draw sets CURRENT_ELEMENT_ARENA to the app's arena.
    pub(crate) static ELEMENT_ARENA: RefCell<Arena> = RefCell::new(Arena::new(1024 * 1024));

    /// Points to the current App's element arena during draw operations.
    /// This allows multiple test Apps to have isolated arenas, preventing
    /// cross-session corruption when the scheduler interleaves their tasks.
    static CURRENT_ELEMENT_ARENA: Cell<Option<*const RefCell<Arena>>> = const { Cell::new(None) };
}

/// Allocates an element in the current arena. Uses the app-specific arena if one
/// is active (during draw), otherwise falls back to the thread-local ELEMENT_ARENA.
pub(crate) fn with_element_arena<R>(f: impl FnOnce(&mut Arena) -> R) -> R {
    CURRENT_ELEMENT_ARENA.with(|current| {
        if let Some(arena_ptr) = current.get() {
            // SAFETY: The pointer is valid for the duration of the draw operation
            // that set it, and we're being called during that same draw.
            let arena_cell = unsafe { &*arena_ptr };
            f(&mut arena_cell.borrow_mut())
        } else {
            ELEMENT_ARENA.with_borrow_mut(f)
        }
    })
}

/// RAII guard that sets CURRENT_ELEMENT_ARENA for the duration of a draw operation.
/// When dropped, restores the previous arena (supporting nested draws).
pub(crate) struct ElementArenaScope {
    previous: Option<*const RefCell<Arena>>,
}

impl ElementArenaScope {
    /// Enter a scope where element allocations use the given arena.
    pub(crate) fn enter(arena: &RefCell<Arena>) -> Self {
        let previous = CURRENT_ELEMENT_ARENA.with(|current| {
            let prev = current.get();
            current.set(Some(arena as *const RefCell<Arena>));
            prev
        });
        Self { previous }
    }
}

impl Drop for ElementArenaScope {
    fn drop(&mut self) {
        CURRENT_ELEMENT_ARENA.with(|current| {
            current.set(self.previous);
        });
    }
}

/// Returned when the element arena has been used and so must be cleared before the next draw.
#[must_use]
pub struct ArenaClearNeeded {
    arena: *const RefCell<Arena>,
}

impl ArenaClearNeeded {
    /// Create a new ArenaClearNeeded that will clear the given arena.
    pub(crate) fn new(arena: &RefCell<Arena>) -> Self {
        Self {
            arena: arena as *const RefCell<Arena>,
        }
    }

    /// Clear the element arena.
    pub fn clear(self) {
        // SAFETY: The arena pointer is valid because ArenaClearNeeded is created
        // at the end of draw() and must be cleared before the next draw.
        let arena_cell = unsafe { &*self.arena };
        arena_cell.borrow_mut().clear();
    }
}

pub(crate) type FocusMap = RwLock<SlotMap<FocusId, FocusRef>>;
pub(crate) struct FocusRef {
    pub(crate) ref_count: AtomicUsize,
    pub(crate) tab_index: isize,
    pub(crate) tab_stop: bool,
}

impl FocusId {
    /// Obtains whether the element associated with this handle is currently focused.
    pub fn is_focused(&self, window: &Window) -> bool {
        window.focus == Some(*self)
    }

    /// Obtains whether the element associated with this handle contains the focused
    /// element or is itself focused.
    pub fn contains_focused(&self, window: &Window, cx: &App) -> bool {
        window
            .focused(cx)
            .is_some_and(|focused| self.contains(focused.id, window))
    }

    /// Obtains whether the element associated with this handle is contained within the
    /// focused element or is itself focused.
    pub fn within_focused(&self, window: &Window, cx: &App) -> bool {
        let focused = window.focused(cx);
        focused.is_some_and(|focused| focused.id.contains(*self, window))
    }

    /// Obtains whether this handle contains the given handle in the most recently rendered frame.
    pub(crate) fn contains(&self, other: Self, window: &Window) -> bool {
        window
            .rendered_frame
            .dispatch_tree
            .focus_contains(*self, other)
    }
}

/// A handle which can be used to track and manipulate the focused element in a window.
pub struct FocusHandle {
    pub(crate) id: FocusId,
    handles: Arc<FocusMap>,
    /// The index of this element in the tab order.
    pub tab_index: isize,
    /// Whether this element can be focused by tab navigation.
    pub tab_stop: bool,
}

impl std::fmt::Debug for FocusHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("FocusHandle({:?})", self.id))
    }
}

impl FocusHandle {
    pub(crate) fn new(handles: &Arc<FocusMap>) -> Self {
        let id = handles.write().insert(FocusRef {
            ref_count: AtomicUsize::new(1),
            tab_index: 0,
            tab_stop: false,
        });

        Self {
            id,
            tab_index: 0,
            tab_stop: false,
            handles: handles.clone(),
        }
    }

    pub(crate) fn for_id(id: FocusId, handles: &Arc<FocusMap>) -> Option<Self> {
        let lock = handles.read();
        let focus = lock.get(id)?;
        if atomic_incr_if_not_zero(&focus.ref_count) == 0 {
            return None;
        }
        Some(Self {
            id,
            tab_index: focus.tab_index,
            tab_stop: focus.tab_stop,
            handles: handles.clone(),
        })
    }

    /// Sets the tab index of the element associated with this handle.
    pub fn tab_index(mut self, index: isize) -> Self {
        self.tab_index = index;
        if let Some(focus) = self.handles.write().get_mut(self.id) {
            focus.tab_index = index;
        }
        self
    }

    /// Sets whether the element associated with this handle is a tab stop.
    ///
    /// When `false`, the element will not be included in the tab order.
    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        if let Some(focus) = self.handles.write().get_mut(self.id) {
            focus.tab_stop = tab_stop;
        }
        self
    }

    /// Converts this focus handle into a weak variant, which does not prevent it from being released.
    pub fn downgrade(&self) -> WeakFocusHandle {
        WeakFocusHandle {
            id: self.id,
            handles: Arc::downgrade(&self.handles),
        }
    }

    /// Moves the focus to the element associated with this handle.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(self, cx)
    }

    /// Obtains whether the element associated with this handle is currently focused.
    pub fn is_focused(&self, window: &Window) -> bool {
        self.id.is_focused(window)
    }

    /// Obtains whether the element associated with this handle contains the focused
    /// element or is itself focused.
    pub fn contains_focused(&self, window: &Window, cx: &App) -> bool {
        self.id.contains_focused(window, cx)
    }

    /// Obtains whether the element associated with this handle is contained within the
    /// focused element or is itself focused.
    pub fn within_focused(&self, window: &Window, cx: &mut App) -> bool {
        self.id.within_focused(window, cx)
    }

    /// Obtains whether this handle contains the given handle in the most recently rendered frame.
    pub fn contains(&self, other: &Self, window: &Window) -> bool {
        self.id.contains(other.id, window)
    }

    /// Dispatch an action on the element that rendered this focus handle
    pub fn dispatch_action(&self, action: &dyn Action, window: &mut Window, cx: &mut App) {
        if let Some(node_id) = window
            .rendered_frame
            .dispatch_tree
            .focusable_node_id(self.id)
        {
            window.dispatch_action_on_node(node_id, action, cx)
        }
    }
}

impl Clone for FocusHandle {
    fn clone(&self) -> Self {
        Self::for_id(self.id, &self.handles).unwrap()
    }
}

impl PartialEq for FocusHandle {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for FocusHandle {}

impl Drop for FocusHandle {
    fn drop(&mut self) {
        self.handles
            .read()
            .get(self.id)
            .unwrap()
            .ref_count
            .fetch_sub(1, SeqCst);
    }
}

/// A weak reference to a focus handle.
#[derive(Clone, Debug)]
pub struct WeakFocusHandle {
    pub(crate) id: FocusId,
    pub(crate) handles: Weak<FocusMap>,
}

impl WeakFocusHandle {
    /// Attempts to upgrade the [WeakFocusHandle] to a [FocusHandle].
    pub fn upgrade(&self) -> Option<FocusHandle> {
        let handles = self.handles.upgrade()?;
        FocusHandle::for_id(self.id, &handles)
    }
}

impl PartialEq for WeakFocusHandle {
    fn eq(&self, other: &WeakFocusHandle) -> bool {
        self.id == other.id
    }
}

impl Eq for WeakFocusHandle {}

impl PartialEq<FocusHandle> for WeakFocusHandle {
    fn eq(&self, other: &FocusHandle) -> bool {
        self.id == other.id
    }
}

impl PartialEq<WeakFocusHandle> for FocusHandle {
    fn eq(&self, other: &WeakFocusHandle) -> bool {
        self.id == other.id
    }
}

/// Focusable allows users of your view to easily
/// focus it (using window.focus_view(cx, view))
pub trait Focusable: 'static {
    /// Returns the focus handle associated with this view.
    fn focus_handle(&self, cx: &App) -> FocusHandle;
}

impl<V: Focusable> Focusable for Entity<V> {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.read(cx).focus_handle(cx)
    }
}

/// ManagedView is a view (like a Modal, Popover, Menu, etc.)
/// where the lifecycle of the view is handled by another view.
pub trait ManagedView: Focusable + EventEmitter<DismissEvent> + Render {}

impl<M: Focusable + EventEmitter<DismissEvent> + Render> ManagedView for M {}

/// Emitted by implementers of [`ManagedView`] to indicate the view should be dismissed, such as when a view is presented as a modal.
pub struct DismissEvent;

/// Event emitted when a native toolbar button is clicked.
#[derive(Clone, Debug)]
pub struct NativeToolbarClickEvent {
    /// The identifier of the toolbar item that was clicked.
    pub item_id: SharedString,
}

/// Event emitted when a native toolbar search field changes or is submitted.
#[derive(Clone, Debug)]
pub struct NativeToolbarSearchEvent {
    /// The identifier of the search field item.
    pub item_id: SharedString,
    /// The current text of the search field.
    pub text: String,
}

/// Identifies a native search field that can be focused programmatically.
#[derive(Clone, Debug)]
pub enum NativeSearchFieldTarget {
    /// A search field rendered in the native window toolbar.
    ToolbarItem(SharedString),
    /// A content-area native search field rendered via `native_search_field(...)`.
    ContentElement(ElementId),
}

/// Event emitted when a native toolbar segmented control selection changes.
#[derive(Clone, Debug)]
pub struct NativeToolbarSegmentedEvent {
    /// The identifier of the segmented control item.
    pub item_id: SharedString,
    /// The index of the newly selected segment.
    pub selected_index: usize,
}

/// Event emitted when a native toolbar popup button selection changes.
#[derive(Clone, Debug)]
pub struct NativeToolbarPopUpEvent {
    /// The identifier of the popup button item.
    pub item_id: SharedString,
    /// The index of the newly selected item.
    pub selected_index: usize,
}

/// Event emitted when a native toolbar combo box text changes.
#[derive(Clone, Debug)]
pub struct NativeToolbarComboBoxChangeEvent {
    /// The identifier of the combo box item.
    pub item_id: SharedString,
    /// The current text of the combo box.
    pub text: String,
}

/// Event emitted when a native toolbar combo box item is selected.
#[derive(Clone, Debug)]
pub struct NativeToolbarComboBoxSelectEvent {
    /// The identifier of the combo box item.
    pub item_id: SharedString,
    /// The index of the selected item.
    pub selected_index: usize,
}

/// Event emitted when a native toolbar menu button item is selected.
#[derive(Clone, Debug)]
pub struct NativeToolbarMenuButtonSelectEvent {
    /// The identifier of the menu button item.
    pub item_id: SharedString,
    /// The zero-based action index (depth-first across all action items).
    pub index: usize,
}

/// Display mode for the native macOS toolbar.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum NativeToolbarDisplayMode {
    /// Let the system decide.
    #[default]
    Default,
    /// Show icon and label when applicable.
    IconAndLabel,
    /// Show icon only.
    IconOnly,
    /// Show label only.
    LabelOnly,
}

impl From<NativeToolbarDisplayMode> for PlatformNativeToolbarDisplayMode {
    fn from(value: NativeToolbarDisplayMode) -> Self {
        match value {
            NativeToolbarDisplayMode::Default => PlatformNativeToolbarDisplayMode::Default,
            NativeToolbarDisplayMode::IconAndLabel => {
                PlatformNativeToolbarDisplayMode::IconAndLabel
            }
            NativeToolbarDisplayMode::IconOnly => PlatformNativeToolbarDisplayMode::IconOnly,
            NativeToolbarDisplayMode::LabelOnly => PlatformNativeToolbarDisplayMode::LabelOnly,
        }
    }
}

/// Size mode for the native macOS toolbar.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum NativeToolbarSizeMode {
    /// Let the system decide.
    #[default]
    Default,
    /// Standard toolbar sizing.
    Regular,
    /// Compact toolbar sizing.
    Small,
}

impl From<NativeToolbarSizeMode> for PlatformNativeToolbarSizeMode {
    fn from(value: NativeToolbarSizeMode) -> Self {
        match value {
            NativeToolbarSizeMode::Default => PlatformNativeToolbarSizeMode::Default,
            NativeToolbarSizeMode::Regular => PlatformNativeToolbarSizeMode::Regular,
            NativeToolbarSizeMode::Small => PlatformNativeToolbarSizeMode::Small,
        }
    }
}

/// A native toolbar button item.
pub struct NativeToolbarButton {
    id: SharedString,
    label: SharedString,
    tool_tip: Option<SharedString>,
    icon: Option<SharedString>,
    image_url: Option<SharedString>,
    image_circular: bool,
    on_click: Option<Box<dyn Fn(&NativeToolbarClickEvent, &mut Window, &mut App) + 'static>>,
}

impl NativeToolbarButton {
    /// Creates a button item with an identifier and label.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            tool_tip: None,
            icon: None,
            image_url: None,
            image_circular: false,
            on_click: None,
        }
    }

    /// Sets a tooltip for this button.
    pub fn tool_tip(mut self, tool_tip: impl Into<SharedString>) -> Self {
        self.tool_tip = Some(tool_tip.into());
        self
    }

    /// Sets an SF Symbol name to display as the button icon.
    pub fn icon(mut self, symbol_name: impl Into<SharedString>) -> Self {
        self.icon = Some(symbol_name.into());
        self
    }

    /// Sets a URL for an image to display as the button icon.
    /// The image is loaded asynchronously and cached.
    pub fn image_url(mut self, url: impl Into<SharedString>) -> Self {
        self.image_url = Some(url.into());
        self
    }

    /// When true, the loaded image is clipped to a circle (for avatars).
    pub fn image_circular(mut self, circular: bool) -> Self {
        self.image_circular = circular;
        self
    }

    /// Registers a callback invoked when this button is clicked.
    pub fn on_click(
        mut self,
        listener: impl Fn(&NativeToolbarClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(listener));
        self
    }
}

/// A native toolbar search field item.
pub struct NativeToolbarSearchField {
    id: SharedString,
    placeholder: SharedString,
    text: SharedString,
    min_width: Pixels,
    max_width: Pixels,
    on_change: Option<Box<dyn Fn(&NativeToolbarSearchEvent, &mut Window, &mut App) + 'static>>,
    on_submit: Option<Box<dyn Fn(&NativeToolbarSearchEvent, &mut Window, &mut App) + 'static>>,
    on_move_up: Option<Box<dyn Fn(&NativeToolbarSearchEvent, &mut Window, &mut App) + 'static>>,
    on_move_down: Option<Box<dyn Fn(&NativeToolbarSearchEvent, &mut Window, &mut App) + 'static>>,
    on_cancel: Option<Box<dyn Fn(&NativeToolbarSearchEvent, &mut Window, &mut App) + 'static>>,
    on_begin_editing: Option<Box<dyn Fn(&NativeToolbarSearchEvent, &mut Window, &mut App) + 'static>>,
    on_end_editing: Option<Box<dyn Fn(&NativeToolbarSearchEvent, &mut Window, &mut App) + 'static>>,
}

impl NativeToolbarSearchField {
    /// Creates a search field item with an identifier.
    pub fn new(id: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            placeholder: SharedString::default(),
            text: SharedString::default(),
            min_width: px(180.0),
            max_width: px(320.0),
            on_change: None,
            on_submit: None,
            on_move_up: None,
            on_move_down: None,
            on_cancel: None,
            on_begin_editing: None,
            on_end_editing: None,
        }
    }

    /// Sets placeholder text.
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Sets the initial text.
    pub fn text(mut self, text: impl Into<SharedString>) -> Self {
        self.text = text.into();
        self
    }

    /// Sets the minimum width of the search field.
    pub fn min_width(mut self, min_width: Pixels) -> Self {
        self.min_width = min_width;
        self
    }

    /// Sets the maximum width of the search field.
    pub fn max_width(mut self, max_width: Pixels) -> Self {
        self.max_width = max_width;
        self
    }

    /// Registers a callback invoked when the text changes.
    pub fn on_change(
        mut self,
        listener: impl Fn(&NativeToolbarSearchEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Box::new(listener));
        self
    }

    /// Registers a callback invoked when Enter is pressed.
    pub fn on_submit(
        mut self,
        listener: impl Fn(&NativeToolbarSearchEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_submit = Some(Box::new(listener));
        self
    }

    /// Registers a callback invoked when the up arrow key is pressed.
    pub fn on_move_up(
        mut self,
        listener: impl Fn(&NativeToolbarSearchEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_move_up = Some(Box::new(listener));
        self
    }

    /// Registers a callback invoked when the down arrow key is pressed.
    pub fn on_move_down(
        mut self,
        listener: impl Fn(&NativeToolbarSearchEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_move_down = Some(Box::new(listener));
        self
    }

    /// Registers a callback invoked when Escape is pressed.
    pub fn on_cancel(
        mut self,
        listener: impl Fn(&NativeToolbarSearchEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_cancel = Some(Box::new(listener));
        self
    }

    /// Registers a callback invoked when the search field gains focus (editing begins).
    pub fn on_begin_editing(
        mut self,
        listener: impl Fn(&NativeToolbarSearchEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_begin_editing = Some(Box::new(listener));
        self
    }

    /// Registers a callback invoked when the search field loses focus (editing ends).
    pub fn on_end_editing(
        mut self,
        listener: impl Fn(&NativeToolbarSearchEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_end_editing = Some(Box::new(listener));
        self
    }
}

/// A single segment within a native toolbar segmented control.
pub struct NativeToolbarSegment {
    /// The text label for this segment.
    pub label: SharedString,
    /// An optional SF Symbol name to display as an icon.
    pub icon: Option<SharedString>,
}

impl NativeToolbarSegment {
    /// Creates a segment with a label.
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            icon: None,
        }
    }

    /// Sets an SF Symbol icon for this segment.
    pub fn icon(mut self, icon: impl Into<SharedString>) -> Self {
        self.icon = Some(icon.into());
        self
    }
}

/// A native toolbar segmented control item.
pub struct NativeToolbarSegmentedControl {
    id: SharedString,
    segments: Vec<NativeToolbarSegment>,
    selected_index: usize,
    on_select:
        Option<Box<dyn Fn(&NativeToolbarSegmentedEvent, &mut Window, &mut App) + 'static>>,
}

impl NativeToolbarSegmentedControl {
    /// Creates a segmented control with an identifier and segments.
    pub fn new(
        id: impl Into<SharedString>,
        segments: Vec<NativeToolbarSegment>,
    ) -> Self {
        Self {
            id: id.into(),
            segments,
            selected_index: 0,
            on_select: None,
        }
    }

    /// Sets which segment is initially selected.
    pub fn selected_index(mut self, index: usize) -> Self {
        self.selected_index = index;
        self
    }

    /// Registers a callback invoked when the selected segment changes.
    pub fn on_select(
        mut self,
        listener: impl Fn(&NativeToolbarSegmentedEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Box::new(listener));
        self
    }
}

/// A native toolbar popup button item.
pub struct NativeToolbarPopUpButton {
    id: SharedString,
    items: Vec<SharedString>,
    selected_index: usize,
    on_select: Option<Box<dyn Fn(&NativeToolbarPopUpEvent, &mut Window, &mut App) + 'static>>,
}

impl NativeToolbarPopUpButton {
    /// Creates a popup button with an identifier and menu items.
    pub fn new(id: impl Into<SharedString>, items: Vec<SharedString>) -> Self {
        Self {
            id: id.into(),
            items,
            selected_index: 0,
            on_select: None,
        }
    }

    /// Sets which item is initially selected.
    pub fn selected_index(mut self, index: usize) -> Self {
        self.selected_index = index;
        self
    }

    /// Registers a callback invoked when the selected item changes.
    pub fn on_select(
        mut self,
        listener: impl Fn(&NativeToolbarPopUpEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Box::new(listener));
        self
    }
}

/// A native toolbar combo box item (editable text field with dropdown suggestions).
pub struct NativeToolbarComboBox {
    id: SharedString,
    placeholder: SharedString,
    text: SharedString,
    items: Vec<SharedString>,
    min_width: Pixels,
    max_width: Pixels,
    on_change:
        Option<Box<dyn Fn(&NativeToolbarComboBoxChangeEvent, &mut Window, &mut App) + 'static>>,
    on_select:
        Option<Box<dyn Fn(&NativeToolbarComboBoxSelectEvent, &mut Window, &mut App) + 'static>>,
    on_submit:
        Option<Box<dyn Fn(&NativeToolbarComboBoxChangeEvent, &mut Window, &mut App) + 'static>>,
}

impl NativeToolbarComboBox {
    /// Creates a combo box item with an identifier.
    pub fn new(id: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            placeholder: SharedString::default(),
            text: SharedString::default(),
            items: Vec::new(),
            min_width: px(200.0),
            max_width: px(400.0),
            on_change: None,
            on_select: None,
            on_submit: None,
        }
    }

    /// Sets placeholder text.
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Sets the current text value.
    pub fn text(mut self, text: impl Into<SharedString>) -> Self {
        self.text = text.into();
        self
    }

    /// Sets the dropdown suggestion items.
    pub fn items(mut self, items: Vec<SharedString>) -> Self {
        self.items = items;
        self
    }

    /// Sets the minimum width of the combo box.
    pub fn min_width(mut self, min_width: Pixels) -> Self {
        self.min_width = min_width;
        self
    }

    /// Sets the maximum width of the combo box.
    pub fn max_width(mut self, max_width: Pixels) -> Self {
        self.max_width = max_width;
        self
    }

    /// Registers a callback invoked when the text changes.
    pub fn on_change(
        mut self,
        listener: impl Fn(&NativeToolbarComboBoxChangeEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Box::new(listener));
        self
    }

    /// Registers a callback invoked when a dropdown item is selected.
    pub fn on_select(
        mut self,
        listener: impl Fn(&NativeToolbarComboBoxSelectEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Box::new(listener));
        self
    }

    /// Registers a callback invoked when Enter is pressed.
    pub fn on_submit(
        mut self,
        listener: impl Fn(&NativeToolbarComboBoxChangeEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_submit = Some(Box::new(listener));
        self
    }
}

/// A menu item for use in a native toolbar menu button.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeToolbarMenuItem {
    /// A clickable action item.
    Action {
        /// Visible title.
        title: SharedString,
        /// Whether this item is enabled.
        enabled: bool,
        /// Optional SF Symbol name to display as an icon.
        icon: Option<SharedString>,
    },
    /// A submenu containing nested items.
    Submenu {
        /// Visible title.
        title: SharedString,
        /// Whether this submenu is enabled.
        enabled: bool,
        /// Optional SF Symbol name to display as an icon.
        icon: Option<SharedString>,
        /// Nested menu items.
        items: Vec<NativeToolbarMenuItem>,
    },
    /// A visual separator line.
    Separator,
}

impl NativeToolbarMenuItem {
    /// Creates an enabled action item.
    pub fn action(title: impl Into<SharedString>) -> Self {
        Self::Action {
            title: title.into(),
            enabled: true,
            icon: None,
        }
    }

    /// Creates an enabled submenu.
    pub fn submenu(title: impl Into<SharedString>, items: Vec<NativeToolbarMenuItem>) -> Self {
        Self::Submenu {
            title: title.into(),
            enabled: true,
            icon: None,
            items,
        }
    }

    /// Creates a separator item.
    pub fn separator() -> Self {
        Self::Separator
    }

    /// Sets an SF Symbol name to display as an icon on this menu item.
    pub fn icon(self, symbol_name: impl Into<SharedString>) -> Self {
        let icon = Some(symbol_name.into());
        match self {
            Self::Action { title, enabled, .. } => Self::Action {
                title,
                enabled,
                icon,
            },
            Self::Submenu {
                title,
                enabled,
                items,
                ..
            } => Self::Submenu {
                title,
                enabled,
                icon,
                items,
            },
            Self::Separator => Self::Separator,
        }
    }

    /// Sets enabled state on action and submenu items.
    pub fn enabled(self, enabled: bool) -> Self {
        match self {
            Self::Action { title, icon, .. } => Self::Action {
                title,
                enabled,
                icon,
            },
            Self::Submenu {
                title, items, icon, ..
            } => Self::Submenu {
                title,
                enabled,
                icon,
                items,
            },
            Self::Separator => Self::Separator,
        }
    }
}

/// A native toolbar button that opens a dropdown menu (NSMenuToolbarItem).
pub struct NativeToolbarMenuButton {
    id: SharedString,
    label: SharedString,
    tool_tip: Option<SharedString>,
    icon: Option<SharedString>,
    image_url: Option<SharedString>,
    image_circular: bool,
    shows_indicator: bool,
    items: Vec<NativeToolbarMenuItem>,
    on_select:
        Option<Box<dyn Fn(&NativeToolbarMenuButtonSelectEvent, &mut Window, &mut App) + 'static>>,
}

impl NativeToolbarMenuButton {
    /// Creates a new menu button with the given identifier, label, and menu items.
    pub fn new(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        items: Vec<NativeToolbarMenuItem>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            tool_tip: None,
            icon: None,
            image_url: None,
            image_circular: false,
            shows_indicator: true,
            items,
            on_select: None,
        }
    }

    /// Sets an optional tooltip.
    pub fn tool_tip(mut self, tip: impl Into<SharedString>) -> Self {
        self.tool_tip = Some(tip.into());
        self
    }

    /// Sets an SF Symbol name to display as the button icon.
    pub fn icon(mut self, symbol_name: impl Into<SharedString>) -> Self {
        self.icon = Some(symbol_name.into());
        self
    }

    /// Sets a URL for an image to display as the button icon.
    /// The image is loaded asynchronously and cached.
    pub fn image_url(mut self, url: impl Into<SharedString>) -> Self {
        self.image_url = Some(url.into());
        self
    }

    /// When true, the loaded image is clipped to a circle (for avatars).
    pub fn image_circular(mut self, circular: bool) -> Self {
        self.image_circular = circular;
        self
    }

    /// Controls whether the dropdown chevron indicator is visible.
    pub fn shows_indicator(mut self, shows: bool) -> Self {
        self.shows_indicator = shows;
        self
    }

    /// Registers a callback invoked when a menu action item is selected.
    pub fn on_select(
        mut self,
        listener: impl Fn(&NativeToolbarMenuButtonSelectEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Box::new(listener));
        self
    }
}

/// A text-only toolbar label item with no click behavior.
/// Used for informational display (e.g., image metadata, status text).
pub struct NativeToolbarLabel {
    id: SharedString,
    text: SharedString,
    icon: Option<SharedString>,
}

impl NativeToolbarLabel {
    /// Creates a new label item with the given identifier and text.
    pub fn new(id: impl Into<SharedString>, text: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            icon: None,
        }
    }

    /// Sets an SF Symbol name to display alongside the text.
    pub fn icon(mut self, symbol_name: impl Into<SharedString>) -> Self {
        self.icon = Some(symbol_name.into());
        self
    }
}

/// A native toolbar item.
pub enum NativeToolbarItem {
    /// A button item.
    Button(NativeToolbarButton),
    /// A fixed-width spacer item.
    Space,
    /// A flexible spacer item.
    FlexibleSpace,
    /// A search field item.
    SearchField(NativeToolbarSearchField),
    /// A segmented control item.
    SegmentedControl(NativeToolbarSegmentedControl),
    /// A popup button item.
    PopUpButton(NativeToolbarPopUpButton),
    /// A combo box item.
    ComboBox(NativeToolbarComboBox),
    /// A button that opens a dropdown menu (NSMenuToolbarItem).
    MenuButton(NativeToolbarMenuButton),
    /// A text-only label item with no click behavior.
    Label(NativeToolbarLabel),
    /// The system-provided sidebar toggle button (NSToolbarToggleSidebarItemIdentifier).
    /// macOS handles the button appearance and the `toggleSidebar:` action automatically.
    SidebarToggle,
    /// The system-provided sidebar tracking separator (NSToolbarSidebarTrackingSeparatorItemIdentifier).
    /// Items before this separator appear in the sidebar area of the titlebar;
    /// items after it appear in the main content area.
    SidebarTrackingSeparator,
}

impl NativeToolbarItem {
    /// Creates a button item.
    pub fn button(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self::Button(NativeToolbarButton::new(id, label))
    }

    /// Creates a search field item.
    pub fn search_field(id: impl Into<SharedString>) -> Self {
        Self::SearchField(NativeToolbarSearchField::new(id))
    }

    /// Creates a segmented control item.
    pub fn segmented_control(
        id: impl Into<SharedString>,
        segments: Vec<NativeToolbarSegment>,
    ) -> Self {
        Self::SegmentedControl(NativeToolbarSegmentedControl::new(id, segments))
    }

    /// Creates a popup button item.
    pub fn popup_button(id: impl Into<SharedString>, items: Vec<SharedString>) -> Self {
        Self::PopUpButton(NativeToolbarPopUpButton::new(id, items))
    }

    /// Creates a combo box item.
    pub fn combo_box(id: impl Into<SharedString>) -> Self {
        Self::ComboBox(NativeToolbarComboBox::new(id))
    }

    /// Creates a menu button item (toolbar button that opens a dropdown menu).
    pub fn menu_button(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        items: Vec<NativeToolbarMenuItem>,
    ) -> Self {
        Self::MenuButton(NativeToolbarMenuButton::new(id, label, items))
    }

    /// Creates a text-only label item with no click behavior.
    pub fn label(id: impl Into<SharedString>, text: impl Into<SharedString>) -> Self {
        Self::Label(NativeToolbarLabel::new(id, text))
    }
}

/// Configuration for a native macOS toolbar.
pub struct NativeToolbar {
    identifier: SharedString,
    title: Option<SharedString>,
    display_mode: NativeToolbarDisplayMode,
    size_mode: NativeToolbarSizeMode,
    shows_baseline_separator: bool,
    items: Vec<NativeToolbarItem>,
}

impl NativeToolbar {
    /// Creates a toolbar configuration with a stable identifier.
    pub fn new(identifier: impl Into<SharedString>) -> Self {
        Self {
            identifier: identifier.into(),
            title: None,
            display_mode: NativeToolbarDisplayMode::Default,
            size_mode: NativeToolbarSizeMode::Default,
            shows_baseline_separator: true,
            items: Vec::new(),
        }
    }

    /// Sets an optional title to apply to the window when this toolbar is installed.
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Sets the toolbar display mode.
    pub fn display_mode(mut self, mode: NativeToolbarDisplayMode) -> Self {
        self.display_mode = mode;
        self
    }

    /// Sets the toolbar size mode.
    pub fn size_mode(mut self, mode: NativeToolbarSizeMode) -> Self {
        self.size_mode = mode;
        self
    }

    /// Controls whether the baseline separator is shown.
    pub fn shows_baseline_separator(mut self, shows: bool) -> Self {
        self.shows_baseline_separator = shows;
        self
    }

    /// Appends one toolbar item.
    pub fn item(mut self, item: NativeToolbarItem) -> Self {
        self.items.push(item);
        self
    }

    /// Appends several toolbar items.
    pub fn items(mut self, items: impl IntoIterator<Item = NativeToolbarItem>) -> Self {
        self.items.extend(items);
        self
    }

    fn into_platform(
        self,
        next_frame_callbacks: Rc<RefCell<Vec<FrameCallback>>>,
        invalidator: WindowInvalidator,
    ) -> PlatformNativeToolbar {
        let items = self
            .items
            .into_iter()
            .map(|item| match item {
                NativeToolbarItem::Button(button) => {
                    let on_click = button.on_click.map(|handler| {
                        let item_id = button.id.clone();
                        schedule_native_toolbar_callback_no_args(
                            Rc::new(handler),
                            move || NativeToolbarClickEvent {
                                item_id: item_id.clone(),
                            },
                            next_frame_callbacks.clone(),
                            invalidator.clone(),
                        )
                    });

                    PlatformNativeToolbarItem::Button(PlatformNativeToolbarButtonItem {
                        id: button.id,
                        label: button.label,
                        tool_tip: button.tool_tip,
                        icon: button.icon,
                        image_url: button.image_url,
                        image_circular: button.image_circular,
                        on_click,
                    })
                }
                NativeToolbarItem::Space => PlatformNativeToolbarItem::Space,
                NativeToolbarItem::FlexibleSpace => PlatformNativeToolbarItem::FlexibleSpace,
                NativeToolbarItem::SidebarToggle => PlatformNativeToolbarItem::SidebarToggle,
                NativeToolbarItem::SidebarTrackingSeparator => {
                    PlatformNativeToolbarItem::SidebarTrackingSeparator
                }
                NativeToolbarItem::SearchField(search) => {
                    let search_id = search.id.clone();
                    let on_change = search.on_change.map(|handler| {
                        schedule_native_toolbar_callback(
                            Rc::new(handler),
                            move |text: String| NativeToolbarSearchEvent {
                                item_id: search_id.clone(),
                                text,
                            },
                            next_frame_callbacks.clone(),
                            invalidator.clone(),
                        )
                    });

                    let submit_id = search.id.clone();
                    let on_submit = search.on_submit.map(|handler| {
                        schedule_native_toolbar_callback(
                            Rc::new(handler),
                            move |text: String| NativeToolbarSearchEvent {
                                item_id: submit_id.clone(),
                                text,
                            },
                            next_frame_callbacks.clone(),
                            invalidator.clone(),
                        )
                    });

                    let move_up_id = search.id.clone();
                    let on_move_up = search.on_move_up.map(|handler| {
                        schedule_native_toolbar_callback_no_args(
                            Rc::new(handler),
                            move || NativeToolbarSearchEvent {
                                item_id: move_up_id.clone(),
                                text: String::new(),
                            },
                            next_frame_callbacks.clone(),
                            invalidator.clone(),
                        )
                    });

                    let move_down_id = search.id.clone();
                    let on_move_down = search.on_move_down.map(|handler| {
                        schedule_native_toolbar_callback_no_args(
                            Rc::new(handler),
                            move || NativeToolbarSearchEvent {
                                item_id: move_down_id.clone(),
                                text: String::new(),
                            },
                            next_frame_callbacks.clone(),
                            invalidator.clone(),
                        )
                    });

                    let cancel_id = search.id.clone();
                    let on_cancel = search.on_cancel.map(|handler| {
                        schedule_native_toolbar_callback_no_args(
                            Rc::new(handler),
                            move || NativeToolbarSearchEvent {
                                item_id: cancel_id.clone(),
                                text: String::new(),
                            },
                            next_frame_callbacks.clone(),
                            invalidator.clone(),
                        )
                    });

                    let begin_editing_id = search.id.clone();
                    let on_begin_editing = search.on_begin_editing.map(|handler| {
                        schedule_native_toolbar_callback_no_args(
                            Rc::new(handler),
                            move || NativeToolbarSearchEvent {
                                item_id: begin_editing_id.clone(),
                                text: String::new(),
                            },
                            next_frame_callbacks.clone(),
                            invalidator.clone(),
                        )
                    });

                    let end_editing_id = search.id.clone();
                    let on_end_editing = search.on_end_editing.map(|handler| {
                        schedule_native_toolbar_callback(
                            Rc::new(handler),
                            move |text: String| NativeToolbarSearchEvent {
                                item_id: end_editing_id.clone(),
                                text,
                            },
                            next_frame_callbacks.clone(),
                            invalidator.clone(),
                        )
                    });

                    PlatformNativeToolbarItem::SearchField(PlatformNativeToolbarSearchFieldItem {
                        id: search.id,
                        placeholder: search.placeholder,
                        text: search.text,
                        min_width: search.min_width,
                        max_width: search.max_width,
                        on_change,
                        on_submit,
                        on_move_up,
                        on_move_down,
                        on_cancel,
                        on_begin_editing,
                        on_end_editing,
                    })
                }
                NativeToolbarItem::SegmentedControl(segmented) => {
                    let seg_id = segmented.id.clone();
                    let on_select = segmented.on_select.map(|handler| {
                        schedule_native_toolbar_callback(
                            Rc::new(handler),
                            move |selected_index: usize| NativeToolbarSegmentedEvent {
                                item_id: seg_id.clone(),
                                selected_index,
                            },
                            next_frame_callbacks.clone(),
                            invalidator.clone(),
                        )
                    });

                    let labels: Vec<SharedString> =
                        segmented.segments.iter().map(|s| s.label.clone()).collect();
                    let icons: Vec<Option<SharedString>> =
                        segmented.segments.into_iter().map(|s| s.icon).collect();

                    PlatformNativeToolbarItem::SegmentedControl(
                        PlatformNativeToolbarSegmentedItem {
                            id: segmented.id,
                            labels,
                            icons,
                            selected_index: segmented.selected_index,
                            on_select,
                        },
                    )
                }
                NativeToolbarItem::PopUpButton(popup) => {
                    let popup_id = popup.id.clone();
                    let on_select = popup.on_select.map(|handler| {
                        schedule_native_toolbar_callback(
                            Rc::new(handler),
                            move |selected_index: usize| NativeToolbarPopUpEvent {
                                item_id: popup_id.clone(),
                                selected_index,
                            },
                            next_frame_callbacks.clone(),
                            invalidator.clone(),
                        )
                    });

                    PlatformNativeToolbarItem::PopUpButton(PlatformNativeToolbarPopUpItem {
                        id: popup.id,
                        items: popup.items,
                        selected_index: popup.selected_index,
                        on_select,
                    })
                }
                NativeToolbarItem::ComboBox(combo) => {
                    let change_id = combo.id.clone();
                    let on_change = combo.on_change.map(|handler| {
                        schedule_native_toolbar_callback(
                            Rc::new(handler),
                            move |text: String| NativeToolbarComboBoxChangeEvent {
                                item_id: change_id.clone(),
                                text,
                            },
                            next_frame_callbacks.clone(),
                            invalidator.clone(),
                        )
                    });

                    let select_id = combo.id.clone();
                    let on_select = combo.on_select.map(|handler| {
                        schedule_native_toolbar_callback(
                            Rc::new(handler),
                            move |selected_index: usize| NativeToolbarComboBoxSelectEvent {
                                item_id: select_id.clone(),
                                selected_index,
                            },
                            next_frame_callbacks.clone(),
                            invalidator.clone(),
                        )
                    });

                    let submit_id = combo.id.clone();
                    let on_submit = combo.on_submit.map(|handler| {
                        schedule_native_toolbar_callback(
                            Rc::new(handler),
                            move |text: String| NativeToolbarComboBoxChangeEvent {
                                item_id: submit_id.clone(),
                                text,
                            },
                            next_frame_callbacks.clone(),
                            invalidator.clone(),
                        )
                    });

                    PlatformNativeToolbarItem::ComboBox(PlatformNativeToolbarComboBoxItem {
                        id: combo.id,
                        placeholder: combo.placeholder,
                        text: combo.text,
                        items: combo.items,
                        min_width: combo.min_width,
                        max_width: combo.max_width,
                        on_change,
                        on_select,
                        on_submit,
                    })
                }
                NativeToolbarItem::MenuButton(menu_button) => {
                    let btn_id = menu_button.id.clone();
                    let on_select = menu_button.on_select.map(|handler| {
                        schedule_native_toolbar_callback(
                            Rc::new(handler),
                            move |index: usize| NativeToolbarMenuButtonSelectEvent {
                                item_id: btn_id.clone(),
                                index,
                            },
                            next_frame_callbacks.clone(),
                            invalidator.clone(),
                        )
                    });

                    PlatformNativeToolbarItem::MenuButton(PlatformNativeToolbarMenuButtonItem {
                        id: menu_button.id,
                        label: menu_button.label,
                        tool_tip: menu_button.tool_tip,
                        icon: menu_button.icon,
                        image_url: menu_button.image_url,
                        image_circular: menu_button.image_circular,
                        shows_indicator: menu_button.shows_indicator,
                        items: convert_toolbar_menu_items(&menu_button.items),
                        on_select,
                    })
                }
                NativeToolbarItem::Label(label) => {
                    PlatformNativeToolbarItem::Label(PlatformNativeToolbarLabelItem {
                        id: label.id,
                        text: label.text,
                        icon: label.icon,
                    })
                }
            })
            .collect();

        PlatformNativeToolbar {
            identifier: self.identifier,
            title: self.title,
            display_mode: self.display_mode.into(),
            size_mode: self.size_mode.into(),
            shows_baseline_separator: self.shows_baseline_separator,
            items,
        }
    }
}

fn convert_toolbar_menu_items(
    items: &[NativeToolbarMenuItem],
) -> Vec<PlatformNativeToolbarMenuItemData> {
    items
        .iter()
        .map(|item| match item {
            NativeToolbarMenuItem::Action {
                title,
                enabled,
                icon,
            } => PlatformNativeToolbarMenuItemData::Action {
                title: title.clone(),
                enabled: *enabled,
                icon: icon.clone(),
            },
            NativeToolbarMenuItem::Submenu {
                title,
                enabled,
                icon,
                items,
            } => PlatformNativeToolbarMenuItemData::Submenu {
                title: title.clone(),
                enabled: *enabled,
                icon: icon.clone(),
                items: convert_toolbar_menu_items(items),
            },
            NativeToolbarMenuItem::Separator => PlatformNativeToolbarMenuItemData::Separator,
        })
        .collect()
}

fn schedule_native_toolbar_callback_no_args<Event: 'static>(
    handler: Rc<Box<dyn Fn(&Event, &mut Window, &mut App)>>,
    make_event: impl Fn() -> Event + 'static,
    next_frame_callbacks: Rc<RefCell<Vec<FrameCallback>>>,
    invalidator: WindowInvalidator,
) -> Box<dyn Fn()> {
    Box::new(move || {
        let handler = handler.clone();
        let event = make_event();
        let callback: FrameCallback = Box::new(move |window, cx| {
            handler(&event, window, cx);
        });
        RefCell::borrow_mut(&next_frame_callbacks).push(callback);
        invalidator.set_dirty(true);
    })
}

fn schedule_native_toolbar_callback<P: 'static, Event: 'static>(
    handler: Rc<Box<dyn Fn(&Event, &mut Window, &mut App)>>,
    make_event: impl Fn(P) -> Event + 'static,
    next_frame_callbacks: Rc<RefCell<Vec<FrameCallback>>>,
    invalidator: WindowInvalidator,
) -> Box<dyn Fn(P)> {
    Box::new(move |param| {
        let handler = handler.clone();
        let event = make_event(param);
        let callback: FrameCallback = Box::new(move |window, cx| {
            handler(&event, window, cx);
        });
        RefCell::borrow_mut(&next_frame_callbacks).push(callback);
        invalidator.set_dirty(true);
    })
}

// =============================================================================
// Shared content item converter
// =============================================================================

fn convert_popover_content_items(
    items: Vec<NativePopoverContentItem>,
    next_frame_callbacks: &Rc<RefCell<Vec<FrameCallback>>>,
    invalidator: &WindowInvalidator,
) -> Vec<PlatformNativePopoverContentItem> {
    items
        .into_iter()
        .map(|item| match item {
            NativePopoverContentItem::Label { text, bold } => {
                PlatformNativePopoverContentItem::Label { text, bold }
            }
            NativePopoverContentItem::SmallLabel { text } => {
                PlatformNativePopoverContentItem::SmallLabel { text }
            }
            NativePopoverContentItem::IconLabel { icon, text } => {
                PlatformNativePopoverContentItem::IconLabel { icon, text }
            }
            NativePopoverContentItem::Button { title, on_click } => {
                let platform_on_click = on_click.map(|handler| -> Box<dyn Fn()> {
                    let handler = Rc::new(handler);
                    let nfc = next_frame_callbacks.clone();
                    let inv = invalidator.clone();
                    Box::new(move || {
                        let handler = handler.clone();
                        let callback: FrameCallback =
                            Box::new(move |window, cx| handler(window, cx));
                        RefCell::borrow_mut(&nfc).push(callback);
                        inv.set_dirty(true);
                    })
                });
                PlatformNativePopoverContentItem::Button {
                    title,
                    on_click: platform_on_click,
                }
            }
            NativePopoverContentItem::Toggle {
                text,
                checked,
                on_change,
                enabled,
                description,
            } => {
                let platform_on_change = on_change.map(|handler| -> Box<dyn Fn(bool)> {
                    let handler = Rc::new(handler);
                    let nfc = next_frame_callbacks.clone();
                    let inv = invalidator.clone();
                    Box::new(move |checked| {
                        let handler = handler.clone();
                        let callback: FrameCallback =
                            Box::new(move |window, cx| handler(checked, window, cx));
                        RefCell::borrow_mut(&nfc).push(callback);
                        inv.set_dirty(true);
                    })
                });
                PlatformNativePopoverContentItem::Toggle {
                    text,
                    checked,
                    on_change: platform_on_change,
                    enabled,
                    description,
                }
            }
            NativePopoverContentItem::Checkbox {
                text,
                checked,
                on_change,
                enabled,
            } => {
                let platform_on_change = on_change.map(|handler| -> Box<dyn Fn(bool)> {
                    let handler = Rc::new(handler);
                    let nfc = next_frame_callbacks.clone();
                    let inv = invalidator.clone();
                    Box::new(move |checked| {
                        let handler = handler.clone();
                        let callback: FrameCallback =
                            Box::new(move |window, cx| handler(checked, window, cx));
                        RefCell::borrow_mut(&nfc).push(callback);
                        inv.set_dirty(true);
                    })
                });
                PlatformNativePopoverContentItem::Checkbox {
                    text,
                    checked,
                    on_change: platform_on_change,
                    enabled,
                }
            }
            NativePopoverContentItem::ProgressBar { value, max, label } => {
                PlatformNativePopoverContentItem::ProgressBar { value, max, label }
            }
            NativePopoverContentItem::ColorDot {
                color,
                text,
                detail,
                on_click,
            } => {
                let platform_color = match color {
                    NativeColor::Red => PlatformNativeColor::Red,
                    NativeColor::Orange => PlatformNativeColor::Orange,
                    NativeColor::Yellow => PlatformNativeColor::Yellow,
                    NativeColor::Green => PlatformNativeColor::Green,
                    NativeColor::Blue => PlatformNativeColor::Blue,
                    NativeColor::Purple => PlatformNativeColor::Purple,
                    NativeColor::Gray => PlatformNativeColor::Gray,
                    NativeColor::Primary => PlatformNativeColor::Primary,
                    NativeColor::Secondary => PlatformNativeColor::Secondary,
                };
                let platform_on_click = on_click.map(|handler| -> Box<dyn Fn()> {
                    let handler = Rc::new(handler);
                    let nfc = next_frame_callbacks.clone();
                    let inv = invalidator.clone();
                    Box::new(move || {
                        let handler = handler.clone();
                        let callback: FrameCallback =
                            Box::new(move |window, cx| handler(window, cx));
                        RefCell::borrow_mut(&nfc).push(callback);
                        inv.set_dirty(true);
                    })
                });
                PlatformNativePopoverContentItem::ColorDot {
                    color: platform_color,
                    text,
                    detail,
                    on_click: platform_on_click,
                }
            }
            NativePopoverContentItem::ClickableRow {
                icon,
                text,
                detail,
                on_click,
                enabled,
                selected,
            } => {
                let platform_on_click = on_click.map(|handler| -> Box<dyn Fn()> {
                    let handler = Rc::new(handler);
                    let nfc = next_frame_callbacks.clone();
                    let inv = invalidator.clone();
                    Box::new(move || {
                        let handler = handler.clone();
                        let callback: FrameCallback =
                            Box::new(move |window, cx| handler(window, cx));
                        RefCell::borrow_mut(&nfc).push(callback);
                        inv.set_dirty(true);
                    })
                });
                PlatformNativePopoverContentItem::ClickableRow {
                    icon,
                    text,
                    detail,
                    on_click: platform_on_click,
                    enabled,
                    selected,
                }
            }
            NativePopoverContentItem::Separator => PlatformNativePopoverContentItem::Separator,
        })
        .collect()
}

// =============================================================================
// Native Popover
// =============================================================================

/// Event emitted when a native popover is closed.
#[derive(Clone, Debug)]
pub struct NativePopoverCloseEvent;

/// Event emitted when a native popover is shown.
#[derive(Clone, Debug)]
pub struct NativePopoverShowEvent;

/// Behavior of a native popover.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NativePopoverBehavior {
    /// The app controls when the popover closes.
    ApplicationDefined,
    /// The popover closes when the user clicks outside of it.
    #[default]
    Transient,
    /// The popover closes when the user clicks outside, but not in the popover's
    /// positioning view.
    Semitransient,
}

/// Specifies where a popover should appear relative to its anchor.
#[derive(Clone, Debug)]
pub enum NativePopoverAnchor {
    /// Anchor the popover to a toolbar item with the given identifier.
    /// Uses `showRelativeToToolbarItem:` on macOS 14+.
    ToolbarItem(SharedString),
}

/// A named color for use in native popover content items.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeColor {
    /// System red.
    Red,
    /// System orange.
    Orange,
    /// System yellow.
    Yellow,
    /// System green.
    Green,
    /// System blue.
    Blue,
    /// System purple.
    Purple,
    /// System gray.
    Gray,
    /// Primary label color.
    Primary,
    /// Secondary label color.
    Secondary,
}

/// A content item to display inside a native popover.
pub enum NativePopoverContentItem {
    /// A non-editable text label.
    Label {
        /// The text to display.
        text: SharedString,
        /// Whether the label should be bold (heading style).
        bold: bool,
    },
    /// A smaller, secondary text label (for metadata/details).
    SmallLabel {
        /// The text to display.
        text: SharedString,
    },
    /// A label with an SF Symbol icon to its left.
    IconLabel {
        /// SF Symbol name (e.g. "arrow.triangle.branch", "globe").
        icon: SharedString,
        /// The text to display next to the icon.
        text: SharedString,
    },
    /// A clickable button.
    Button {
        /// The button's title text.
        title: SharedString,
        /// Callback invoked when the button is clicked.
        on_click: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
    },
    /// A label on the left with an NSSwitch toggle on the right.
    Toggle {
        /// The label text.
        text: SharedString,
        /// Whether the toggle is currently on.
        checked: bool,
        /// Callback invoked when the toggle state changes.
        on_change: Option<Box<dyn Fn(bool, &mut Window, &mut App) + 'static>>,
        /// Whether the toggle is interactive.
        enabled: bool,
        /// Optional smaller description text below the label.
        description: Option<SharedString>,
    },
    /// A native checkbox (NSButton in checkbox mode).
    Checkbox {
        /// The checkbox label text.
        text: SharedString,
        /// Whether the checkbox is currently checked.
        checked: bool,
        /// Callback invoked when the checkbox state changes.
        on_change: Option<Box<dyn Fn(bool, &mut Window, &mut App) + 'static>>,
        /// Whether the checkbox is interactive.
        enabled: bool,
    },
    /// A determinate progress bar with an optional text label.
    ProgressBar {
        /// Current progress value.
        value: f64,
        /// Maximum progress value.
        max: f64,
        /// Optional label text displayed below the bar.
        label: Option<SharedString>,
    },
    /// A colored circle with text and optional detail.
    ColorDot {
        /// The dot color.
        color: NativeColor,
        /// Primary text next to the dot.
        text: SharedString,
        /// Optional secondary detail text.
        detail: Option<SharedString>,
        /// Callback invoked when the row is clicked.
        on_click: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
    },
    /// A full-width clickable row with optional icon and detail text.
    ClickableRow {
        /// Optional SF Symbol icon name.
        icon: Option<SharedString>,
        /// Primary text.
        text: SharedString,
        /// Optional secondary detail text.
        detail: Option<SharedString>,
        /// Callback invoked when the row is clicked.
        on_click: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
        /// Whether the row is interactive.
        enabled: bool,
        /// Whether the row is visually selected (e.g. via keyboard navigation).
        selected: bool,
    },
    /// A horizontal separator line.
    Separator,
}

impl NativePopoverContentItem {
    /// Creates a text label item.
    pub fn label(text: impl Into<SharedString>) -> Self {
        Self::Label {
            text: text.into(),
            bold: false,
        }
    }

    /// Creates a bold text label item (e.g. for headings).
    pub fn heading(text: impl Into<SharedString>) -> Self {
        Self::Label {
            text: text.into(),
            bold: true,
        }
    }

    /// Creates a smaller, muted text label (for metadata like timestamps, descriptions).
    pub fn small_label(text: impl Into<SharedString>) -> Self {
        Self::SmallLabel { text: text.into() }
    }

    /// Creates a label with an SF Symbol icon to its left.
    pub fn icon_label(icon: impl Into<SharedString>, text: impl Into<SharedString>) -> Self {
        Self::IconLabel {
            icon: icon.into(),
            text: text.into(),
        }
    }

    /// Creates a button item.
    pub fn button(
        title: impl Into<SharedString>,
        on_click: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self::Button {
            title: title.into(),
            on_click: Some(Box::new(on_click)),
        }
    }

    /// Creates a separator item.
    pub fn separator() -> Self {
        Self::Separator
    }
}

/// Builder for a toggle (label + NSSwitch) popover content item.
pub struct NativePopoverToggle {
    text: SharedString,
    checked: bool,
    on_change: Option<Box<dyn Fn(bool, &mut Window, &mut App) + 'static>>,
    enabled: bool,
    description: Option<SharedString>,
}

impl NativePopoverToggle {
    /// Creates a new toggle builder.
    pub fn new(text: impl Into<SharedString>, checked: bool) -> Self {
        Self {
            text: text.into(),
            checked,
            on_change: None,
            enabled: true,
            description: None,
        }
    }

    /// Sets the callback invoked when the toggle state changes.
    pub fn on_change(mut self, handler: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Box::new(handler));
        self
    }

    /// Sets whether the toggle is interactive.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Sets a description shown below the label.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }
}

impl From<NativePopoverToggle> for NativePopoverContentItem {
    fn from(builder: NativePopoverToggle) -> Self {
        Self::Toggle {
            text: builder.text,
            checked: builder.checked,
            on_change: builder.on_change,
            enabled: builder.enabled,
            description: builder.description,
        }
    }
}

/// Builder for a checkbox popover content item.
pub struct NativePopoverCheckbox {
    text: SharedString,
    checked: bool,
    on_change: Option<Box<dyn Fn(bool, &mut Window, &mut App) + 'static>>,
    enabled: bool,
}

impl NativePopoverCheckbox {
    /// Creates a new checkbox builder.
    pub fn new(text: impl Into<SharedString>, checked: bool) -> Self {
        Self {
            text: text.into(),
            checked,
            on_change: None,
            enabled: true,
        }
    }

    /// Sets the callback invoked when the checkbox state changes.
    pub fn on_change(mut self, handler: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Box::new(handler));
        self
    }

    /// Sets whether the checkbox is interactive.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl From<NativePopoverCheckbox> for NativePopoverContentItem {
    fn from(builder: NativePopoverCheckbox) -> Self {
        Self::Checkbox {
            text: builder.text,
            checked: builder.checked,
            on_change: builder.on_change,
            enabled: builder.enabled,
        }
    }
}

/// Builder for a progress bar popover content item.
pub struct NativePopoverProgress {
    value: f64,
    max: f64,
    label: Option<SharedString>,
}

impl NativePopoverProgress {
    /// Creates a new progress bar builder.
    pub fn new(value: f64, max: f64) -> Self {
        Self {
            value,
            max,
            label: None,
        }
    }

    /// Sets a label displayed below the progress bar.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl From<NativePopoverProgress> for NativePopoverContentItem {
    fn from(builder: NativePopoverProgress) -> Self {
        Self::ProgressBar {
            value: builder.value,
            max: builder.max,
            label: builder.label,
        }
    }
}

/// Builder for a color-dot popover content item.
pub struct NativePopoverColorDot {
    color: NativeColor,
    text: SharedString,
    detail: Option<SharedString>,
    on_click: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
}

impl NativePopoverColorDot {
    /// Creates a new color-dot builder.
    pub fn new(text: impl Into<SharedString>, color: NativeColor) -> Self {
        Self {
            color,
            text: text.into(),
            detail: None,
            on_click: None,
        }
    }

    /// Sets secondary detail text.
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Sets the callback invoked when the row is clicked.
    pub fn on_click(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl From<NativePopoverColorDot> for NativePopoverContentItem {
    fn from(builder: NativePopoverColorDot) -> Self {
        Self::ColorDot {
            color: builder.color,
            text: builder.text,
            detail: builder.detail,
            on_click: builder.on_click,
        }
    }
}

/// Builder for a clickable-row popover content item.
pub struct NativePopoverClickableRow {
    icon: Option<SharedString>,
    text: SharedString,
    detail: Option<SharedString>,
    on_click: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
    enabled: bool,
    selected: bool,
}

impl NativePopoverClickableRow {
    /// Creates a new clickable-row builder.
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            icon: None,
            text: text.into(),
            detail: None,
            on_click: None,
            enabled: true,
            selected: false,
        }
    }

    /// Sets an SF Symbol icon.
    pub fn icon(mut self, icon: impl Into<SharedString>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Sets secondary detail text.
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Sets the callback invoked when the row is clicked.
    pub fn on_click(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }

    /// Sets whether the row is interactive.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Sets whether the row is visually selected (e.g. via keyboard navigation).
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl From<NativePopoverClickableRow> for NativePopoverContentItem {
    fn from(builder: NativePopoverClickableRow) -> Self {
        Self::ClickableRow {
            icon: builder.icon,
            text: builder.text,
            detail: builder.detail,
            on_click: builder.on_click,
            enabled: builder.enabled,
            selected: builder.selected,
        }
    }
}

/// Configuration for a native popover (NSPopover).
pub struct NativePopover {
    content_width: f64,
    content_height: f64,
    behavior: NativePopoverBehavior,
    on_close: Option<Box<dyn Fn(&NativePopoverCloseEvent, &mut Window, &mut App) + 'static>>,
    on_show: Option<Box<dyn Fn(&NativePopoverShowEvent, &mut Window, &mut App) + 'static>>,
    content_items: Vec<NativePopoverContentItem>,
}

impl NativePopover {
    /// Creates a new popover configuration with the given content size.
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            content_width: width,
            content_height: height,
            behavior: NativePopoverBehavior::default(),
            on_close: None,
            on_show: None,
            content_items: Vec::new(),
        }
    }

    /// Sets the popover's dismissal behavior.
    pub fn behavior(mut self, behavior: NativePopoverBehavior) -> Self {
        self.behavior = behavior;
        self
    }

    /// Registers a handler called when the popover closes.
    pub fn on_close(
        mut self,
        handler: impl Fn(&NativePopoverCloseEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_close = Some(Box::new(handler));
        self
    }

    /// Registers a handler called when the popover is shown.
    pub fn on_show(
        mut self,
        handler: impl Fn(&NativePopoverShowEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_show = Some(Box::new(handler));
        self
    }

    /// Adds a content item to the popover.
    pub fn item(mut self, item: impl Into<NativePopoverContentItem>) -> Self {
        self.content_items.push(item.into());
        self
    }

    /// Adds multiple content items to the popover.
    pub fn items(
        mut self,
        items: impl IntoIterator<Item = impl Into<NativePopoverContentItem>>,
    ) -> Self {
        self.content_items.extend(items.into_iter().map(|i| i.into()));
        self
    }

    fn into_platform_with_anchor(
        self,
        anchor: NativePopoverAnchor,
        next_frame_callbacks: Rc<RefCell<Vec<FrameCallback>>>,
        invalidator: WindowInvalidator,
    ) -> (PlatformNativePopover, PlatformNativePopoverAnchor) {
        let on_close = self.on_close.map(|handler| -> Box<dyn Fn()> {
            schedule_native_toolbar_callback_no_args(
                Rc::new(handler),
                || NativePopoverCloseEvent,
                next_frame_callbacks.clone(),
                invalidator.clone(),
            )
        });

        let on_show = self.on_show.map(|handler| -> Box<dyn Fn()> {
            schedule_native_toolbar_callback_no_args(
                Rc::new(handler),
                || NativePopoverShowEvent,
                next_frame_callbacks.clone(),
                invalidator.clone(),
            )
        });

        let behavior = match self.behavior {
            NativePopoverBehavior::ApplicationDefined => {
                PlatformNativePopoverBehavior::ApplicationDefined
            }
            NativePopoverBehavior::Transient => PlatformNativePopoverBehavior::Transient,
            NativePopoverBehavior::Semitransient => PlatformNativePopoverBehavior::Semitransient,
        };

        let content_items = convert_popover_content_items(
            self.content_items,
            &next_frame_callbacks,
            &invalidator,
        );

        let platform_anchor = match &anchor {
            NativePopoverAnchor::ToolbarItem(id) => {
                PlatformNativePopoverAnchor::ToolbarItem(id.clone())
            }
        };

        (
            PlatformNativePopover {
                content_width: self.content_width,
                content_height: self.content_height,
                behavior,
                on_close,
                on_show,
                content_items,
            },
            platform_anchor,
        )
    }
}

// =============================================================================
// Native Panel
// =============================================================================

/// Event fired when a native panel closes.
#[derive(Clone, Debug)]
pub struct NativePanelCloseEvent;

/// The visual style of a native panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NativePanelStyle {
    /// Standard titled panel with close button and transparent titlebar.
    #[default]
    Titled,
    /// Borderless panel — no title bar, no chrome. Used for suggestion boxes, launchers, etc.
    Borderless,
    /// HUD panel — dark translucent appearance with utility title bar.
    Hud,
    /// Utility panel — smaller title bar, floats above regular windows.
    Utility,
}

/// The floating level of a native panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NativePanelLevel {
    /// Normal window level (0).
    Normal,
    /// Floats above regular windows (3).
    #[default]
    Floating,
    /// Modal panel level (8).
    ModalPanel,
    /// Popup menu level (101) — above everything except screen saver.
    PopUpMenu,
    /// Custom level value.
    Custom(i64),
}

/// Visual effect material for a panel's blurred background.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativePanelMaterial {
    /// HUD window material.
    HudWindow,
    /// Popover material (default for suggestion panels).
    Popover,
    /// Sidebar material.
    Sidebar,
    /// Under-window background material.
    UnderWindow,
}

/// Where to position a native panel.
#[derive(Clone, Debug)]
pub enum NativePanelAnchor {
    /// Position below a toolbar item, identified by its id.
    ToolbarItem(SharedString),
    /// Position at explicit screen coordinates (top-left).
    Point {
        /// X coordinate in screen pixels.
        x: f64,
        /// Y coordinate in screen pixels.
        y: f64,
    },
    /// Center on screen.
    Centered,
}

/// Configuration for a native panel (NSPanel).
///
/// Panels are floating auxiliary windows used for tool palettes, inspectors,
/// suggestion boxes, and other secondary UI.
pub struct NativePanel {
    width: f64,
    height: f64,
    style: NativePanelStyle,
    level: NativePanelLevel,
    non_activating: bool,
    has_shadow: bool,
    corner_radius: f64,
    material: Option<NativePanelMaterial>,
    on_close: Option<Box<dyn Fn(&NativePanelCloseEvent, &mut Window, &mut App) + 'static>>,
    content_items: Vec<NativePopoverContentItem>,
}

impl NativePanel {
    /// Creates a new panel configuration with the given size.
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            style: NativePanelStyle::default(),
            level: NativePanelLevel::default(),
            non_activating: true,
            has_shadow: true,
            corner_radius: 10.0,
            material: None,
            on_close: None,
            content_items: Vec::new(),
        }
    }

    /// Sets the panel's visual style.
    pub fn style(mut self, style: NativePanelStyle) -> Self {
        self.style = style;
        self
    }

    /// Sets the panel's floating level.
    pub fn level(mut self, level: NativePanelLevel) -> Self {
        self.level = level;
        self
    }

    /// Sets whether the panel is non-activating (doesn't steal focus from main window).
    pub fn non_activating(mut self, non_activating: bool) -> Self {
        self.non_activating = non_activating;
        self
    }

    /// Sets whether the panel has a shadow.
    pub fn has_shadow(mut self, has_shadow: bool) -> Self {
        self.has_shadow = has_shadow;
        self
    }

    /// Sets the corner radius for the panel.
    pub fn corner_radius(mut self, radius: f64) -> Self {
        self.corner_radius = radius;
        self
    }

    /// Sets the visual effect material for the panel's background.
    pub fn material(mut self, material: NativePanelMaterial) -> Self {
        self.material = Some(material);
        self
    }

    /// Registers a handler called when the panel closes.
    pub fn on_close(
        mut self,
        handler: impl Fn(&NativePanelCloseEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_close = Some(Box::new(handler));
        self
    }

    /// Adds a content item to the panel.
    pub fn item(mut self, item: impl Into<NativePopoverContentItem>) -> Self {
        self.content_items.push(item.into());
        self
    }

    /// Adds multiple content items to the panel.
    pub fn items(
        mut self,
        items: impl IntoIterator<Item = impl Into<NativePopoverContentItem>>,
    ) -> Self {
        self.content_items.extend(items.into_iter().map(|i| i.into()));
        self
    }

    fn into_platform_with_anchor(
        self,
        anchor: NativePanelAnchor,
        next_frame_callbacks: Rc<RefCell<Vec<FrameCallback>>>,
        invalidator: WindowInvalidator,
    ) -> (PlatformNativePanel, PlatformNativePanelAnchor) {
        let on_close = self.on_close.map(|handler| -> Box<dyn Fn()> {
            schedule_native_toolbar_callback_no_args(
                Rc::new(handler),
                || NativePanelCloseEvent,
                next_frame_callbacks.clone(),
                invalidator.clone(),
            )
        });

        let style = match self.style {
            NativePanelStyle::Titled => PlatformNativePanelStyle::Titled,
            NativePanelStyle::Borderless => PlatformNativePanelStyle::Borderless,
            NativePanelStyle::Hud => PlatformNativePanelStyle::Hud,
            NativePanelStyle::Utility => PlatformNativePanelStyle::Utility,
        };

        let level = match self.level {
            NativePanelLevel::Normal => PlatformNativePanelLevel::Normal,
            NativePanelLevel::Floating => PlatformNativePanelLevel::Floating,
            NativePanelLevel::ModalPanel => PlatformNativePanelLevel::ModalPanel,
            NativePanelLevel::PopUpMenu => PlatformNativePanelLevel::PopUpMenu,
            NativePanelLevel::Custom(v) => PlatformNativePanelLevel::Custom(v),
        };

        let material = self.material.map(|m| match m {
            NativePanelMaterial::HudWindow => PlatformNativePanelMaterial::HudWindow,
            NativePanelMaterial::Popover => PlatformNativePanelMaterial::Popover,
            NativePanelMaterial::Sidebar => PlatformNativePanelMaterial::Sidebar,
            NativePanelMaterial::UnderWindow => PlatformNativePanelMaterial::UnderWindow,
        });

        let content_items = convert_popover_content_items(
            self.content_items,
            &next_frame_callbacks,
            &invalidator,
        );

        let platform_anchor = match anchor {
            NativePanelAnchor::ToolbarItem(id) => PlatformNativePanelAnchor::ToolbarItem(id),
            NativePanelAnchor::Point { x, y } => PlatformNativePanelAnchor::Point { x, y },
            NativePanelAnchor::Centered => PlatformNativePanelAnchor::Centered,
        };

        (
            PlatformNativePanel {
                width: self.width,
                height: self.height,
                style,
                level,
                non_activating: self.non_activating,
                has_shadow: self.has_shadow,
                corner_radius: self.corner_radius,
                material,
                on_close,
                content_items,
            },
            platform_anchor,
        )
    }
}

// =============================================================================
// Native Alert
// =============================================================================

/// The style of a native alert dialog.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NativeAlertStyle {
    /// Warning alert (default) — caution icon.
    #[default]
    Warning,
    /// Informational alert — info icon.
    Informational,
    /// Critical alert — error/stop icon.
    Critical,
}

/// The response from a native alert dialog.
#[derive(Clone, Debug)]
pub struct NativeAlertResponse {
    /// The index of the button that was clicked (0 = first button added).
    pub button_index: usize,
    /// Whether the suppression checkbox was checked (if shown).
    pub suppression_checked: bool,
}

/// Configuration for a native alert dialog (NSAlert).
pub struct NativeAlert {
    style: NativeAlertStyle,
    message: SharedString,
    informative_text: Option<SharedString>,
    button_titles: Vec<SharedString>,
    shows_suppression_button: bool,
}

impl NativeAlert {
    /// Creates a new alert with the given message.
    pub fn new(message: impl Into<SharedString>) -> Self {
        Self {
            style: NativeAlertStyle::default(),
            message: message.into(),
            informative_text: None,
            button_titles: Vec::new(),
            shows_suppression_button: false,
        }
    }

    /// Sets the alert's visual style.
    pub fn style(mut self, style: NativeAlertStyle) -> Self {
        self.style = style;
        self
    }

    /// Sets the informative text displayed below the main message.
    pub fn informative_text(mut self, text: impl Into<SharedString>) -> Self {
        self.informative_text = Some(text.into());
        self
    }

    /// Adds a button to the alert. The first button added is the default (rightmost).
    pub fn button(mut self, title: impl Into<SharedString>) -> Self {
        self.button_titles.push(title.into());
        self
    }

    /// Enables the "Don't show again" suppression checkbox.
    pub fn shows_suppression_button(mut self, shows: bool) -> Self {
        self.shows_suppression_button = shows;
        self
    }

    fn into_platform(self) -> PlatformNativeAlert {
        let style = match self.style {
            NativeAlertStyle::Warning => PlatformNativeAlertStyle::Warning,
            NativeAlertStyle::Informational => PlatformNativeAlertStyle::Informational,
            NativeAlertStyle::Critical => PlatformNativeAlertStyle::Critical,
        };

        PlatformNativeAlert {
            style,
            message: self.message,
            informative_text: self.informative_text,
            button_titles: self.button_titles,
            shows_suppression_button: self.shows_suppression_button,
        }
    }
}

type FrameCallback = Box<dyn FnOnce(&mut Window, &mut App)>;

pub(crate) type AnyMouseListener =
    Box<dyn FnMut(&dyn Any, DispatchPhase, &mut Window, &mut App) + 'static>;

#[derive(Clone)]
pub(crate) struct CursorStyleRequest {
    pub(crate) hitbox_id: Option<HitboxId>,
    pub(crate) style: CursorStyle,
}

#[derive(Clone, Default, Eq, PartialEq)]
pub(crate) struct HitTest {
    pub(crate) ids: SmallVec<[HitboxId; 8]>,
    pub(crate) hover_hitbox_count: usize,
}

/// A type of window control area that corresponds to the platform window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowControlArea {
    /// An area that allows dragging of the platform window.
    Drag,
    /// An area that allows closing of the platform window.
    Close,
    /// An area that allows maximizing of the platform window.
    Max,
    /// An area that allows minimizing of the platform window.
    Min,
}

/// An identifier for a [Hitbox] which also includes [HitboxBehavior].
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct HitboxId(u64);

impl HitboxId {
    /// Checks if the hitbox with this ID is currently hovered. Except when handling
    /// `ScrollWheelEvent`, this is typically what you want when determining whether to handle mouse
    /// events or paint hover styles.
    ///
    /// See [`Hitbox::is_hovered`] for details.
    pub fn is_hovered(self, window: &Window) -> bool {
        let hit_test = &window.mouse_hit_test;
        for id in hit_test.ids.iter().take(hit_test.hover_hitbox_count) {
            if self == *id {
                return true;
            }
        }
        false
    }

    /// Checks if the hitbox with this ID contains the mouse and should handle scroll events.
    /// Typically this should only be used when handling `ScrollWheelEvent`, and otherwise
    /// `is_hovered` should be used. See the documentation of `Hitbox::is_hovered` for details about
    /// this distinction.
    pub fn should_handle_scroll(self, window: &Window) -> bool {
        window.mouse_hit_test.ids.contains(&self)
    }

    fn next(mut self) -> HitboxId {
        HitboxId(self.0.wrapping_add(1))
    }
}

/// A rectangular region that potentially blocks hitboxes inserted prior.
/// See [Window::insert_hitbox] for more details.
#[derive(Clone, Debug, Deref)]
pub struct Hitbox {
    /// A unique identifier for the hitbox.
    pub id: HitboxId,
    /// The bounds of the hitbox.
    #[deref]
    pub bounds: Bounds<Pixels>,
    /// The content mask when the hitbox was inserted.
    pub content_mask: ContentMask<Pixels>,
    /// Flags that specify hitbox behavior.
    pub behavior: HitboxBehavior,
}

impl Hitbox {
    /// Checks if the hitbox is currently hovered. Except when handling `ScrollWheelEvent`, this is
    /// typically what you want when determining whether to handle mouse events or paint hover
    /// styles.
    ///
    /// This can return `false` even when the hitbox contains the mouse, if a hitbox in front of
    /// this sets `HitboxBehavior::BlockMouse` (`InteractiveElement::occlude`) or
    /// `HitboxBehavior::BlockMouseExceptScroll` (`InteractiveElement::block_mouse_except_scroll`).
    ///
    /// Handling of `ScrollWheelEvent` should typically use `should_handle_scroll` instead.
    /// Concretely, this is due to use-cases like overlays that cause the elements under to be
    /// non-interactive while still allowing scrolling. More abstractly, this is because
    /// `is_hovered` is about element interactions directly under the mouse - mouse moves, clicks,
    /// hover styling, etc. In contrast, scrolling is about finding the current outer scrollable
    /// container.
    pub fn is_hovered(&self, window: &Window) -> bool {
        self.id.is_hovered(window)
    }

    /// Checks if the hitbox contains the mouse and should handle scroll events. Typically this
    /// should only be used when handling `ScrollWheelEvent`, and otherwise `is_hovered` should be
    /// used. See the documentation of `Hitbox::is_hovered` for details about this distinction.
    ///
    /// This can return `false` even when the hitbox contains the mouse, if a hitbox in front of
    /// this sets `HitboxBehavior::BlockMouse` (`InteractiveElement::occlude`).
    pub fn should_handle_scroll(&self, window: &Window) -> bool {
        self.id.should_handle_scroll(window)
    }
}

/// How the hitbox affects mouse behavior.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum HitboxBehavior {
    /// Normal hitbox mouse behavior, doesn't affect mouse handling for other hitboxes.
    #[default]
    Normal,

    /// All hitboxes behind this hitbox will be ignored and so will have `hitbox.is_hovered() ==
    /// false` and `hitbox.should_handle_scroll() == false`. Typically for elements this causes
    /// skipping of all mouse events, hover styles, and tooltips. This flag is set by
    /// [`InteractiveElement::occlude`].
    ///
    /// For mouse handlers that check those hitboxes, this behaves the same as registering a
    /// bubble-phase handler for every mouse event type:
    ///
    /// ```ignore
    /// window.on_mouse_event(move |_: &EveryMouseEventTypeHere, phase, window, cx| {
    ///     if phase == DispatchPhase::Capture && hitbox.is_hovered(window) {
    ///         cx.stop_propagation();
    ///     }
    /// })
    /// ```
    ///
    /// This has effects beyond event handling - any use of hitbox checking, such as hover
    /// styles and tooltips. These other behaviors are the main point of this mechanism. An
    /// alternative might be to not affect mouse event handling - but this would allow
    /// inconsistent UI where clicks and moves interact with elements that are not considered to
    /// be hovered.
    BlockMouse,

    /// All hitboxes behind this hitbox will have `hitbox.is_hovered() == false`, even when
    /// `hitbox.should_handle_scroll() == true`. Typically for elements this causes all mouse
    /// interaction except scroll events to be ignored - see the documentation of
    /// [`Hitbox::is_hovered`] for details. This flag is set by
    /// [`InteractiveElement::block_mouse_except_scroll`].
    ///
    /// For mouse handlers that check those hitboxes, this behaves the same as registering a
    /// bubble-phase handler for every mouse event type **except** `ScrollWheelEvent`:
    ///
    /// ```ignore
    /// window.on_mouse_event(move |_: &EveryMouseEventTypeExceptScroll, phase, window, cx| {
    ///     if phase == DispatchPhase::Bubble && hitbox.should_handle_scroll(window) {
    ///         cx.stop_propagation();
    ///     }
    /// })
    /// ```
    ///
    /// See the documentation of [`Hitbox::is_hovered`] for details of why `ScrollWheelEvent` is
    /// handled differently than other mouse events. If also blocking these scroll events is
    /// desired, then a `cx.stop_propagation()` handler like the one above can be used.
    ///
    /// This has effects beyond event handling - this affects any use of `is_hovered`, such as
    /// hover styles and tooltips. These other behaviors are the main point of this mechanism.
    /// An alternative might be to not affect mouse event handling - but this would allow
    /// inconsistent UI where clicks and moves interact with elements that are not considered to
    /// be hovered.
    BlockMouseExceptScroll,
}

/// An identifier for a tooltip.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct TooltipId(usize);

impl TooltipId {
    /// Checks if the tooltip is currently hovered.
    pub fn is_hovered(&self, window: &Window) -> bool {
        window
            .tooltip_bounds
            .as_ref()
            .is_some_and(|tooltip_bounds| {
                tooltip_bounds.id == *self
                    && tooltip_bounds.bounds.contains(&window.mouse_position())
            })
    }
}

pub(crate) struct TooltipBounds {
    id: TooltipId,
    bounds: Bounds<Pixels>,
}

#[derive(Clone)]
pub(crate) struct TooltipRequest {
    id: TooltipId,
    tooltip: AnyTooltip,
}

pub(crate) struct DeferredDraw {
    current_view: EntityId,
    priority: usize,
    parent_node: DispatchNodeId,
    element_id_stack: SmallVec<[ElementId; 32]>,
    text_style_stack: Vec<TextStyleRefinement>,
    rem_size: Pixels,
    element: Option<AnyElement>,
    absolute_offset: Point<Pixels>,
    prepaint_range: Range<PrepaintStateIndex>,
    paint_range: Range<PaintIndex>,
}

pub(crate) struct Frame {
    pub(crate) focus: Option<FocusId>,
    pub(crate) window_active: bool,
    pub(crate) element_states: FxHashMap<(GlobalElementId, TypeId), ElementStateBox>,
    accessed_element_states: Vec<(GlobalElementId, TypeId)>,
    pub(crate) mouse_listeners: Vec<Option<AnyMouseListener>>,
    pub(crate) dispatch_tree: DispatchTree,
    pub(crate) scene: Scene,
    pub(crate) hitboxes: Vec<Hitbox>,
    pub(crate) window_control_hitboxes: Vec<(WindowControlArea, Hitbox)>,
    pub(crate) deferred_draws: Vec<DeferredDraw>,
    pub(crate) input_handlers: Vec<Option<PlatformInputHandler>>,
    pub(crate) tooltip_requests: Vec<Option<TooltipRequest>>,
    pub(crate) cursor_styles: Vec<CursorStyleRequest>,
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) debug_bounds: FxHashMap<String, Bounds<Pixels>>,
    #[cfg(any(feature = "inspector", debug_assertions))]
    pub(crate) next_inspector_instance_ids: FxHashMap<Rc<crate::InspectorElementPath>, usize>,
    #[cfg(any(feature = "inspector", debug_assertions))]
    pub(crate) inspector_hitboxes: FxHashMap<HitboxId, crate::InspectorElementId>,
    pub(crate) tab_stops: TabStopMap,
}

#[derive(Clone, Default)]
pub(crate) struct PrepaintStateIndex {
    hitboxes_index: usize,
    tooltips_index: usize,
    deferred_draws_index: usize,
    dispatch_tree_index: usize,
    accessed_element_states_index: usize,
    line_layout_index: LineLayoutIndex,
}

#[derive(Clone, Default)]
pub(crate) struct PaintIndex {
    scene_index: usize,
    mouse_listeners_index: usize,
    input_handlers_index: usize,
    cursor_styles_index: usize,
    accessed_element_states_index: usize,
    tab_handle_index: usize,
    line_layout_index: LineLayoutIndex,
}

impl Frame {
    pub(crate) fn new(dispatch_tree: DispatchTree) -> Self {
        Frame {
            focus: None,
            window_active: false,
            element_states: FxHashMap::default(),
            accessed_element_states: Vec::new(),
            mouse_listeners: Vec::new(),
            dispatch_tree,
            scene: Scene::default(),
            hitboxes: Vec::new(),
            window_control_hitboxes: Vec::new(),
            deferred_draws: Vec::new(),
            input_handlers: Vec::new(),
            tooltip_requests: Vec::new(),
            cursor_styles: Vec::new(),

            #[cfg(any(test, feature = "test-support"))]
            debug_bounds: FxHashMap::default(),

            #[cfg(any(feature = "inspector", debug_assertions))]
            next_inspector_instance_ids: FxHashMap::default(),

            #[cfg(any(feature = "inspector", debug_assertions))]
            inspector_hitboxes: FxHashMap::default(),
            tab_stops: TabStopMap::default(),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.element_states.clear();
        self.accessed_element_states.clear();
        self.mouse_listeners.clear();
        self.dispatch_tree.clear();
        self.scene.clear();
        self.input_handlers.clear();
        self.tooltip_requests.clear();
        self.cursor_styles.clear();
        self.hitboxes.clear();
        self.window_control_hitboxes.clear();
        self.deferred_draws.clear();
        self.tab_stops.clear();
        self.focus = None;

        #[cfg(any(feature = "inspector", debug_assertions))]
        {
            self.next_inspector_instance_ids.clear();
            self.inspector_hitboxes.clear();
        }
    }

    pub(crate) fn cursor_style(&self, window: &Window) -> Option<CursorStyle> {
        self.cursor_styles
            .iter()
            .rev()
            .fold_while(None, |style, request| match request.hitbox_id {
                None => Done(Some(request.style)),
                Some(hitbox_id) => Continue(
                    style.or_else(|| hitbox_id.is_hovered(window).then_some(request.style)),
                ),
            })
            .into_inner()
    }

    pub(crate) fn hit_test(&self, position: Point<Pixels>) -> HitTest {
        let mut set_hover_hitbox_count = false;
        let mut hit_test = HitTest::default();
        for hitbox in self.hitboxes.iter().rev() {
            let bounds = hitbox.bounds.intersect(&hitbox.content_mask.bounds);
            if bounds.contains(&position) {
                hit_test.ids.push(hitbox.id);
                if !set_hover_hitbox_count
                    && hitbox.behavior == HitboxBehavior::BlockMouseExceptScroll
                {
                    hit_test.hover_hitbox_count = hit_test.ids.len();
                    set_hover_hitbox_count = true;
                }
                if hitbox.behavior == HitboxBehavior::BlockMouse {
                    break;
                }
            }
        }
        if !set_hover_hitbox_count {
            hit_test.hover_hitbox_count = hit_test.ids.len();
        }
        hit_test
    }

    pub(crate) fn focus_path(&self) -> SmallVec<[FocusId; 8]> {
        self.focus
            .map(|focus_id| self.dispatch_tree.focus_path(focus_id))
            .unwrap_or_default()
    }

    pub(crate) fn finish(&mut self, prev_frame: &mut Self) {
        for element_state_key in &self.accessed_element_states {
            if let Some((element_state_key, element_state)) =
                prev_frame.element_states.remove_entry(element_state_key)
            {
                self.element_states.insert(element_state_key, element_state);
            }
        }

        self.scene.finish();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
enum InputModality {
    Mouse,
    Keyboard,
}

/// Holds the state for a specific window.
pub struct Window {
    pub(crate) handle: AnyWindowHandle,
    pub(crate) invalidator: WindowInvalidator,
    pub(crate) removed: bool,
    pub(crate) platform_window: Box<dyn PlatformWindow>,
    display_id: Option<DisplayId>,
    sprite_atlas: Arc<dyn PlatformAtlas>,
    text_system: Arc<WindowTextSystem>,
    text_rendering_mode: Rc<Cell<TextRenderingMode>>,
    rem_size: Pixels,
    /// The stack of override values for the window's rem size.
    ///
    /// This is used by `with_rem_size` to allow rendering an element tree with
    /// a given rem size.
    rem_size_override_stack: SmallVec<[Pixels; 8]>,
    pub(crate) viewport_size: Size<Pixels>,
    layout_engine: Option<TaffyLayoutEngine>,
    pub(crate) root: Option<AnyView>,
    pub(crate) element_id_stack: SmallVec<[ElementId; 32]>,
    pub(crate) text_style_stack: Vec<TextStyleRefinement>,
    pub(crate) rendered_entity_stack: Vec<EntityId>,
    pub(crate) element_offset_stack: Vec<Point<Pixels>>,
    pub(crate) element_opacity: f32,
    pub(crate) content_mask_stack: Vec<ContentMask<Pixels>>,
    pub(crate) requested_autoscroll: Option<Bounds<Pixels>>,
    pub(crate) image_cache_stack: Vec<AnyImageCache>,
    pub(crate) rendered_frame: Frame,
    pub(crate) next_frame: Frame,
    next_hitbox_id: HitboxId,
    pub(crate) next_tooltip_id: TooltipId,
    pub(crate) tooltip_bounds: Option<TooltipBounds>,
    pub(crate) next_frame_callbacks: Rc<RefCell<Vec<FrameCallback>>>,
    pub(crate) dirty_views: FxHashSet<EntityId>,
    focus_listeners: SubscriberSet<(), AnyWindowFocusListener>,
    pub(crate) focus_lost_listeners: SubscriberSet<(), AnyObserver>,
    default_prevented: bool,
    mouse_position: Point<Pixels>,
    mouse_hit_test: HitTest,
    modifiers: Modifiers,
    capslock: Capslock,
    scale_factor: f32,
    pub(crate) bounds_observers: SubscriberSet<(), AnyObserver>,
    appearance: WindowAppearance,
    pub(crate) appearance_observers: SubscriberSet<(), AnyObserver>,
    active: Rc<Cell<bool>>,
    hovered: Rc<Cell<bool>>,
    pub(crate) needs_present: Rc<Cell<bool>>,
    /// Tracks recent input event timestamps to determine if input is arriving at a high rate.
    /// Used to selectively enable VRR optimization only when input rate exceeds 60fps.
    pub(crate) input_rate_tracker: Rc<RefCell<InputRateTracker>>,
    last_input_modality: InputModality,
    pub(crate) refreshing: bool,
    pub(crate) activation_observers: SubscriberSet<(), AnyObserver>,
    pub(crate) focus: Option<FocusId>,
    focus_enabled: bool,
    pending_input: Option<PendingInput>,
    pending_modifier: ModifierState,
    pub(crate) pending_input_observers: SubscriberSet<(), AnyObserver>,
    prompt: Option<RenderablePromptHandle>,
    pub(crate) client_inset: Option<Pixels>,
    #[cfg(any(feature = "inspector", debug_assertions))]
    inspector: Option<Entity<Inspector>>,
    /// Stack of native view pointers for surface rendering. When a surface is being
    /// painted, its NSView is pushed here so that `raw_native_view_ptr()` returns
    /// the surface's view instead of the main window's view. This makes all existing
    /// native controls (NativeButton, NativeToggleGroup, etc.) work inside surfaces
    /// with zero changes.
    native_view_override_stack: Vec<*mut std::ffi::c_void>,
    /// Secondary GPUI rendering surfaces, each with their own Metal layer and element tree.
    #[cfg(target_os = "macos")]
    pub(crate) surfaces: FxHashMap<SurfaceId, SurfaceState>,
}

#[derive(Clone, Debug, Default)]
struct ModifierState {
    modifiers: Modifiers,
    saw_keystroke: bool,
}

/// Tracks input event timestamps to determine if input is arriving at a high rate.
/// Used for selective VRR (Variable Refresh Rate) optimization.
#[derive(Clone, Debug)]
pub(crate) struct InputRateTracker {
    timestamps: Vec<Instant>,
    window: Duration,
    inputs_per_second: u32,
    sustain_until: Instant,
    sustain_duration: Duration,
}

impl Default for InputRateTracker {
    fn default() -> Self {
        Self {
            timestamps: Vec::new(),
            window: Duration::from_millis(100),
            inputs_per_second: 60,
            sustain_until: Instant::now(),
            sustain_duration: Duration::from_secs(1),
        }
    }
}

impl InputRateTracker {
    pub fn record_input(&mut self) {
        let now = Instant::now();
        self.timestamps.push(now);
        self.prune_old_timestamps(now);

        let min_events = self.inputs_per_second as u128 * self.window.as_millis() / 1000;
        if self.timestamps.len() as u128 >= min_events {
            self.sustain_until = now + self.sustain_duration;
        }
    }

    pub fn is_high_rate(&self) -> bool {
        Instant::now() < self.sustain_until
    }

    fn prune_old_timestamps(&mut self, now: Instant) {
        self.timestamps
            .retain(|&t| now.duration_since(t) <= self.window);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DrawPhase {
    None,
    Prepaint,
    Paint,
    Focus,
}

#[derive(Default, Debug)]
struct PendingInput {
    keystrokes: SmallVec<[Keystroke; 1]>,
    focus: Option<FocusId>,
    timer: Option<Task<()>>,
    needs_timeout: bool,
}

pub(crate) struct ElementStateBox {
    pub(crate) inner: Box<dyn Any>,
    #[cfg(debug_assertions)]
    pub(crate) type_name: &'static str,
}

fn default_bounds(display_id: Option<DisplayId>, cx: &mut App) -> WindowBounds {
    // TODO, BUG: if you open a window with the currently active window
    // on the stack, this will erroneously fallback to `None`
    //
    // TODO these should be the initial window bounds not considering maximized/fullscreen
    let active_window_bounds = cx
        .active_window()
        .and_then(|w| w.update(cx, |_, window, _| window.window_bounds()).ok());

    const CASCADE_OFFSET: f32 = 25.0;

    let display = display_id
        .map(|id| cx.find_display(id))
        .unwrap_or_else(|| cx.primary_display());

    let default_placement = || Bounds::new(point(px(0.), px(0.)), DEFAULT_WINDOW_SIZE);

    // Use visible_bounds to exclude taskbar/dock areas
    let display_bounds = display
        .as_ref()
        .map(|d| d.visible_bounds())
        .unwrap_or_else(default_placement);

    let (
        Bounds {
            origin: base_origin,
            size: base_size,
        },
        window_bounds_ctor,
    ): (_, fn(Bounds<Pixels>) -> WindowBounds) = match active_window_bounds {
        Some(bounds) => match bounds {
            WindowBounds::Windowed(bounds) => (bounds, WindowBounds::Windowed),
            WindowBounds::Maximized(bounds) => (bounds, WindowBounds::Maximized),
            WindowBounds::Fullscreen(bounds) => (bounds, WindowBounds::Fullscreen),
        },
        None => (
            display
                .as_ref()
                .map(|d| d.default_bounds())
                .unwrap_or_else(default_placement),
            WindowBounds::Windowed,
        ),
    };

    let cascade_offset = point(px(CASCADE_OFFSET), px(CASCADE_OFFSET));
    let proposed_origin = base_origin + cascade_offset;
    let proposed_bounds = Bounds::new(proposed_origin, base_size);

    let display_right = display_bounds.origin.x + display_bounds.size.width;
    let display_bottom = display_bounds.origin.y + display_bounds.size.height;
    let window_right = proposed_bounds.origin.x + proposed_bounds.size.width;
    let window_bottom = proposed_bounds.origin.y + proposed_bounds.size.height;

    let fits_horizontally = window_right <= display_right;
    let fits_vertically = window_bottom <= display_bottom;

    let final_origin = match (fits_horizontally, fits_vertically) {
        (true, true) => proposed_origin,
        (false, true) => point(display_bounds.origin.x, base_origin.y),
        (true, false) => point(base_origin.x, display_bounds.origin.y),
        (false, false) => display_bounds.origin,
    };
    window_bounds_ctor(Bounds::new(final_origin, base_size))
}

impl Window {
    pub(crate) fn new(
        handle: AnyWindowHandle,
        options: WindowOptions,
        cx: &mut App,
    ) -> Result<Self> {
        let WindowOptions {
            window_bounds,
            titlebar,
            focus,
            show,
            kind,
            is_movable,
            is_resizable,
            is_minimizable,
            display_id,
            window_background,
            app_id,
            window_min_size,
            window_decorations,
            #[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
            tabbing_identifier,
        } = options;

        let window_bounds = window_bounds.unwrap_or_else(|| default_bounds(display_id, cx));
        let mut platform_window = cx.platform.open_window(
            handle,
            WindowParams {
                bounds: window_bounds.get_bounds(),
                titlebar,
                kind,
                is_movable,
                is_resizable,
                is_minimizable,
                focus,
                show,
                display_id,
                window_min_size,
                #[cfg(target_os = "macos")]
                tabbing_identifier,
            },
        )?;

        let tab_bar_visible = platform_window.tab_bar_visible();
        SystemWindowTabController::init_visible(cx, tab_bar_visible);
        if let Some(tabs) = platform_window.tabbed_windows() {
            SystemWindowTabController::add_tab(cx, handle.window_id(), tabs);
        }

        let display_id = platform_window.display().map(|display| display.id());
        let sprite_atlas = platform_window.sprite_atlas();
        let mouse_position = platform_window.mouse_position();
        let modifiers = platform_window.modifiers();
        let capslock = platform_window.capslock();
        let content_size = platform_window.content_size();
        let scale_factor = platform_window.scale_factor();
        let appearance = platform_window.appearance();
        let text_system = Arc::new(WindowTextSystem::new(cx.text_system().clone()));
        let invalidator = WindowInvalidator::new();
        let active = Rc::new(Cell::new(platform_window.is_active()));
        let hovered = Rc::new(Cell::new(platform_window.is_hovered()));
        let needs_present = Rc::new(Cell::new(false));
        let next_frame_callbacks: Rc<RefCell<Vec<FrameCallback>>> = Default::default();
        let input_rate_tracker = Rc::new(RefCell::new(InputRateTracker::default()));
        let last_frame_time = Rc::new(Cell::new(None));

        platform_window
            .request_decorations(window_decorations.unwrap_or(WindowDecorations::Server));
        platform_window.set_background_appearance(window_background);

        match window_bounds {
            WindowBounds::Fullscreen(_) => platform_window.toggle_fullscreen(),
            WindowBounds::Maximized(_) => platform_window.zoom(),
            WindowBounds::Windowed(_) => {}
        }

        platform_window.on_close(Box::new({
            let window_id = handle.window_id();
            let mut cx = cx.to_async();
            move || {
                let _ = handle.update(&mut cx, |_, window, _| window.remove_window());
                let _ = cx.update(|cx| {
                    SystemWindowTabController::remove_tab(cx, window_id);
                });
            }
        }));
        platform_window.on_request_frame(Box::new({
            let mut cx = cx.to_async();
            let invalidator = invalidator.clone();
            let active = active.clone();
            let needs_present = needs_present.clone();
            let next_frame_callbacks = next_frame_callbacks.clone();
            let input_rate_tracker = input_rate_tracker.clone();
            move |request_frame_options| {
                let thermal_state = cx.update(|cx| cx.thermal_state());

                if thermal_state == ThermalState::Serious || thermal_state == ThermalState::Critical
                {
                    let now = Instant::now();
                    let last_frame_time = last_frame_time.replace(Some(now));

                    if let Some(last_frame) = last_frame_time
                        && now.duration_since(last_frame) < Duration::from_micros(16667)
                    {
                        return;
                    }
                }

                let next_frame_callbacks = next_frame_callbacks.take();
                if !next_frame_callbacks.is_empty() {
                    handle
                        .update(&mut cx, |_, window, cx| {
                            for callback in next_frame_callbacks {
                                callback(window, cx);
                            }
                        })
                        .log_err();
                }

                // Keep presenting if input was recently arriving at a high rate (>= 60fps).
                // Once high-rate input is detected, we sustain presentation for 1 second
                // to prevent display underclocking during active input.
                let needs_present = request_frame_options.require_presentation
                    || needs_present.get()
                    || (active.get() && input_rate_tracker.borrow_mut().is_high_rate());

                if invalidator.is_dirty() || request_frame_options.force_render {
                    measure("frame duration", || {
                        handle
                            .update(&mut cx, |_, window, cx| {
                                let arena_clear_needed = window.draw(cx);
                                window.present();
                                arena_clear_needed.clear();
                            })
                            .log_err();
                    })
                } else if needs_present {
                    handle
                        .update(&mut cx, |_, window, _| window.present())
                        .log_err();
                }

                handle
                    .update(&mut cx, |_, window, _| {
                        window.complete_frame();
                    })
                    .log_err();
            }
        }));
        platform_window.on_resize(Box::new({
            let mut cx = cx.to_async();
            let invalidator = invalidator.clone();
            move |_, _| {
                if handle
                    .update(&mut cx, |_, window, cx| window.bounds_changed(cx))
                    .is_err()
                {
                    // Window is already borrowed (e.g. native controls triggering
                    // macOS layout during paint). Mark dirty so the next frame
                    // picks up the bounds change.
                    invalidator.set_dirty(true);
                }
            }
        }));
        platform_window.on_moved(Box::new({
            let mut cx = cx.to_async();
            let invalidator = invalidator.clone();
            move || {
                if handle
                    .update(&mut cx, |_, window, cx| window.bounds_changed(cx))
                    .is_err()
                {
                    invalidator.set_dirty(true);
                }
            }
        }));
        platform_window.on_appearance_changed(Box::new({
            let mut cx = cx.to_async();
            move || {
                handle
                    .update(&mut cx, |_, window, cx| window.appearance_changed(cx))
                    .log_err();
            }
        }));
        platform_window.on_active_status_change(Box::new({
            let mut cx = cx.to_async();
            move |active| {
                handle
                    .update(&mut cx, |_, window, cx| {
                        window.active.set(active);
                        window.modifiers = window.platform_window.modifiers();
                        window.capslock = window.platform_window.capslock();
                        window
                            .activation_observers
                            .clone()
                            .retain(&(), |callback| callback(window, cx));

                        window.bounds_changed(cx);
                        window.refresh();

                        SystemWindowTabController::update_last_active(cx, window.handle.id);
                    })
                    .log_err();
            }
        }));
        platform_window.on_hover_status_change(Box::new({
            let mut cx = cx.to_async();
            move |active| {
                handle
                    .update(&mut cx, |_, window, _| {
                        window.hovered.set(active);
                        window.refresh();
                    })
                    .log_err();
            }
        }));
        platform_window.on_input({
            let mut cx = cx.to_async();
            Box::new(move |event| {
                handle
                    .update(&mut cx, |_, window, cx| window.dispatch_event(event, cx))
                    .log_err()
                    .unwrap_or(DispatchEventResult::default())
            })
        });
        #[cfg(target_os = "macos")]
        platform_window.on_surface_input({
            let handle = handle.clone();
            let mut cx = cx.to_async();
            Box::new(move |native_view_ptr, event| {
                handle
                    .update(&mut cx, |_, window, cx| {
                        window.dispatch_surface_event(native_view_ptr, event, cx)
                    })
                    .log_err();
                DispatchEventResult::default()
            })
        });
        platform_window.on_hit_test_window_control({
            let mut cx = cx.to_async();
            Box::new(move || {
                handle
                    .update(&mut cx, |_, window, _cx| {
                        for (area, hitbox) in &window.rendered_frame.window_control_hitboxes {
                            if window.mouse_hit_test.ids.contains(&hitbox.id) {
                                return Some(*area);
                            }
                        }
                        None
                    })
                    .log_err()
                    .unwrap_or(None)
            })
        });
        platform_window.on_move_tab_to_new_window({
            let mut cx = cx.to_async();
            Box::new(move || {
                handle
                    .update(&mut cx, |_, _window, cx| {
                        SystemWindowTabController::move_tab_to_new_window(cx, handle.window_id());
                    })
                    .log_err();
            })
        });
        platform_window.on_merge_all_windows({
            let mut cx = cx.to_async();
            Box::new(move || {
                handle
                    .update(&mut cx, |_, _window, cx| {
                        SystemWindowTabController::merge_all_windows(cx, handle.window_id());
                    })
                    .log_err();
            })
        });
        platform_window.on_select_next_tab({
            let mut cx = cx.to_async();
            Box::new(move || {
                handle
                    .update(&mut cx, |_, _window, cx| {
                        SystemWindowTabController::select_next_tab(cx, handle.window_id());
                    })
                    .log_err();
            })
        });
        platform_window.on_select_previous_tab({
            let mut cx = cx.to_async();
            Box::new(move || {
                handle
                    .update(&mut cx, |_, _window, cx| {
                        SystemWindowTabController::select_previous_tab(cx, handle.window_id())
                    })
                    .log_err();
            })
        });
        platform_window.on_toggle_tab_bar({
            let mut cx = cx.to_async();
            Box::new(move || {
                handle
                    .update(&mut cx, |_, window, cx| {
                        let tab_bar_visible = window.platform_window.tab_bar_visible();
                        SystemWindowTabController::set_visible(cx, tab_bar_visible);
                    })
                    .log_err();
            })
        });

        if let Some(app_id) = app_id {
            platform_window.set_app_id(&app_id);
        }

        platform_window.map_window().unwrap();

        Ok(Window {
            handle,
            invalidator,
            removed: false,
            platform_window,
            display_id,
            sprite_atlas,
            text_system,
            text_rendering_mode: cx.text_rendering_mode.clone(),
            rem_size: px(16.),
            rem_size_override_stack: SmallVec::new(),
            viewport_size: content_size,
            layout_engine: Some(TaffyLayoutEngine::new()),
            root: None,
            element_id_stack: SmallVec::default(),
            text_style_stack: Vec::new(),
            rendered_entity_stack: Vec::new(),
            element_offset_stack: Vec::new(),
            content_mask_stack: Vec::new(),
            element_opacity: 1.0,
            requested_autoscroll: None,
            rendered_frame: Frame::new(DispatchTree::new(cx.keymap.clone(), cx.actions.clone())),
            next_frame: Frame::new(DispatchTree::new(cx.keymap.clone(), cx.actions.clone())),
            next_frame_callbacks,
            next_hitbox_id: HitboxId(0),
            next_tooltip_id: TooltipId::default(),
            tooltip_bounds: None,
            dirty_views: FxHashSet::default(),
            focus_listeners: SubscriberSet::new(),
            focus_lost_listeners: SubscriberSet::new(),
            default_prevented: true,
            mouse_position,
            mouse_hit_test: HitTest::default(),
            modifiers,
            capslock,
            scale_factor,
            bounds_observers: SubscriberSet::new(),
            appearance,
            appearance_observers: SubscriberSet::new(),
            active,
            hovered,
            needs_present,
            input_rate_tracker,
            last_input_modality: InputModality::Mouse,
            refreshing: false,
            activation_observers: SubscriberSet::new(),
            focus: None,
            focus_enabled: true,
            pending_input: None,
            pending_modifier: ModifierState::default(),
            pending_input_observers: SubscriberSet::new(),
            prompt: None,
            client_inset: None,
            image_cache_stack: Vec::new(),
            #[cfg(any(feature = "inspector", debug_assertions))]
            inspector: None,
            native_view_override_stack: Vec::new(),
            #[cfg(target_os = "macos")]
            surfaces: FxHashMap::default(),
        })
    }

    pub(crate) fn new_focus_listener(
        &self,
        value: AnyWindowFocusListener,
    ) -> (Subscription, impl FnOnce() + use<>) {
        self.focus_listeners.insert((), value)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DispatchEventResult {
    pub propagate: bool,
    pub default_prevented: bool,
}

/// Indicates which region of the window is visible. Content falling outside of this mask will not be
/// rendered. Supports both rectangular and rounded-rectangle clip shapes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct ContentMask<P: Clone + Debug + Default + PartialEq> {
    /// The bounds used for hardware (rectangular) clipping.
    pub bounds: Bounds<P>,
    /// Corner radii for rounded-rectangle clipping. All zeros means rectangular clipping.
    pub corner_radii: Corners<P>,
    /// The original bounds of the element that introduced the rounded clip shape.
    /// Used as the reference rectangle for the SDF so that child elements are always
    /// clipped against the card's outline, not their own (potentially smaller) bounds.
    pub corner_radii_bounds: Bounds<P>,
}

impl ContentMask<Pixels> {
    /// Scale the content mask's pixel units by the given scaling factor.
    pub fn scale(&self, factor: f32) -> ContentMask<ScaledPixels> {
        ContentMask {
            bounds: self.bounds.scale(factor),
            corner_radii: self.corner_radii.scale(factor),
            corner_radii_bounds: self.corner_radii_bounds.scale(factor),
        }
    }

    /// Intersect the content mask with the given content mask.
    ///
    /// Only one set of corner radii can be active at a time because the GPU
    /// shader evaluates a single SDF per fragment. When both masks are rounded
    /// the outer mask wins — this is intentional: child elements (text, icons)
    /// should clip against their parent card's outline, not their own bounds.
    pub fn intersect(&self, other: &Self) -> Self {
        let bounds = self.bounds.intersect(&other.bounds);
        let (corner_radii, corner_radii_bounds) = if self.corner_radii != Corners::default() {
            (self.corner_radii.clone(), self.corner_radii_bounds)
        } else {
            (other.corner_radii.clone(), other.corner_radii_bounds)
        };
        ContentMask { bounds, corner_radii, corner_radii_bounds }
    }
}

impl Window {
    fn mark_view_dirty(&mut self, view_id: EntityId) {
        // Mark ancestor views as dirty. If already in the `dirty_views` set, then all its ancestors
        // should already be dirty.
        for view_id in self
            .rendered_frame
            .dispatch_tree
            .view_path_reversed(view_id)
        {
            if !self.dirty_views.insert(view_id) {
                break;
            }
        }
    }

    /// Registers a callback to be invoked when the window appearance changes.
    pub fn observe_window_appearance(
        &self,
        mut callback: impl FnMut(&mut Window, &mut App) + 'static,
    ) -> Subscription {
        let (subscription, activate) = self.appearance_observers.insert(
            (),
            Box::new(move |window, cx| {
                callback(window, cx);
                true
            }),
        );
        activate();
        subscription
    }

    /// Replaces the root entity of the window with a new one.
    pub fn replace_root<E>(
        &mut self,
        cx: &mut App,
        build_view: impl FnOnce(&mut Window, &mut Context<E>) -> E,
    ) -> Entity<E>
    where
        E: 'static + Render,
    {
        let view = cx.new(|cx| build_view(self, cx));
        self.root = Some(view.clone().into());
        self.refresh();
        view
    }

    /// Returns the root entity of the window, if it has one.
    pub fn root<E>(&self) -> Option<Option<Entity<E>>>
    where
        E: 'static + Render,
    {
        self.root
            .as_ref()
            .map(|view| view.clone().downcast::<E>().ok())
    }

    /// Obtain a handle to the window that belongs to this context.
    pub fn window_handle(&self) -> AnyWindowHandle {
        self.handle
    }

    /// Mark the window as dirty, scheduling it to be redrawn on the next frame.
    pub fn refresh(&mut self) {
        if self.invalidator.not_drawing() {
            self.refreshing = true;
            self.invalidator.set_dirty(true);
        }
    }

    /// Close this window.
    pub fn remove_window(&mut self) {
        self.removed = true;
    }

    /// Obtain the currently focused [`FocusHandle`]. If no elements are focused, returns `None`.
    pub fn focused(&self, cx: &App) -> Option<FocusHandle> {
        self.focus
            .and_then(|id| FocusHandle::for_id(id, &cx.focus_handles))
    }

    /// Move focus to the element associated with the given [`FocusHandle`].
    pub fn focus(&mut self, handle: &FocusHandle, cx: &mut App) {
        if !self.focus_enabled || self.focus == Some(handle.id) {
            return;
        }

        self.focus = Some(handle.id);
        self.clear_pending_keystrokes();

        // Avoid re-entrant entity updates by deferring observer notifications to the end of the
        // current effect cycle, and only for this window.
        let window_handle = self.handle;
        cx.defer(move |cx| {
            window_handle
                .update(cx, |_, window, cx| {
                    window.pending_input_changed(cx);
                })
                .ok();
        });

        self.refresh();
    }

    /// Remove focus from all elements within this context's window.
    pub fn blur(&mut self) {
        if !self.focus_enabled {
            return;
        }

        self.focus = None;
        self.refresh();
    }

    /// Blur the window and don't allow anything in it to be focused again.
    pub fn disable_focus(&mut self) {
        self.blur();
        self.focus_enabled = false;
    }

    /// Move focus to next tab stop.
    pub fn focus_next(&mut self, cx: &mut App) {
        if !self.focus_enabled {
            return;
        }

        if let Some(handle) = self.rendered_frame.tab_stops.next(self.focus.as_ref()) {
            self.focus(&handle, cx)
        }
    }

    /// Move focus to previous tab stop.
    pub fn focus_prev(&mut self, cx: &mut App) {
        if !self.focus_enabled {
            return;
        }

        if let Some(handle) = self.rendered_frame.tab_stops.prev(self.focus.as_ref()) {
            self.focus(&handle, cx)
        }
    }

    /// Accessor for the text system.
    pub fn text_system(&self) -> &Arc<WindowTextSystem> {
        &self.text_system
    }

    /// The current text style. Which is composed of all the style refinements provided to `with_text_style`.
    pub fn text_style(&self) -> TextStyle {
        let mut style = TextStyle::default();
        for refinement in &self.text_style_stack {
            style.refine(refinement);
        }
        style
    }

    /// Check if the platform window is maximized.
    ///
    /// On some platforms (namely Windows) this is different than the bounds being the size of the display
    pub fn is_maximized(&self) -> bool {
        self.platform_window.is_maximized()
    }

    /// request a certain window decoration (Wayland)
    pub fn request_decorations(&self, decorations: WindowDecorations) {
        self.platform_window.request_decorations(decorations);
    }

    /// Start a window resize operation (Wayland)
    pub fn start_window_resize(&self, edge: ResizeEdge) {
        self.platform_window.start_window_resize(edge);
    }

    /// Return the `WindowBounds` to indicate that how a window should be opened
    /// after it has been closed
    pub fn window_bounds(&self) -> WindowBounds {
        self.platform_window.window_bounds()
    }

    /// Return the `WindowBounds` excluding insets (Wayland and X11)
    pub fn inner_window_bounds(&self) -> WindowBounds {
        self.platform_window.inner_window_bounds()
    }

    /// Dispatch the given action on the currently focused element.
    pub fn dispatch_action(&mut self, action: Box<dyn Action>, cx: &mut App) {
        let focus_id = self.focused(cx).map(|handle| handle.id);

        let window = self.handle;
        cx.defer(move |cx| {
            window
                .update(cx, |_, window, cx| {
                    let node_id = window.focus_node_id_in_rendered_frame(focus_id);
                    window.dispatch_action_on_node(node_id, action.as_ref(), cx);
                })
                .log_err();
        })
    }

    pub(crate) fn dispatch_keystroke_observers(
        &mut self,
        event: &dyn Any,
        action: Option<Box<dyn Action>>,
        context_stack: Vec<KeyContext>,
        cx: &mut App,
    ) {
        let Some(key_down_event) = event.downcast_ref::<KeyDownEvent>() else {
            return;
        };

        cx.keystroke_observers.clone().retain(&(), move |callback| {
            (callback)(
                &KeystrokeEvent {
                    keystroke: key_down_event.keystroke.clone(),
                    action: action.as_ref().map(|action| action.boxed_clone()),
                    context_stack: context_stack.clone(),
                },
                self,
                cx,
            )
        });
    }

    pub(crate) fn dispatch_keystroke_interceptors(
        &mut self,
        event: &dyn Any,
        context_stack: Vec<KeyContext>,
        cx: &mut App,
    ) {
        let Some(key_down_event) = event.downcast_ref::<KeyDownEvent>() else {
            return;
        };

        cx.keystroke_interceptors
            .clone()
            .retain(&(), move |callback| {
                (callback)(
                    &KeystrokeEvent {
                        keystroke: key_down_event.keystroke.clone(),
                        action: None,
                        context_stack: context_stack.clone(),
                    },
                    self,
                    cx,
                )
            });
    }

    /// Schedules the given function to be run at the end of the current effect cycle, allowing entities
    /// that are currently on the stack to be returned to the app.
    pub fn defer(&self, cx: &mut App, f: impl FnOnce(&mut Window, &mut App) + 'static) {
        let handle = self.handle;
        cx.defer(move |cx| {
            handle.update(cx, |_, window, cx| f(window, cx)).ok();
        });
    }

    /// Subscribe to events emitted by a entity.
    /// The entity to which you're subscribing must implement the [`EventEmitter`] trait.
    /// The callback will be invoked a handle to the emitting entity, the event, and a window context for the current window.
    pub fn observe<T: 'static>(
        &mut self,
        observed: &Entity<T>,
        cx: &mut App,
        mut on_notify: impl FnMut(Entity<T>, &mut Window, &mut App) + 'static,
    ) -> Subscription {
        let entity_id = observed.entity_id();
        let observed = observed.downgrade();
        let window_handle = self.handle;
        cx.new_observer(
            entity_id,
            Box::new(move |cx| {
                window_handle
                    .update(cx, |_, window, cx| {
                        if let Some(handle) = observed.upgrade() {
                            on_notify(handle, window, cx);
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false)
            }),
        )
    }

    /// Subscribe to events emitted by a entity.
    /// The entity to which you're subscribing must implement the [`EventEmitter`] trait.
    /// The callback will be invoked a handle to the emitting entity, the event, and a window context for the current window.
    pub fn subscribe<Emitter, Evt>(
        &mut self,
        entity: &Entity<Emitter>,
        cx: &mut App,
        mut on_event: impl FnMut(Entity<Emitter>, &Evt, &mut Window, &mut App) + 'static,
    ) -> Subscription
    where
        Emitter: EventEmitter<Evt>,
        Evt: 'static,
    {
        let entity_id = entity.entity_id();
        let handle = entity.downgrade();
        let window_handle = self.handle;
        cx.new_subscription(
            entity_id,
            (
                TypeId::of::<Evt>(),
                Box::new(move |event, cx| {
                    window_handle
                        .update(cx, |_, window, cx| {
                            if let Some(entity) = handle.upgrade() {
                                let event = event.downcast_ref().expect("invalid event type");
                                on_event(entity, event, window, cx);
                                true
                            } else {
                                false
                            }
                        })
                        .unwrap_or(false)
                }),
            ),
        )
    }

    /// Register a callback to be invoked when the given `Entity` is released.
    pub fn observe_release<T>(
        &self,
        entity: &Entity<T>,
        cx: &mut App,
        mut on_release: impl FnOnce(&mut T, &mut Window, &mut App) + 'static,
    ) -> Subscription
    where
        T: 'static,
    {
        let entity_id = entity.entity_id();
        let window_handle = self.handle;
        let (subscription, activate) = cx.release_listeners.insert(
            entity_id,
            Box::new(move |entity, cx| {
                let entity = entity.downcast_mut().expect("invalid entity type");
                let _ = window_handle.update(cx, |_, window, cx| on_release(entity, window, cx));
            }),
        );
        activate();
        subscription
    }

    /// Creates an [`AsyncWindowContext`], which has a static lifetime and can be held across
    /// await points in async code.
    pub fn to_async(&self, cx: &App) -> AsyncWindowContext {
        AsyncWindowContext::new_context(cx.to_async(), self.handle)
    }

    /// Schedule the given closure to be run directly after the current frame is rendered.
    pub fn on_next_frame(&self, callback: impl FnOnce(&mut Window, &mut App) + 'static) {
        RefCell::borrow_mut(&self.next_frame_callbacks).push(Box::new(callback));
    }

    /// Schedule a frame to be drawn on the next animation frame.
    ///
    /// This is useful for elements that need to animate continuously, such as a video player or an animated GIF.
    /// It will cause the window to redraw on the next frame, even if no other changes have occurred.
    ///
    /// If called from within a view, it will notify that view on the next frame. Otherwise, it will refresh the entire window.
    pub fn request_animation_frame(&self) {
        let entity = self.current_view();
        self.on_next_frame(move |_, cx| cx.notify(entity));
    }

    /// Spawn the future returned by the given closure on the application thread pool.
    /// The closure is provided a handle to the current window and an `AsyncWindowContext` for
    /// use within your future.
    #[track_caller]
    pub fn spawn<AsyncFn, R>(&self, cx: &App, f: AsyncFn) -> Task<R>
    where
        R: 'static,
        AsyncFn: AsyncFnOnce(&mut AsyncWindowContext) -> R + 'static,
    {
        let handle = self.handle;
        cx.spawn(async move |app| {
            let mut async_window_cx = AsyncWindowContext::new_context(app.clone(), handle);
            f(&mut async_window_cx).await
        })
    }

    /// Spawn the future returned by the given closure on the application thread
    /// pool, with the given priority. The closure is provided a handle to the
    /// current window and an `AsyncWindowContext` for use within your future.
    #[track_caller]
    pub fn spawn_with_priority<AsyncFn, R>(
        &self,
        priority: Priority,
        cx: &App,
        f: AsyncFn,
    ) -> Task<R>
    where
        R: 'static,
        AsyncFn: AsyncFnOnce(&mut AsyncWindowContext) -> R + 'static,
    {
        let handle = self.handle;
        cx.spawn_with_priority(priority, async move |app| {
            let mut async_window_cx = AsyncWindowContext::new_context(app.clone(), handle);
            f(&mut async_window_cx).await
        })
    }

    fn bounds_changed(&mut self, cx: &mut App) {
        self.scale_factor = self.platform_window.scale_factor();
        self.viewport_size = self.platform_window.content_size();
        self.display_id = self.platform_window.display().map(|display| display.id());

        self.refresh();

        self.bounds_observers
            .clone()
            .retain(&(), |callback| callback(self, cx));
    }

    /// Returns the bounds of the current window in the global coordinate space, which could span across multiple displays.
    pub fn bounds(&self) -> Bounds<Pixels> {
        self.platform_window.bounds()
    }

    /// Renders the current frame's scene to a texture and returns the pixel data as an RGBA image.
    /// This does not present the frame to screen - useful for visual testing where we want
    /// to capture what would be rendered without displaying it or requiring the window to be visible.
    #[cfg(any(test, feature = "test-support"))]
    pub fn render_to_image(&self) -> anyhow::Result<image::RgbaImage> {
        self.platform_window
            .render_to_image(&self.rendered_frame.scene)
    }

    /// Set the content size of the window.
    pub fn resize(&mut self, size: Size<Pixels>) {
        self.platform_window.resize(size);
    }

    /// Returns whether or not the window is currently fullscreen
    pub fn is_fullscreen(&self) -> bool {
        self.platform_window.is_fullscreen()
    }

    pub(crate) fn appearance_changed(&mut self, cx: &mut App) {
        self.appearance = self.platform_window.appearance();

        self.appearance_observers
            .clone()
            .retain(&(), |callback| callback(self, cx));

        self.refresh();
    }

    /// Returns the appearance of the current window.
    pub fn appearance(&self) -> WindowAppearance {
        self.appearance
    }

    /// Returns the height of the titlebar area (including any native toolbar).
    pub fn titlebar_height(&self) -> Pixels {
        self.platform_window.titlebar_height()
    }

    /// Returns the size of the drawable area within the window.
    pub fn viewport_size(&self) -> Size<Pixels> {
        self.viewport_size
    }

    /// Returns whether this window is focused by the operating system (receiving key events).
    pub fn is_window_active(&self) -> bool {
        self.active.get()
    }

    /// Returns whether this window is considered to be the window
    /// that currently owns the mouse cursor.
    /// On mac, this is equivalent to `is_window_active`.
    pub fn is_window_hovered(&self) -> bool {
        if cfg!(any(
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )) {
            self.hovered.get()
        } else {
            self.is_window_active()
        }
    }

    /// Toggle zoom on the window.
    pub fn zoom_window(&self) {
        self.platform_window.zoom();
    }

    /// Opens the native title bar context menu, useful when implementing client side decorations (Wayland and X11)
    pub fn show_window_menu(&self, position: Point<Pixels>) {
        self.platform_window.show_window_menu(position)
    }

    /// Handle window movement for Linux and macOS.
    /// Tells the compositor to take control of window movement (Wayland and X11)
    ///
    /// Events may not be received during a move operation.
    pub fn start_window_move(&self) {
        self.platform_window.start_window_move()
    }

    /// When using client side decorations, set this to the width of the invisible decorations (Wayland and X11)
    pub fn set_client_inset(&mut self, inset: Pixels) {
        self.client_inset = Some(inset);
        self.platform_window.set_client_inset(inset);
    }

    /// Returns the client_inset value by [`Self::set_client_inset`].
    pub fn client_inset(&self) -> Option<Pixels> {
        self.client_inset
    }

    /// Returns whether the title bar window controls need to be rendered by the application (Wayland and X11)
    pub fn window_decorations(&self) -> Decorations {
        self.platform_window.window_decorations()
    }

    /// Returns which window controls are currently visible (Wayland)
    pub fn window_controls(&self) -> WindowControls {
        self.platform_window.window_controls()
    }

    /// Updates the window's title at the platform level.
    pub fn set_window_title(&mut self, title: &str) {
        self.platform_window.set_title(title);
    }

    /// Configures the native toolbar for this window.
    ///
    /// On macOS this installs an `NSToolbar` with native items.
    /// On other platforms this is currently a no-op.
    pub fn set_native_toolbar(&mut self, toolbar: Option<NativeToolbar>) {
        let toolbar = toolbar.map(|toolbar| {
            toolbar.into_platform(self.next_frame_callbacks.clone(), self.invalidator.clone())
        });
        self.platform_window.set_native_toolbar(toolbar);
    }

    /// Focuses a native search field by target.
    ///
    /// This is designed for keyboard shortcut handlers (e.g. `cmd-l`) where
    /// search focus should move to a native toolbar or content search field.
    pub fn focus_native_search_field(
        &mut self,
        target: NativeSearchFieldTarget,
        select_all_text: bool,
    ) {
        let platform_target = match target {
            NativeSearchFieldTarget::ToolbarItem(item_id) => {
                PlatformNativeSearchFieldTarget::ToolbarItem(item_id)
            }
            NativeSearchFieldTarget::ContentElement(element_id) => {
                PlatformNativeSearchFieldTarget::ContentView(element_id.to_string().into())
            }
        };
        self.platform_window
            .focus_native_search_field(platform_target, select_all_text);
    }

    /// Resigns first responder from any active native field editor (e.g. NSSearchField),
    /// causing the field to lose focus as if the user clicked away.
    ///
    /// On macOS this calls `[window makeFirstResponder: nil]`. On other platforms this is a no-op.
    pub fn blur_native_field_editor(&mut self) {
        self.platform_window.blur_native_field_editor();
    }

    /// Shows a native popover (NSPopover) anchored to the given position.
    ///
    /// On macOS this creates an NSPopover with native appearance and behavior.
    /// On other platforms this is currently a no-op.
    pub fn show_native_popover(&mut self, popover: NativePopover, anchor: NativePopoverAnchor) {
        let (platform_popover, platform_anchor) = popover.into_platform_with_anchor(
            anchor,
            self.next_frame_callbacks.clone(),
            self.invalidator.clone(),
        );
        self.platform_window
            .show_native_popover(platform_popover, platform_anchor);
    }

    /// Dismisses any currently shown native popover.
    pub fn dismiss_native_popover(&mut self) {
        self.platform_window.dismiss_native_popover();
    }

    /// Shows a native panel (NSPanel) with the given content and anchor position.
    ///
    /// Panels are floating auxiliary windows used for tool palettes, suggestion boxes,
    /// inspectors, and other secondary UI. Unlike popovers, panels are standalone
    /// windows that can be positioned freely.
    pub fn show_native_panel(&mut self, panel: NativePanel, anchor: NativePanelAnchor) {
        let (platform_panel, platform_anchor) = panel.into_platform_with_anchor(
            anchor,
            self.next_frame_callbacks.clone(),
            self.invalidator.clone(),
        );
        self.platform_window
            .show_native_panel(platform_panel, platform_anchor);
    }

    /// Dismisses any currently shown native panel.
    pub fn dismiss_native_panel(&mut self) {
        self.platform_window.dismiss_native_panel();
    }

    /// Shows a native alert dialog as a sheet attached to this window.
    ///
    /// Returns a `oneshot::Receiver<usize>` that resolves to the button index
    /// when the user dismisses the alert. Button index 0 is the first button added.
    pub fn show_native_alert(
        &self,
        alert: NativeAlert,
    ) -> Option<oneshot::Receiver<usize>> {
        let platform_alert = alert.into_platform();
        self.platform_window.show_native_alert_sheet(platform_alert)
    }

    /// Sets the application identifier.
    pub fn set_app_id(&mut self, app_id: &str) {
        self.platform_window.set_app_id(app_id);
    }

    /// Sets the window background appearance.
    pub fn set_background_appearance(&self, background_appearance: WindowBackgroundAppearance) {
        self.platform_window
            .set_background_appearance(background_appearance);
    }

    /// Mark the window as dirty at the platform level.
    pub fn set_window_edited(&mut self, edited: bool) {
        self.platform_window.set_edited(edited);
    }

    /// Determine the display on which the window is visible.
    pub fn display(&self, cx: &App) -> Option<Rc<dyn PlatformDisplay>> {
        cx.platform
            .displays()
            .into_iter()
            .find(|display| Some(display.id()) == self.display_id)
    }

    /// Show the platform character palette.
    pub fn show_character_palette(&self) {
        self.platform_window.show_character_palette();
    }

    /// Returns a raw pointer to the platform's native view (e.g., NSView on macOS).
    /// Used by native controls to add platform subviews.
    ///
    /// When a GpuiSurface is being painted, this returns the surface's GPUISurfaceView
    /// instead of the main window's view, so native controls automatically parent
    /// themselves into the correct container.
    pub fn raw_native_view_ptr(&self) -> *mut std::ffi::c_void {
        if let Some(&override_ptr) = self.native_view_override_stack.last() {
            override_ptr
        } else {
            self.platform_window.raw_native_view_ptr()
        }
    }

    /// Push a native view override. While active, `raw_native_view_ptr()` returns
    /// this pointer instead of the main window's view. Used during surface paint.
    pub(crate) fn push_native_view_override(&mut self, ptr: *mut std::ffi::c_void) {
        self.native_view_override_stack.push(ptr);
    }

    /// Pop the most recent native view override.
    pub(crate) fn pop_native_view_override(&mut self) {
        self.native_view_override_stack.pop();
    }

    /// Register a secondary GPUI rendering surface. Returns a handle with the
    /// surface's native view pointer for embedding in a container NSView.
    ///
    /// The surface will render the given `view` into its own Metal layer each frame.
    /// Native controls painted within the surface's view will automatically parent
    /// themselves to the surface's NSView.
    #[cfg(target_os = "macos")]
    pub fn register_surface(&mut self, view: AnyView) -> GpuiSurfaceHandle {
        use crate::platform::gpui_surface::GpuiSurface;

        let id = SurfaceId::new();

        // Get shared resources from the main renderer via the platform window
        let shared = self.platform_window.shared_render_resources();

        let mut gpui_surface = GpuiSurface::new(shared, true);

        // Attach window state so the surface view can forward events
        // through MacWindowState::surface_event_callback.
        let window_state_ptr = self.platform_window.window_state_ptr();
        gpui_surface.set_window_state(window_state_ptr);

        let native_view_ptr = gpui_surface.native_view_ptr();

        self.surfaces.insert(id, SurfaceState {
            gpui_surface,
            root_view: view,
            scene: Scene::default(),
            layout_engine: Some(TaffyLayoutEngine::new()),
            dirty: true,
            mouse_listeners: Vec::new(),
            hitboxes: Vec::new(),
            mouse_position: Point::default(),
            mouse_hit_test: HitTest::default(),
        });

        GpuiSurfaceHandle {
            id,
            native_view_ptr,
        }
    }

    /// Unregister a previously registered surface.
    #[cfg(target_os = "macos")]
    pub fn unregister_surface(&mut self, id: SurfaceId) {
        self.surfaces.remove(&id);
    }

    /// Get the native view pointer for a registered surface.
    #[cfg(target_os = "macos")]
    pub fn surface_native_view_ptr(&self, id: SurfaceId) -> Option<*mut std::ffi::c_void> {
        self.surfaces.get(&id).map(|s| s.gpui_surface.native_view_ptr())
    }

    /// Mark a surface as needing redraw.
    #[cfg(target_os = "macos")]
    pub fn invalidate_surface(&mut self, id: SurfaceId) {
        if let Some(surface) = self.surfaces.get_mut(&id) {
            surface.dirty = true;
        }
    }

    /// Update the root view of a registered surface and mark it dirty.
    #[cfg(target_os = "macos")]
    pub fn update_surface_root_view(&mut self, id: SurfaceId, view: AnyView) {
        if let Some(surface) = self.surfaces.get_mut(&id) {
            surface.root_view = view;
            surface.dirty = true;
        }
    }

    /// The scale factor of the display associated with the window. For example, it could
    /// return 2.0 for a "retina" display, indicating that each logical pixel should actually
    /// be rendered as two pixels on screen.
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    /// The size of an em for the base font of the application. Adjusting this value allows the
    /// UI to scale, just like zooming a web page.
    pub fn rem_size(&self) -> Pixels {
        self.rem_size_override_stack
            .last()
            .copied()
            .unwrap_or(self.rem_size)
    }

    /// Sets the size of an em for the base font of the application. Adjusting this value allows the
    /// UI to scale, just like zooming a web page.
    pub fn set_rem_size(&mut self, rem_size: impl Into<Pixels>) {
        self.rem_size = rem_size.into();
    }

    /// Acquire a globally unique identifier for the given ElementId.
    /// Only valid for the duration of the provided closure.
    pub fn with_global_id<R>(
        &mut self,
        element_id: ElementId,
        f: impl FnOnce(&GlobalElementId, &mut Self) -> R,
    ) -> R {
        self.with_id(element_id, |this| {
            let global_id = GlobalElementId(Arc::from(&*this.element_id_stack));

            f(&global_id, this)
        })
    }

    /// Calls the provided closure with the element ID pushed on the stack.
    #[inline]
    pub fn with_id<R>(
        &mut self,
        element_id: impl Into<ElementId>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.element_id_stack.push(element_id.into());
        let result = f(self);
        self.element_id_stack.pop();
        result
    }

    /// Executes the provided function with the specified rem size.
    ///
    /// This method must only be called as part of element drawing.
    // This function is called in a highly recursive manner in editor
    // prepainting, make sure its inlined to reduce the stack burden
    #[inline]
    pub fn with_rem_size<F, R>(&mut self, rem_size: Option<impl Into<Pixels>>, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        self.invalidator.debug_assert_paint_or_prepaint();

        if let Some(rem_size) = rem_size {
            self.rem_size_override_stack.push(rem_size.into());
            let result = f(self);
            self.rem_size_override_stack.pop();
            result
        } else {
            f(self)
        }
    }

    /// The line height associated with the current text style.
    pub fn line_height(&self) -> Pixels {
        self.text_style().line_height_in_pixels(self.rem_size())
    }

    /// Call to prevent the default action of an event. Currently only used to prevent
    /// parent elements from becoming focused on mouse down.
    pub fn prevent_default(&mut self) {
        self.default_prevented = true;
    }

    /// Obtain whether default has been prevented for the event currently being dispatched.
    pub fn default_prevented(&self) -> bool {
        self.default_prevented
    }

    /// Determine whether the given action is available along the dispatch path to the currently focused element.
    pub fn is_action_available(&self, action: &dyn Action, cx: &App) -> bool {
        let node_id =
            self.focus_node_id_in_rendered_frame(self.focused(cx).map(|handle| handle.id));
        self.rendered_frame
            .dispatch_tree
            .is_action_available(action, node_id)
    }

    /// Determine whether the given action is available along the dispatch path to the given focus_handle.
    pub fn is_action_available_in(&self, action: &dyn Action, focus_handle: &FocusHandle) -> bool {
        let node_id = self.focus_node_id_in_rendered_frame(Some(focus_handle.id));
        self.rendered_frame
            .dispatch_tree
            .is_action_available(action, node_id)
    }

    /// The position of the mouse relative to the window.
    pub fn mouse_position(&self) -> Point<Pixels> {
        self.mouse_position
    }

    /// The current state of the keyboard's modifiers
    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// Returns true if the last input event was keyboard-based (key press, tab navigation, etc.)
    /// This is used for focus-visible styling to show focus indicators only for keyboard navigation.
    pub fn last_input_was_keyboard(&self) -> bool {
        self.last_input_modality == InputModality::Keyboard
    }

    /// The current state of the keyboard's capslock
    pub fn capslock(&self) -> Capslock {
        self.capslock
    }

    fn complete_frame(&self) {
        self.platform_window.completed_frame();
    }

    /// Produces a new frame and assigns it to `rendered_frame`. To actually show
    /// the contents of the new [`Scene`], use [`Self::present`].
    #[profiling::function]
    pub fn draw(&mut self, cx: &mut App) -> ArenaClearNeeded {
        // Set up the per-App arena for element allocation during this draw.
        // This ensures that multiple test Apps have isolated arenas.
        let _arena_scope = ElementArenaScope::enter(&cx.element_arena);

        self.invalidate_entities();
        cx.entities.clear_accessed();
        debug_assert!(self.rendered_entity_stack.is_empty());
        self.invalidator.set_dirty(false);
        self.requested_autoscroll = None;

        // Restore the previously-used input handler.
        if let Some(input_handler) = self.platform_window.take_input_handler() {
            self.rendered_frame.input_handlers.push(Some(input_handler));
        }
        if !cx.mode.skip_drawing() {
            self.draw_roots(cx);
            #[cfg(target_os = "macos")]
            self.draw_surfaces(cx);
        }
        self.dirty_views.clear();
        self.next_frame.window_active = self.active.get();

        // Register requested input handler with the platform window.
        if let Some(input_handler) = self.next_frame.input_handlers.pop() {
            self.platform_window
                .set_input_handler(input_handler.unwrap());
        }

        self.layout_engine.as_mut().unwrap().clear();
        self.text_system().finish_frame();
        self.next_frame.finish(&mut self.rendered_frame);

        self.invalidator.set_phase(DrawPhase::Focus);
        let previous_focus_path = self.rendered_frame.focus_path();
        let previous_window_active = self.rendered_frame.window_active;
        mem::swap(&mut self.rendered_frame, &mut self.next_frame);
        self.next_frame.clear();
        let current_focus_path = self.rendered_frame.focus_path();
        let current_window_active = self.rendered_frame.window_active;

        if previous_focus_path != current_focus_path
            || previous_window_active != current_window_active
        {
            if !previous_focus_path.is_empty() && current_focus_path.is_empty() {
                self.focus_lost_listeners
                    .clone()
                    .retain(&(), |listener| listener(self, cx));
            }

            let event = WindowFocusEvent {
                previous_focus_path: if previous_window_active {
                    previous_focus_path
                } else {
                    Default::default()
                },
                current_focus_path: if current_window_active {
                    current_focus_path
                } else {
                    Default::default()
                },
            };
            self.focus_listeners
                .clone()
                .retain(&(), |listener| listener(&event, self, cx));
        }

        debug_assert!(self.rendered_entity_stack.is_empty());
        self.record_entities_accessed(cx);
        self.reset_cursor_style(cx);
        self.refreshing = false;
        self.invalidator.set_phase(DrawPhase::None);
        self.needs_present.set(true);

        ArenaClearNeeded::new(&cx.element_arena)
    }

    fn record_entities_accessed(&mut self, cx: &mut App) {
        let mut entities_ref = cx.entities.accessed_entities.borrow_mut();
        let mut entities = mem::take(entities_ref.deref_mut());
        drop(entities_ref);
        let handle = self.handle;
        cx.record_entities_accessed(
            handle,
            // Try moving window invalidator into the Window
            self.invalidator.clone(),
            &entities,
        );
        let mut entities_ref = cx.entities.accessed_entities.borrow_mut();
        mem::swap(&mut entities, entities_ref.deref_mut());
    }

    fn invalidate_entities(&mut self) {
        let mut views = self.invalidator.take_views();
        for entity in views.drain() {
            self.mark_view_dirty(entity);
        }
        self.invalidator.replace_views(views);
    }

    #[profiling::function]
    fn present(&mut self) {
        self.platform_window.draw(&self.rendered_frame.scene);
        #[cfg(target_os = "macos")]
        {
            let scale_factor = self.scale_factor;
            for surface in self.surfaces.values_mut() {
                let content_size = surface.gpui_surface.content_size();
                let device_width = (content_size.width.0 * scale_factor).ceil() as i32;
                let device_height = (content_size.height.0 * scale_factor).ceil() as i32;
                let device_size = crate::size(
                    crate::DevicePixels(device_width),
                    crate::DevicePixels(device_height),
                );
                surface.gpui_surface.set_contents_scale(scale_factor as f64);
                surface.gpui_surface.update_drawable_size(device_size);
                surface.gpui_surface.draw(&surface.scene);
            }
        }
        self.needs_present.set(false);
        profiling::finish_frame!();
    }

    fn draw_roots(&mut self, cx: &mut App) {
        self.invalidator.set_phase(DrawPhase::Prepaint);
        self.tooltip_bounds.take();

        let _inspector_width: Pixels = rems(30.0).to_pixels(self.rem_size());
        let root_size = {
            #[cfg(any(feature = "inspector", debug_assertions))]
            {
                if self.inspector.is_some() {
                    let mut size = self.viewport_size;
                    size.width = (size.width - _inspector_width).max(px(0.0));
                    size
                } else {
                    self.viewport_size
                }
            }
            #[cfg(not(any(feature = "inspector", debug_assertions)))]
            {
                self.viewport_size
            }
        };

        // Layout all root elements.
        let mut root_element = self.root.as_ref().unwrap().clone().into_any();
        root_element.prepaint_as_root(Point::default(), root_size.into(), self, cx);

        #[cfg(any(feature = "inspector", debug_assertions))]
        let inspector_element = self.prepaint_inspector(_inspector_width, cx);

        let mut sorted_deferred_draws =
            (0..self.next_frame.deferred_draws.len()).collect::<SmallVec<[_; 8]>>();
        sorted_deferred_draws.sort_by_key(|ix| self.next_frame.deferred_draws[*ix].priority);
        self.prepaint_deferred_draws(&sorted_deferred_draws, cx);

        let mut prompt_element = None;
        let mut active_drag_element = None;
        let mut tooltip_element = None;
        if let Some(prompt) = self.prompt.take() {
            let mut element = prompt.view.any_view().into_any();
            element.prepaint_as_root(Point::default(), root_size.into(), self, cx);
            prompt_element = Some(element);
            self.prompt = Some(prompt);
        } else if let Some(active_drag) = cx.active_drag.take() {
            let mut element = active_drag.view.clone().into_any();
            let offset = self.mouse_position() - active_drag.cursor_offset;
            element.prepaint_as_root(offset, AvailableSpace::min_size(), self, cx);
            active_drag_element = Some(element);
            cx.active_drag = Some(active_drag);
        } else {
            tooltip_element = self.prepaint_tooltip(cx);
        }

        self.mouse_hit_test = self.next_frame.hit_test(self.mouse_position);

        // Now actually paint the elements.
        self.invalidator.set_phase(DrawPhase::Paint);
        root_element.paint(self, cx);

        #[cfg(any(feature = "inspector", debug_assertions))]
        self.paint_inspector(inspector_element, cx);

        self.paint_deferred_draws(&sorted_deferred_draws, cx);

        if let Some(mut prompt_element) = prompt_element {
            prompt_element.paint(self, cx);
        } else if let Some(mut drag_element) = active_drag_element {
            drag_element.paint(self, cx);
        } else if let Some(mut tooltip_element) = tooltip_element {
            tooltip_element.paint(self, cx);
        }

        #[cfg(any(feature = "inspector", debug_assertions))]
        self.paint_inspector_hitbox(cx);
    }

    /// Draw all registered secondary surfaces. Each surface gets its own
    /// prepaint/paint cycle with an isolated scene and layout engine.
    /// Frame state (mouse listeners, hitboxes, input handlers, cursor styles)
    /// is saved before each surface draw and restored afterward so that
    /// surface elements don't pollute the main frame's event dispatch.
    #[cfg(target_os = "macos")]
    fn draw_surfaces(&mut self, cx: &mut App) {
        let surface_ids: Vec<SurfaceId> = self.surfaces.keys().copied().collect();

        for id in surface_ids {
            let surface = self.surfaces.get_mut(&id).unwrap();

            // Always redraw surfaces for now (dirty tracking can be refined later)
            surface.dirty = false;

            // Save the main frame's accumulated state so the surface draw
            // doesn't add its listeners/hitboxes to the main dispatch path.
            let main_scene = std::mem::take(&mut self.next_frame.scene);
            let main_mouse_listeners = std::mem::take(&mut self.next_frame.mouse_listeners);
            let main_hitboxes = std::mem::take(&mut self.next_frame.hitboxes);
            let main_input_handlers = std::mem::take(&mut self.next_frame.input_handlers);
            let main_cursor_styles = std::mem::take(&mut self.next_frame.cursor_styles);
            let main_tooltip_requests = std::mem::take(&mut self.next_frame.tooltip_requests);

            // Swap in the surface's layout engine
            let surface_layout = surface.layout_engine.take();
            let main_layout = std::mem::replace(&mut self.layout_engine, surface_layout);

            // Grab what we need before releasing the borrow
            let root_view = surface.root_view.clone();
            let surface_size = surface.gpui_surface.content_size();
            let native_view_ptr = surface.gpui_surface.native_view_ptr();
            let surface_mouse_position = surface.mouse_position;

            // Push native view override so native controls parent to the surface's NSView
            self.push_native_view_override(native_view_ptr);

            // Set the surface's mouse position for hit testing
            let saved_mouse_position = self.mouse_position;
            let saved_mouse_hit_test = self.mouse_hit_test.clone();
            self.mouse_position = surface_mouse_position;

            // Prepaint the surface's element tree — this allocates fresh hitbox IDs
            self.invalidator.set_phase(DrawPhase::Prepaint);
            let mut root_element = root_view.into_any();
            root_element.prepaint_as_root(
                Point::default(),
                surface_size.into(),
                self,
                cx,
            );

            // Recompute the hit test from the freshly allocated hitboxes so that
            // is_hovered() sees matching IDs during paint. We cannot reuse the
            // hit test stored from event dispatch because next_hitbox_id is
            // monotonically increasing — old IDs are stale after a new render.
            let hit_test = {
                let mut set_hover_hitbox_count = false;
                let mut ht = HitTest::default();
                for hitbox in self.next_frame.hitboxes.iter().rev() {
                    let bounds = hitbox.bounds.intersect(&hitbox.content_mask.bounds);
                    if bounds.contains(&surface_mouse_position) {
                        ht.ids.push(hitbox.id);
                        if !set_hover_hitbox_count
                            && hitbox.behavior == HitboxBehavior::BlockMouseExceptScroll
                        {
                            ht.hover_hitbox_count = ht.ids.len();
                            set_hover_hitbox_count = true;
                        }
                        if hitbox.behavior == HitboxBehavior::BlockMouse {
                            break;
                        }
                    }
                }
                if !set_hover_hitbox_count {
                    ht.hover_hitbox_count = ht.ids.len();
                }
                ht
            };
            self.mouse_hit_test = hit_test;

            // Paint the surface's element tree
            self.invalidator.set_phase(DrawPhase::Paint);
            root_element.paint(self, cx);

            // Pop native view override
            self.pop_native_view_override();

            // Restore the main window's mouse state
            self.mouse_position = saved_mouse_position;
            self.mouse_hit_test = saved_mouse_hit_test;

            // Extract the surface's scene, listeners, and hitboxes into
            // SurfaceState so they can be used for surface event dispatch.
            // Restore the main frame's state so the main dispatch path is
            // unaffected.
            let surface = self.surfaces.get_mut(&id).unwrap();
            surface.scene = std::mem::replace(&mut self.next_frame.scene, main_scene);
            surface.mouse_listeners = std::mem::replace(&mut self.next_frame.mouse_listeners, main_mouse_listeners);
            surface.hitboxes = std::mem::replace(&mut self.next_frame.hitboxes, main_hitboxes);
            // Preserve any input handlers registered by surface elements (e.g.
            // focused text fields) so they survive to the set_input_handler call
            // after draw. The main frame handlers are restored first, then surface
            // handlers are appended — .pop() at line ~4600 will pick up whichever
            // was last pushed (i.e. the focused element's handler).
            let surface_input_handlers = std::mem::replace(
                &mut self.next_frame.input_handlers,
                main_input_handlers,
            );
            self.next_frame
                .input_handlers
                .extend(surface_input_handlers);
            self.next_frame.cursor_styles = main_cursor_styles;
            self.next_frame.tooltip_requests = main_tooltip_requests;

            // Restore layout engines
            let surface_layout = std::mem::replace(&mut self.layout_engine, main_layout);
            surface.layout_engine = surface_layout;
        }
    }

    fn prepaint_tooltip(&mut self, cx: &mut App) -> Option<AnyElement> {
        // Use indexing instead of iteration to avoid borrowing self for the duration of the loop.
        for tooltip_request_index in (0..self.next_frame.tooltip_requests.len()).rev() {
            let Some(Some(tooltip_request)) = self
                .next_frame
                .tooltip_requests
                .get(tooltip_request_index)
                .cloned()
            else {
                log::error!("Unexpectedly absent TooltipRequest");
                continue;
            };
            let mut element = tooltip_request.tooltip.view.clone().into_any();
            let mouse_position = tooltip_request.tooltip.mouse_position;
            let tooltip_size = element.layout_as_root(AvailableSpace::min_size(), self, cx);

            let mut tooltip_bounds =
                Bounds::new(mouse_position + point(px(1.), px(1.)), tooltip_size);
            let window_bounds = Bounds {
                origin: Point::default(),
                size: self.viewport_size(),
            };

            if tooltip_bounds.right() > window_bounds.right() {
                let new_x = mouse_position.x - tooltip_bounds.size.width - px(1.);
                if new_x >= Pixels::ZERO {
                    tooltip_bounds.origin.x = new_x;
                } else {
                    tooltip_bounds.origin.x = cmp::max(
                        Pixels::ZERO,
                        tooltip_bounds.origin.x - tooltip_bounds.right() - window_bounds.right(),
                    );
                }
            }

            if tooltip_bounds.bottom() > window_bounds.bottom() {
                let new_y = mouse_position.y - tooltip_bounds.size.height - px(1.);
                if new_y >= Pixels::ZERO {
                    tooltip_bounds.origin.y = new_y;
                } else {
                    tooltip_bounds.origin.y = cmp::max(
                        Pixels::ZERO,
                        tooltip_bounds.origin.y - tooltip_bounds.bottom() - window_bounds.bottom(),
                    );
                }
            }

            // It's possible for an element to have an active tooltip while not being painted (e.g.
            // via the `visible_on_hover` method). Since mouse listeners are not active in this
            // case, instead update the tooltip's visibility here.
            let is_visible =
                (tooltip_request.tooltip.check_visible_and_update)(tooltip_bounds, self, cx);
            if !is_visible {
                continue;
            }

            self.with_absolute_element_offset(tooltip_bounds.origin, |window| {
                element.prepaint(window, cx)
            });

            self.tooltip_bounds = Some(TooltipBounds {
                id: tooltip_request.id,
                bounds: tooltip_bounds,
            });
            return Some(element);
        }
        None
    }

    fn prepaint_deferred_draws(&mut self, deferred_draw_indices: &[usize], cx: &mut App) {
        assert_eq!(self.element_id_stack.len(), 0);

        let mut deferred_draws = mem::take(&mut self.next_frame.deferred_draws);
        for deferred_draw_ix in deferred_draw_indices {
            let deferred_draw = &mut deferred_draws[*deferred_draw_ix];
            self.element_id_stack
                .clone_from(&deferred_draw.element_id_stack);
            self.text_style_stack
                .clone_from(&deferred_draw.text_style_stack);
            self.next_frame
                .dispatch_tree
                .set_active_node(deferred_draw.parent_node);

            let prepaint_start = self.prepaint_index();
            if let Some(element) = deferred_draw.element.as_mut() {
                self.with_rendered_view(deferred_draw.current_view, |window| {
                    window.with_rem_size(Some(deferred_draw.rem_size), |window| {
                        window.with_absolute_element_offset(
                            deferred_draw.absolute_offset,
                            |window| {
                                element.prepaint(window, cx);
                            },
                        );
                    });
                })
            } else {
                self.reuse_prepaint(deferred_draw.prepaint_range.clone());
            }
            let prepaint_end = self.prepaint_index();
            deferred_draw.prepaint_range = prepaint_start..prepaint_end;
        }
        assert_eq!(
            self.next_frame.deferred_draws.len(),
            0,
            "cannot call defer_draw during deferred drawing"
        );
        self.next_frame.deferred_draws = deferred_draws;
        self.element_id_stack.clear();
        self.text_style_stack.clear();
    }

    fn paint_deferred_draws(&mut self, deferred_draw_indices: &[usize], cx: &mut App) {
        assert_eq!(self.element_id_stack.len(), 0);

        let mut deferred_draws = mem::take(&mut self.next_frame.deferred_draws);
        for deferred_draw_ix in deferred_draw_indices {
            let mut deferred_draw = &mut deferred_draws[*deferred_draw_ix];
            self.element_id_stack
                .clone_from(&deferred_draw.element_id_stack);
            self.next_frame
                .dispatch_tree
                .set_active_node(deferred_draw.parent_node);

            let paint_start = self.paint_index();
            if let Some(element) = deferred_draw.element.as_mut() {
                self.with_rendered_view(deferred_draw.current_view, |window| {
                    window.with_rem_size(Some(deferred_draw.rem_size), |window| {
                        element.paint(window, cx);
                    })
                })
            } else {
                self.reuse_paint(deferred_draw.paint_range.clone());
            }
            let paint_end = self.paint_index();
            deferred_draw.paint_range = paint_start..paint_end;
        }
        self.next_frame.deferred_draws = deferred_draws;
        self.element_id_stack.clear();
    }

    pub(crate) fn prepaint_index(&self) -> PrepaintStateIndex {
        PrepaintStateIndex {
            hitboxes_index: self.next_frame.hitboxes.len(),
            tooltips_index: self.next_frame.tooltip_requests.len(),
            deferred_draws_index: self.next_frame.deferred_draws.len(),
            dispatch_tree_index: self.next_frame.dispatch_tree.len(),
            accessed_element_states_index: self.next_frame.accessed_element_states.len(),
            line_layout_index: self.text_system.layout_index(),
        }
    }

    pub(crate) fn reuse_prepaint(&mut self, range: Range<PrepaintStateIndex>) {
        self.next_frame.hitboxes.extend(
            self.rendered_frame.hitboxes[range.start.hitboxes_index..range.end.hitboxes_index]
                .iter()
                .cloned(),
        );
        self.next_frame.tooltip_requests.extend(
            self.rendered_frame.tooltip_requests
                [range.start.tooltips_index..range.end.tooltips_index]
                .iter_mut()
                .map(|request| request.take()),
        );
        self.next_frame.accessed_element_states.extend(
            self.rendered_frame.accessed_element_states[range.start.accessed_element_states_index
                ..range.end.accessed_element_states_index]
                .iter()
                .map(|(id, type_id)| (id.clone(), *type_id)),
        );
        self.text_system
            .reuse_layouts(range.start.line_layout_index..range.end.line_layout_index);

        let reused_subtree = self.next_frame.dispatch_tree.reuse_subtree(
            range.start.dispatch_tree_index..range.end.dispatch_tree_index,
            &mut self.rendered_frame.dispatch_tree,
            self.focus,
        );

        if reused_subtree.contains_focus() {
            self.next_frame.focus = self.focus;
        }

        self.next_frame.deferred_draws.extend(
            self.rendered_frame.deferred_draws
                [range.start.deferred_draws_index..range.end.deferred_draws_index]
                .iter()
                .map(|deferred_draw| DeferredDraw {
                    current_view: deferred_draw.current_view,
                    parent_node: reused_subtree.refresh_node_id(deferred_draw.parent_node),
                    element_id_stack: deferred_draw.element_id_stack.clone(),
                    text_style_stack: deferred_draw.text_style_stack.clone(),
                    rem_size: deferred_draw.rem_size,
                    priority: deferred_draw.priority,
                    element: None,
                    absolute_offset: deferred_draw.absolute_offset,
                    prepaint_range: deferred_draw.prepaint_range.clone(),
                    paint_range: deferred_draw.paint_range.clone(),
                }),
        );
    }

    pub(crate) fn paint_index(&self) -> PaintIndex {
        PaintIndex {
            scene_index: self.next_frame.scene.len(),
            mouse_listeners_index: self.next_frame.mouse_listeners.len(),
            input_handlers_index: self.next_frame.input_handlers.len(),
            cursor_styles_index: self.next_frame.cursor_styles.len(),
            accessed_element_states_index: self.next_frame.accessed_element_states.len(),
            tab_handle_index: self.next_frame.tab_stops.paint_index(),
            line_layout_index: self.text_system.layout_index(),
        }
    }

    pub(crate) fn reuse_paint(&mut self, range: Range<PaintIndex>) {
        self.next_frame.cursor_styles.extend(
            self.rendered_frame.cursor_styles
                [range.start.cursor_styles_index..range.end.cursor_styles_index]
                .iter()
                .cloned(),
        );
        self.next_frame.input_handlers.extend(
            self.rendered_frame.input_handlers
                [range.start.input_handlers_index..range.end.input_handlers_index]
                .iter_mut()
                .map(|handler| handler.take()),
        );
        self.next_frame.mouse_listeners.extend(
            self.rendered_frame.mouse_listeners
                [range.start.mouse_listeners_index..range.end.mouse_listeners_index]
                .iter_mut()
                .map(|listener| listener.take()),
        );
        self.next_frame.accessed_element_states.extend(
            self.rendered_frame.accessed_element_states[range.start.accessed_element_states_index
                ..range.end.accessed_element_states_index]
                .iter()
                .map(|(id, type_id)| (id.clone(), *type_id)),
        );
        self.next_frame.tab_stops.replay(
            &self.rendered_frame.tab_stops.insertion_history
                [range.start.tab_handle_index..range.end.tab_handle_index],
        );

        self.text_system
            .reuse_layouts(range.start.line_layout_index..range.end.line_layout_index);
        self.next_frame.scene.replay(
            range.start.scene_index..range.end.scene_index,
            &self.rendered_frame.scene,
        );
    }

    /// Push a text style onto the stack, and call a function with that style active.
    /// Use [`Window::text_style`] to get the current, combined text style. This method
    /// should only be called as part of element drawing.
    // This function is called in a highly recursive manner in editor
    // prepainting, make sure its inlined to reduce the stack burden
    #[inline]
    pub fn with_text_style<F, R>(&mut self, style: Option<TextStyleRefinement>, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        self.invalidator.debug_assert_paint_or_prepaint();
        if let Some(style) = style {
            self.text_style_stack.push(style);
            let result = f(self);
            self.text_style_stack.pop();
            result
        } else {
            f(self)
        }
    }

    /// Updates the cursor style at the platform level. This method should only be called
    /// during the paint phase of element drawing.
    pub fn set_cursor_style(&mut self, style: CursorStyle, hitbox: &Hitbox) {
        self.invalidator.debug_assert_paint();
        self.next_frame.cursor_styles.push(CursorStyleRequest {
            hitbox_id: Some(hitbox.id),
            style,
        });
    }

    /// Updates the cursor style for the entire window at the platform level. A cursor
    /// style using this method will have precedence over any cursor style set using
    /// `set_cursor_style`. This method should only be called during the paint
    /// phase of element drawing.
    pub fn set_window_cursor_style(&mut self, style: CursorStyle) {
        self.invalidator.debug_assert_paint();
        self.next_frame.cursor_styles.push(CursorStyleRequest {
            hitbox_id: None,
            style,
        })
    }

    /// Sets a tooltip to be rendered for the upcoming frame. This method should only be called
    /// during the paint phase of element drawing.
    pub fn set_tooltip(&mut self, tooltip: AnyTooltip) -> TooltipId {
        self.invalidator.debug_assert_prepaint();
        let id = TooltipId(post_inc(&mut self.next_tooltip_id.0));
        self.next_frame
            .tooltip_requests
            .push(Some(TooltipRequest { id, tooltip }));
        id
    }

    /// Invoke the given function with the given content mask after intersecting it
    /// with the current mask. This method should only be called during element drawing.
    // This function is called in a highly recursive manner in editor
    // prepainting, make sure its inlined to reduce the stack burden
    #[inline]
    pub fn with_content_mask<R>(
        &mut self,
        mask: Option<ContentMask<Pixels>>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.invalidator.debug_assert_paint_or_prepaint();
        if let Some(mask) = mask {
            let mask = mask.intersect(&self.content_mask());
            self.content_mask_stack.push(mask);
            let result = f(self);
            self.content_mask_stack.pop();
            result
        } else {
            f(self)
        }
    }

    /// Updates the global element offset relative to the current offset. This is used to implement
    /// scrolling. This method should only be called during the prepaint phase of element drawing.
    pub fn with_element_offset<R>(
        &mut self,
        offset: Point<Pixels>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.invalidator.debug_assert_prepaint();

        if offset.is_zero() {
            return f(self);
        };

        let abs_offset = self.element_offset() + offset;
        self.with_absolute_element_offset(abs_offset, f)
    }

    /// Updates the global element offset based on the given offset. This is used to implement
    /// drag handles and other manual painting of elements. This method should only be called during
    /// the prepaint phase of element drawing.
    pub fn with_absolute_element_offset<R>(
        &mut self,
        offset: Point<Pixels>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.invalidator.debug_assert_prepaint();
        self.element_offset_stack.push(offset);
        let result = f(self);
        self.element_offset_stack.pop();
        result
    }

    pub(crate) fn with_element_opacity<R>(
        &mut self,
        opacity: Option<f32>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.invalidator.debug_assert_paint_or_prepaint();

        let Some(opacity) = opacity else {
            return f(self);
        };

        let previous_opacity = self.element_opacity;
        self.element_opacity = previous_opacity * opacity;
        let result = f(self);
        self.element_opacity = previous_opacity;
        result
    }

    /// Perform prepaint on child elements in a "retryable" manner, so that any side effects
    /// of prepaints can be discarded before prepainting again. This is used to support autoscroll
    /// where we need to prepaint children to detect the autoscroll bounds, then adjust the
    /// element offset and prepaint again. See [`crate::List`] for an example. This method should only be
    /// called during the prepaint phase of element drawing.
    pub fn transact<T, U>(&mut self, f: impl FnOnce(&mut Self) -> Result<T, U>) -> Result<T, U> {
        self.invalidator.debug_assert_prepaint();
        let index = self.prepaint_index();
        let result = f(self);
        if result.is_err() {
            self.next_frame.hitboxes.truncate(index.hitboxes_index);
            self.next_frame
                .tooltip_requests
                .truncate(index.tooltips_index);
            self.next_frame
                .deferred_draws
                .truncate(index.deferred_draws_index);
            self.next_frame
                .dispatch_tree
                .truncate(index.dispatch_tree_index);
            self.next_frame
                .accessed_element_states
                .truncate(index.accessed_element_states_index);
            self.text_system.truncate_layouts(index.line_layout_index);
        }
        result
    }

    /// When you call this method during [`Element::prepaint`], containing elements will attempt to
    /// scroll to cause the specified bounds to become visible. When they decide to autoscroll, they will call
    /// [`Element::prepaint`] again with a new set of bounds. See [`crate::List`] for an example of an element
    /// that supports this method being called on the elements it contains. This method should only be
    /// called during the prepaint phase of element drawing.
    pub fn request_autoscroll(&mut self, bounds: Bounds<Pixels>) {
        self.invalidator.debug_assert_prepaint();
        self.requested_autoscroll = Some(bounds);
    }

    /// This method can be called from a containing element such as [`crate::List`] to support the autoscroll behavior
    /// described in [`Self::request_autoscroll`].
    pub fn take_autoscroll(&mut self) -> Option<Bounds<Pixels>> {
        self.invalidator.debug_assert_prepaint();
        self.requested_autoscroll.take()
    }

    /// Asynchronously load an asset, if the asset hasn't finished loading this will return None.
    /// Your view will be re-drawn once the asset has finished loading.
    ///
    /// Note that the multiple calls to this method will only result in one `Asset::load` call at a
    /// time.
    pub fn use_asset<A: Asset>(&mut self, source: &A::Source, cx: &mut App) -> Option<A::Output> {
        let (task, is_first) = cx.fetch_asset::<A>(source);
        task.clone().now_or_never().or_else(|| {
            if is_first {
                let entity_id = self.current_view();
                self.spawn(cx, {
                    let task = task.clone();
                    async move |cx| {
                        task.await;

                        cx.on_next_frame(move |_, cx| {
                            cx.notify(entity_id);
                        });
                    }
                })
                .detach();
            }

            None
        })
    }

    /// Asynchronously load an asset, if the asset hasn't finished loading or doesn't exist this will return None.
    /// Your view will not be re-drawn once the asset has finished loading.
    ///
    /// Note that the multiple calls to this method will only result in one `Asset::load` call at a
    /// time.
    pub fn get_asset<A: Asset>(&mut self, source: &A::Source, cx: &mut App) -> Option<A::Output> {
        let (task, _) = cx.fetch_asset::<A>(source);
        task.now_or_never()
    }
    /// Obtain the current element offset. This method should only be called during the
    /// prepaint phase of element drawing.
    pub fn element_offset(&self) -> Point<Pixels> {
        self.invalidator.debug_assert_prepaint();
        self.element_offset_stack
            .last()
            .copied()
            .unwrap_or_default()
    }

    /// Obtain the current element opacity. This method should only be called during the
    /// prepaint phase of element drawing.
    #[inline]
    pub(crate) fn element_opacity(&self) -> f32 {
        self.invalidator.debug_assert_paint_or_prepaint();
        self.element_opacity
    }

    /// Obtain the current content mask. This method should only be called during element drawing.
    pub fn content_mask(&self) -> ContentMask<Pixels> {
        self.invalidator.debug_assert_paint_or_prepaint();
        self.content_mask_stack
            .last()
            .cloned()
            .unwrap_or_else(|| ContentMask {
                bounds: Bounds {
                    origin: Point::default(),
                    size: self.viewport_size,
                },
                corner_radii: Corners::default(),
                corner_radii_bounds: Bounds::default(),
            })
    }

    /// Provide elements in the called function with a new namespace in which their identifiers must be unique.
    /// This can be used within a custom element to distinguish multiple sets of child elements.
    pub fn with_element_namespace<R>(
        &mut self,
        element_id: impl Into<ElementId>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.element_id_stack.push(element_id.into());
        let result = f(self);
        self.element_id_stack.pop();
        result
    }

    /// Use a piece of state that exists as long this element is being rendered in consecutive frames.
    pub fn use_keyed_state<S: 'static>(
        &mut self,
        key: impl Into<ElementId>,
        cx: &mut App,
        init: impl FnOnce(&mut Self, &mut Context<S>) -> S,
    ) -> Entity<S> {
        let current_view = self.current_view();
        self.with_global_id(key.into(), |global_id, window| {
            window.with_element_state(global_id, |state: Option<Entity<S>>, window| {
                if let Some(state) = state {
                    (state.clone(), state)
                } else {
                    let new_state = cx.new(|cx| init(window, cx));
                    cx.observe(&new_state, move |_, cx| {
                        cx.notify(current_view);
                    })
                    .detach();
                    (new_state.clone(), new_state)
                }
            })
        })
    }

    /// Use a piece of state that exists as long this element is being rendered in consecutive frames, without needing to specify a key
    ///
    /// NOTE: This method uses the location of the caller to generate an ID for this state.
    ///       If this is not sufficient to identify your state (e.g. you're rendering a list item),
    ///       you can provide a custom ElementID using the `use_keyed_state` method.
    #[track_caller]
    pub fn use_state<S: 'static>(
        &mut self,
        cx: &mut App,
        init: impl FnOnce(&mut Self, &mut Context<S>) -> S,
    ) -> Entity<S> {
        self.use_keyed_state(
            ElementId::CodeLocation(*core::panic::Location::caller()),
            cx,
            init,
        )
    }

    /// Updates or initializes state for an element with the given id that lives across multiple
    /// frames. If an element with this ID existed in the rendered frame, its state will be passed
    /// to the given closure. The state returned by the closure will be stored so it can be referenced
    /// when drawing the next frame. This method should only be called as part of element drawing.
    pub fn with_element_state<S, R>(
        &mut self,
        global_id: &GlobalElementId,
        f: impl FnOnce(Option<S>, &mut Self) -> (R, S),
    ) -> R
    where
        S: 'static,
    {
        self.invalidator.debug_assert_paint_or_prepaint();

        let key = (global_id.clone(), TypeId::of::<S>());
        self.next_frame.accessed_element_states.push(key.clone());

        if let Some(any) = self
            .next_frame
            .element_states
            .remove(&key)
            .or_else(|| self.rendered_frame.element_states.remove(&key))
        {
            let ElementStateBox {
                inner,
                #[cfg(debug_assertions)]
                type_name,
            } = any;
            // Using the extra inner option to avoid needing to reallocate a new box.
            let mut state_box = inner
                .downcast::<Option<S>>()
                .map_err(|_| {
                    #[cfg(debug_assertions)]
                    {
                        anyhow::anyhow!(
                            "invalid element state type for id, requested {:?}, actual: {:?}",
                            std::any::type_name::<S>(),
                            type_name
                        )
                    }

                    #[cfg(not(debug_assertions))]
                    {
                        anyhow::anyhow!(
                            "invalid element state type for id, requested {:?}",
                            std::any::type_name::<S>(),
                        )
                    }
                })
                .unwrap();

            let state = state_box.take().expect(
                "reentrant call to with_element_state for the same state type and element id",
            );
            let (result, state) = f(Some(state), self);
            state_box.replace(state);
            self.next_frame.element_states.insert(
                key,
                ElementStateBox {
                    inner: state_box,
                    #[cfg(debug_assertions)]
                    type_name,
                },
            );
            result
        } else {
            let (result, state) = f(None, self);
            self.next_frame.element_states.insert(
                key,
                ElementStateBox {
                    inner: Box::new(Some(state)),
                    #[cfg(debug_assertions)]
                    type_name: std::any::type_name::<S>(),
                },
            );
            result
        }
    }

    /// A variant of `with_element_state` that allows the element's id to be optional. This is a convenience
    /// method for elements where the element id may or may not be assigned. Prefer using `with_element_state`
    /// when the element is guaranteed to have an id.
    ///
    /// The first option means 'no ID provided'
    /// The second option means 'not yet initialized'
    pub fn with_optional_element_state<S, R>(
        &mut self,
        global_id: Option<&GlobalElementId>,
        f: impl FnOnce(Option<Option<S>>, &mut Self) -> (R, Option<S>),
    ) -> R
    where
        S: 'static,
    {
        self.invalidator.debug_assert_paint_or_prepaint();

        if let Some(global_id) = global_id {
            self.with_element_state(global_id, |state, cx| {
                let (result, state) = f(Some(state), cx);
                let state =
                    state.expect("you must return some state when you pass some element id");
                (result, state)
            })
        } else {
            let (result, state) = f(None, self);
            debug_assert!(
                state.is_none(),
                "you must not return an element state when passing None for the global id"
            );
            result
        }
    }

    /// Executes the given closure within the context of a tab group.
    #[inline]
    pub fn with_tab_group<R>(&mut self, index: Option<isize>, f: impl FnOnce(&mut Self) -> R) -> R {
        if let Some(index) = index {
            self.next_frame.tab_stops.begin_group(index);
            let result = f(self);
            self.next_frame.tab_stops.end_group();
            result
        } else {
            f(self)
        }
    }

    /// Defers the drawing of the given element, scheduling it to be painted on top of the currently-drawn tree
    /// at a later time. The `priority` parameter determines the drawing order relative to other deferred elements,
    /// with higher values being drawn on top.
    ///
    /// This method should only be called as part of the prepaint phase of element drawing.
    pub fn defer_draw(
        &mut self,
        element: AnyElement,
        absolute_offset: Point<Pixels>,
        priority: usize,
    ) {
        self.invalidator.debug_assert_prepaint();
        let parent_node = self.next_frame.dispatch_tree.active_node_id().unwrap();
        self.next_frame.deferred_draws.push(DeferredDraw {
            current_view: self.current_view(),
            parent_node,
            element_id_stack: self.element_id_stack.clone(),
            text_style_stack: self.text_style_stack.clone(),
            rem_size: self.rem_size(),
            priority,
            element: Some(element),
            absolute_offset,
            prepaint_range: PrepaintStateIndex::default()..PrepaintStateIndex::default(),
            paint_range: PaintIndex::default()..PaintIndex::default(),
        });
    }

    /// Creates a new painting layer for the specified bounds. A "layer" is a batch
    /// of geometry that are non-overlapping and have the same draw order. This is typically used
    /// for performance reasons.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn paint_layer<R>(&mut self, bounds: Bounds<Pixels>, f: impl FnOnce(&mut Self) -> R) -> R {
        self.invalidator.debug_assert_paint();

        let scale_factor = self.scale_factor();
        let content_mask = self.content_mask();
        let clipped_bounds = bounds.intersect(&content_mask.bounds);
        if !clipped_bounds.is_empty() {
            self.next_frame
                .scene
                .push_layer(clipped_bounds.scale(scale_factor));
        }

        let result = f(self);

        if !clipped_bounds.is_empty() {
            self.next_frame.scene.pop_layer();
        }

        result
    }

    /// Paint one or more drop shadows into the scene for the next frame at the current z-index.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn paint_shadows(
        &mut self,
        bounds: Bounds<Pixels>,
        corner_radii: Corners<Pixels>,
        shadows: &[BoxShadow],
    ) {
        self.invalidator.debug_assert_paint();

        let scale_factor = self.scale_factor();
        let content_mask = self.content_mask();
        let opacity = self.element_opacity();
        for shadow in shadows {
            let shadow_bounds = (bounds + shadow.offset).dilate(shadow.spread_radius);
            self.next_frame.scene.insert_primitive(Shadow {
                order: 0,
                blur_radius: shadow.blur_radius.scale(scale_factor),
                bounds: shadow_bounds.scale(scale_factor),
                content_mask: content_mask.scale(scale_factor),
                corner_radii: corner_radii.scale(scale_factor),
                color: shadow.color.opacity(opacity),
            });
        }
    }

    /// Paint one or more quads into the scene for the next frame at the current stacking context.
    /// Quads are colored rectangular regions with an optional background, border, and corner radius.
    /// see [`fill`], [`outline`], and [`quad`] to construct this type.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    ///
    /// Note that the `quad.corner_radii` are allowed to exceed the bounds, creating sharp corners
    /// where the circular arcs meet. This will not display well when combined with dashed borders.
    /// Use `Corners::clamp_radii_for_quad_size` if the radii should fit within the bounds.
    pub fn paint_quad(&mut self, quad: PaintQuad) {
        self.invalidator.debug_assert_paint();

        let scale_factor = self.scale_factor();
        let content_mask = self.content_mask();
        let opacity = self.element_opacity();
        self.next_frame.scene.insert_primitive(Quad {
            order: 0,
            bounds: quad.bounds.scale(scale_factor),
            content_mask: content_mask.scale(scale_factor),
            background: quad.background.opacity(opacity),
            border_color: quad.border_color.opacity(opacity),
            corner_radii: quad.corner_radii.scale(scale_factor),
            border_widths: quad.border_widths.scale(scale_factor),
            border_style: quad.border_style,
        });
    }

    /// Paint the given `Path` into the scene for the next frame at the current z-index.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn paint_path(&mut self, mut path: Path<Pixels>, color: impl Into<Background>) {
        self.invalidator.debug_assert_paint();

        let scale_factor = self.scale_factor();
        let content_mask = self.content_mask();
        let opacity = self.element_opacity();
        path.content_mask = content_mask;
        let color: Background = color.into();
        path.color = color.opacity(opacity);
        self.next_frame
            .scene
            .insert_primitive(path.scale(scale_factor));
    }

    /// Paint an underline into the scene for the next frame at the current z-index.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn paint_underline(
        &mut self,
        origin: Point<Pixels>,
        width: Pixels,
        style: &UnderlineStyle,
    ) {
        self.invalidator.debug_assert_paint();

        let scale_factor = self.scale_factor();
        let height = if style.wavy {
            style.thickness * 3.
        } else {
            style.thickness
        };
        let bounds = Bounds {
            origin,
            size: size(width, height),
        };
        let content_mask = self.content_mask();
        let element_opacity = self.element_opacity();

        self.next_frame.scene.insert_primitive(Underline {
            order: 0,
            pad: 0,
            bounds: bounds.scale(scale_factor),
            content_mask: content_mask.scale(scale_factor),
            color: style.color.unwrap_or_default().opacity(element_opacity),
            thickness: style.thickness.scale(scale_factor),
            wavy: if style.wavy { 1 } else { 0 },
        });
    }

    /// Paint a strikethrough into the scene for the next frame at the current z-index.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn paint_strikethrough(
        &mut self,
        origin: Point<Pixels>,
        width: Pixels,
        style: &StrikethroughStyle,
    ) {
        self.invalidator.debug_assert_paint();

        let scale_factor = self.scale_factor();
        let height = style.thickness;
        let bounds = Bounds {
            origin,
            size: size(width, height),
        };
        let content_mask = self.content_mask();
        let opacity = self.element_opacity();

        self.next_frame.scene.insert_primitive(Underline {
            order: 0,
            pad: 0,
            bounds: bounds.scale(scale_factor),
            content_mask: content_mask.scale(scale_factor),
            thickness: style.thickness.scale(scale_factor),
            color: style.color.unwrap_or_default().opacity(opacity),
            wavy: 0,
        });
    }

    /// Paints a monochrome (non-emoji) glyph into the scene for the next frame at the current z-index.
    ///
    /// The y component of the origin is the baseline of the glyph.
    /// You should generally prefer to use the [`ShapedLine::paint`](crate::ShapedLine::paint) or
    /// [`WrappedLine::paint`](crate::WrappedLine::paint) methods in the [`TextSystem`](crate::TextSystem).
    /// This method is only useful if you need to paint a single glyph that has already been shaped.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn paint_glyph(
        &mut self,
        origin: Point<Pixels>,
        font_id: FontId,
        glyph_id: GlyphId,
        font_size: Pixels,
        color: Hsla,
    ) -> Result<()> {
        self.invalidator.debug_assert_paint();

        let element_opacity = self.element_opacity();
        let scale_factor = self.scale_factor();
        let glyph_origin = origin.scale(scale_factor);

        let subpixel_variant = Point {
            x: (glyph_origin.x.0.fract() * SUBPIXEL_VARIANTS_X as f32).floor() as u8,
            y: (glyph_origin.y.0.fract() * SUBPIXEL_VARIANTS_Y as f32).floor() as u8,
        };
        let subpixel_rendering = self.should_use_subpixel_rendering(font_id, font_size);
        let params = RenderGlyphParams {
            font_id,
            glyph_id,
            font_size,
            subpixel_variant,
            scale_factor,
            is_emoji: false,
            subpixel_rendering,
        };

        let raster_bounds = self.text_system().raster_bounds(&params)?;
        if !raster_bounds.is_zero() {
            let tile = self
                .sprite_atlas
                .get_or_insert_with(&params.clone().into(), &mut || {
                    let (size, bytes) = self.text_system().rasterize_glyph(&params)?;
                    Ok(Some((size, Cow::Owned(bytes))))
                })?
                .expect("Callback above only errors or returns Some");
            let bounds = Bounds {
                origin: glyph_origin.map(|px| px.floor()) + raster_bounds.origin.map(Into::into),
                size: tile.bounds.size.map(Into::into),
            };
            let content_mask = self.content_mask().scale(scale_factor);

            if subpixel_rendering {
                self.next_frame.scene.insert_primitive(SubpixelSprite {
                    order: 0,
                    pad: 0,
                    bounds,
                    content_mask,
                    color: color.opacity(element_opacity),
                    tile,
                    transformation: TransformationMatrix::unit(),
                });
            } else {
                self.next_frame.scene.insert_primitive(MonochromeSprite {
                    order: 0,
                    pad: 0,
                    bounds,
                    content_mask,
                    color: color.opacity(element_opacity),
                    tile,
                    transformation: TransformationMatrix::unit(),
                });
            }
        }
        Ok(())
    }

    fn should_use_subpixel_rendering(&self, font_id: FontId, font_size: Pixels) -> bool {
        if self.platform_window.background_appearance() != WindowBackgroundAppearance::Opaque {
            return false;
        }

        if !self.platform_window.is_subpixel_rendering_supported() {
            return false;
        }

        let mode = match self.text_rendering_mode.get() {
            TextRenderingMode::PlatformDefault => self
                .text_system()
                .recommended_rendering_mode(font_id, font_size),
            mode => mode,
        };

        mode == TextRenderingMode::Subpixel
    }

    /// Paints an emoji glyph into the scene for the next frame at the current z-index.
    ///
    /// The y component of the origin is the baseline of the glyph.
    /// You should generally prefer to use the [`ShapedLine::paint`](crate::ShapedLine::paint) or
    /// [`WrappedLine::paint`](crate::WrappedLine::paint) methods in the [`TextSystem`](crate::TextSystem).
    /// This method is only useful if you need to paint a single emoji that has already been shaped.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn paint_emoji(
        &mut self,
        origin: Point<Pixels>,
        font_id: FontId,
        glyph_id: GlyphId,
        font_size: Pixels,
    ) -> Result<()> {
        self.invalidator.debug_assert_paint();

        let scale_factor = self.scale_factor();
        let glyph_origin = origin.scale(scale_factor);
        let params = RenderGlyphParams {
            font_id,
            glyph_id,
            font_size,
            // We don't render emojis with subpixel variants.
            subpixel_variant: Default::default(),
            scale_factor,
            is_emoji: true,
            subpixel_rendering: false,
        };

        let raster_bounds = self.text_system().raster_bounds(&params)?;
        if !raster_bounds.is_zero() {
            let tile = self
                .sprite_atlas
                .get_or_insert_with(&params.clone().into(), &mut || {
                    let (size, bytes) = self.text_system().rasterize_glyph(&params)?;
                    Ok(Some((size, Cow::Owned(bytes))))
                })?
                .expect("Callback above only errors or returns Some");

            let bounds = Bounds {
                origin: glyph_origin.map(|px| px.floor()) + raster_bounds.origin.map(Into::into),
                size: tile.bounds.size.map(Into::into),
            };
            let content_mask = self.content_mask().scale(scale_factor);
            let opacity = self.element_opacity();

            self.next_frame.scene.insert_primitive(PolychromeSprite {
                order: 0,
                pad: 0,
                grayscale: false,
                bounds,
                corner_radii: Default::default(),
                content_mask,
                tile,
                opacity,
            });
        }
        Ok(())
    }

    /// Paint a monochrome SVG into the scene for the next frame at the current stacking context.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn paint_svg(
        &mut self,
        bounds: Bounds<Pixels>,
        path: SharedString,
        mut data: Option<&[u8]>,
        transformation: TransformationMatrix,
        color: Hsla,
        cx: &App,
    ) -> Result<()> {
        self.invalidator.debug_assert_paint();

        let element_opacity = self.element_opacity();
        let scale_factor = self.scale_factor();

        let bounds = bounds.scale(scale_factor);
        let params = RenderSvgParams {
            path,
            size: bounds.size.map(|pixels| {
                DevicePixels::from((pixels.0 * SMOOTH_SVG_SCALE_FACTOR).ceil() as i32)
            }),
        };

        let Some(tile) =
            self.sprite_atlas
                .get_or_insert_with(&params.clone().into(), &mut || {
                    let Some((size, bytes)) = cx.svg_renderer.render_alpha_mask(&params, data)?
                    else {
                        return Ok(None);
                    };
                    Ok(Some((size, Cow::Owned(bytes))))
                })?
        else {
            return Ok(());
        };
        let content_mask = self.content_mask().scale(scale_factor);
        let svg_bounds = Bounds {
            origin: bounds.center()
                - Point::new(
                    ScaledPixels(tile.bounds.size.width.0 as f32 / SMOOTH_SVG_SCALE_FACTOR / 2.),
                    ScaledPixels(tile.bounds.size.height.0 as f32 / SMOOTH_SVG_SCALE_FACTOR / 2.),
                ),
            size: tile
                .bounds
                .size
                .map(|value| ScaledPixels(value.0 as f32 / SMOOTH_SVG_SCALE_FACTOR)),
        };

        self.next_frame.scene.insert_primitive(MonochromeSprite {
            order: 0,
            pad: 0,
            bounds: svg_bounds
                .map_origin(|origin| origin.round())
                .map_size(|size| size.ceil()),
            content_mask,
            color: color.opacity(element_opacity),
            tile,
            transformation,
        });

        Ok(())
    }

    /// Paint an image into the scene for the next frame at the current z-index.
    /// This method will panic if the frame_index is not valid
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn paint_image(
        &mut self,
        bounds: Bounds<Pixels>,
        corner_radii: Corners<Pixels>,
        data: Arc<RenderImage>,
        frame_index: usize,
        grayscale: bool,
    ) -> Result<()> {
        self.invalidator.debug_assert_paint();

        let scale_factor = self.scale_factor();
        let bounds = bounds.scale(scale_factor);
        let params = RenderImageParams {
            image_id: data.id,
            frame_index,
        };

        let tile = self
            .sprite_atlas
            .get_or_insert_with(&params.into(), &mut || {
                Ok(Some((
                    data.size(frame_index),
                    Cow::Borrowed(
                        data.as_bytes(frame_index)
                            .expect("It's the caller's job to pass a valid frame index"),
                    ),
                )))
            })?
            .expect("Callback above only returns Some");
        let content_mask = self.content_mask().scale(scale_factor);
        let corner_radii = corner_radii.scale(scale_factor);
        let opacity = self.element_opacity();

        self.next_frame.scene.insert_primitive(PolychromeSprite {
            order: 0,
            pad: 0,
            grayscale,
            bounds: bounds
                .map_origin(|origin| origin.floor())
                .map_size(|size| size.ceil()),
            content_mask,
            corner_radii,
            tile,
            opacity,
        });
        Ok(())
    }

    /// Paint a surface into the scene for the next frame at the current z-index.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    #[cfg(target_os = "macos")]
    pub fn paint_surface(&mut self, bounds: Bounds<Pixels>, image_buffer: CVPixelBuffer) {
        use crate::PaintSurface;

        self.invalidator.debug_assert_paint();

        let scale_factor = self.scale_factor();
        let bounds = bounds.scale(scale_factor);
        let content_mask = self.content_mask().scale(scale_factor);
        self.next_frame.scene.insert_primitive(PaintSurface {
            order: 0,
            bounds,
            content_mask,
            image_buffer,
        });
    }

    /// Removes an image from the sprite atlas.
    pub fn drop_image(&mut self, data: Arc<RenderImage>) -> Result<()> {
        for frame_index in 0..data.frame_count() {
            let params = RenderImageParams {
                image_id: data.id,
                frame_index,
            };

            self.sprite_atlas.remove(&params.clone().into());
        }

        Ok(())
    }

    /// Add a node to the layout tree for the current frame. Takes the `Style` of the element for which
    /// layout is being requested, along with the layout ids of any children. This method is called during
    /// calls to the [`Element::request_layout`] trait method and enables any element to participate in layout.
    ///
    /// This method should only be called as part of the request_layout or prepaint phase of element drawing.
    #[must_use]
    pub fn request_layout(
        &mut self,
        style: Style,
        children: impl IntoIterator<Item = LayoutId>,
        cx: &mut App,
    ) -> LayoutId {
        self.invalidator.debug_assert_prepaint();

        cx.layout_id_buffer.clear();
        cx.layout_id_buffer.extend(children);
        let rem_size = self.rem_size();
        let scale_factor = self.scale_factor();

        self.layout_engine.as_mut().unwrap().request_layout(
            style,
            rem_size,
            scale_factor,
            &cx.layout_id_buffer,
        )
    }

    /// Add a node to the layout tree for the current frame. Instead of taking a `Style` and children,
    /// this variant takes a function that is invoked during layout so you can use arbitrary logic to
    /// determine the element's size. One place this is used internally is when measuring text.
    ///
    /// The given closure is invoked at layout time with the known dimensions and available space and
    /// returns a `Size`.
    ///
    /// This method should only be called as part of the request_layout or prepaint phase of element drawing.
    pub fn request_measured_layout<F>(&mut self, style: Style, measure: F) -> LayoutId
    where
        F: Fn(Size<Option<Pixels>>, Size<AvailableSpace>, &mut Window, &mut App) -> Size<Pixels>
            + 'static,
    {
        self.invalidator.debug_assert_prepaint();

        let rem_size = self.rem_size();
        let scale_factor = self.scale_factor();
        self.layout_engine
            .as_mut()
            .unwrap()
            .request_measured_layout(style, rem_size, scale_factor, measure)
    }

    /// Compute the layout for the given id within the given available space.
    /// This method is called for its side effect, typically by the framework prior to painting.
    /// After calling it, you can request the bounds of the given layout node id or any descendant.
    ///
    /// This method should only be called as part of the prepaint phase of element drawing.
    pub fn compute_layout(
        &mut self,
        layout_id: LayoutId,
        available_space: Size<AvailableSpace>,
        cx: &mut App,
    ) {
        self.invalidator.debug_assert_prepaint();

        let mut layout_engine = self.layout_engine.take().unwrap();
        layout_engine.compute_layout(layout_id, available_space, self, cx);
        self.layout_engine = Some(layout_engine);
    }

    /// Obtain the bounds computed for the given LayoutId relative to the window. This method will usually be invoked by
    /// GPUI itself automatically in order to pass your element its `Bounds` automatically.
    ///
    /// This method should only be called as part of element drawing.
    pub fn layout_bounds(&mut self, layout_id: LayoutId) -> Bounds<Pixels> {
        self.invalidator.debug_assert_prepaint();

        let scale_factor = self.scale_factor();
        let mut bounds = self
            .layout_engine
            .as_mut()
            .unwrap()
            .layout_bounds(layout_id, scale_factor)
            .map(Into::into);
        bounds.origin += self.element_offset();
        bounds
    }

    /// This method should be called during `prepaint`. You can use
    /// the returned [Hitbox] during `paint` or in an event handler
    /// to determine whether the inserted hitbox was the topmost.
    ///
    /// This method should only be called as part of the prepaint phase of element drawing.
    pub fn insert_hitbox(&mut self, bounds: Bounds<Pixels>, behavior: HitboxBehavior) -> Hitbox {
        self.invalidator.debug_assert_prepaint();

        let content_mask = self.content_mask();
        let mut id = self.next_hitbox_id;
        self.next_hitbox_id = self.next_hitbox_id.next();
        let hitbox = Hitbox {
            id,
            bounds,
            content_mask,
            behavior,
        };
        self.next_frame.hitboxes.push(hitbox.clone());
        hitbox
    }

    /// Set a hitbox which will act as a control area of the platform window.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn insert_window_control_hitbox(&mut self, area: WindowControlArea, hitbox: Hitbox) {
        self.invalidator.debug_assert_paint();
        self.next_frame.window_control_hitboxes.push((area, hitbox));
    }

    /// Sets the key context for the current element. This context will be used to translate
    /// keybindings into actions.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn set_key_context(&mut self, context: KeyContext) {
        self.invalidator.debug_assert_paint();
        self.next_frame.dispatch_tree.set_key_context(context);
    }

    /// Sets the focus handle for the current element. This handle will be used to manage focus state
    /// and keyboard event dispatch for the element.
    ///
    /// This method should only be called as part of the prepaint phase of element drawing.
    pub fn set_focus_handle(&mut self, focus_handle: &FocusHandle, _: &App) {
        self.invalidator.debug_assert_prepaint();
        if focus_handle.is_focused(self) {
            self.next_frame.focus = Some(focus_handle.id);
        }
        self.next_frame.dispatch_tree.set_focus_id(focus_handle.id);
    }

    /// Sets the view id for the current element, which will be used to manage view caching.
    ///
    /// This method should only be called as part of element prepaint. We plan on removing this
    /// method eventually when we solve some issues that require us to construct editor elements
    /// directly instead of always using editors via views.
    pub fn set_view_id(&mut self, view_id: EntityId) {
        self.invalidator.debug_assert_prepaint();
        self.next_frame.dispatch_tree.set_view_id(view_id);
    }

    /// Get the entity ID for the currently rendering view
    pub fn current_view(&self) -> EntityId {
        self.invalidator.debug_assert_paint_or_prepaint();
        self.rendered_entity_stack.last().copied().unwrap()
    }

    #[inline]
    pub(crate) fn with_rendered_view<R>(
        &mut self,
        id: EntityId,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.rendered_entity_stack.push(id);
        let result = f(self);
        self.rendered_entity_stack.pop();
        result
    }

    /// Executes the provided function with the specified image cache.
    pub fn with_image_cache<F, R>(&mut self, image_cache: Option<AnyImageCache>, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        if let Some(image_cache) = image_cache {
            self.image_cache_stack.push(image_cache);
            let result = f(self);
            self.image_cache_stack.pop();
            result
        } else {
            f(self)
        }
    }

    /// Sets an input handler, such as [`ElementInputHandler`][element_input_handler], which interfaces with the
    /// platform to receive textual input with proper integration with concerns such
    /// as IME interactions. This handler will be active for the upcoming frame until the following frame is
    /// rendered.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    ///
    /// [element_input_handler]: crate::ElementInputHandler
    pub fn handle_input(
        &mut self,
        focus_handle: &FocusHandle,
        input_handler: impl InputHandler,
        cx: &App,
    ) {
        self.invalidator.debug_assert_paint();

        if focus_handle.is_focused(self) {
            let cx = self.to_async(cx);
            self.next_frame
                .input_handlers
                .push(Some(PlatformInputHandler::new(cx, Box::new(input_handler))));
        }
    }

    /// Register a mouse event listener on the window for the next frame. The type of event
    /// is determined by the first parameter of the given listener. When the next frame is rendered
    /// the listener will be cleared.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn on_mouse_event<Event: MouseEvent>(
        &mut self,
        mut listener: impl FnMut(&Event, DispatchPhase, &mut Window, &mut App) + 'static,
    ) {
        self.invalidator.debug_assert_paint();

        self.next_frame.mouse_listeners.push(Some(Box::new(
            move |event: &dyn Any, phase: DispatchPhase, window: &mut Window, cx: &mut App| {
                if let Some(event) = event.downcast_ref() {
                    listener(event, phase, window, cx)
                }
            },
        )));
    }

    /// Register a key event listener on this node for the next frame. The type of event
    /// is determined by the first parameter of the given listener. When the next frame is rendered
    /// the listener will be cleared.
    ///
    /// This is a fairly low-level method, so prefer using event handlers on elements unless you have
    /// a specific need to register a listener yourself.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn on_key_event<Event: KeyEvent>(
        &mut self,
        listener: impl Fn(&Event, DispatchPhase, &mut Window, &mut App) + 'static,
    ) {
        self.invalidator.debug_assert_paint();

        self.next_frame.dispatch_tree.on_key_event(Rc::new(
            move |event: &dyn Any, phase, window: &mut Window, cx: &mut App| {
                if let Some(event) = event.downcast_ref::<Event>() {
                    listener(event, phase, window, cx)
                }
            },
        ));
    }

    /// Register a modifiers changed event listener on the window for the next frame.
    ///
    /// This is a fairly low-level method, so prefer using event handlers on elements unless you have
    /// a specific need to register a global listener.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn on_modifiers_changed(
        &mut self,
        listener: impl Fn(&ModifiersChangedEvent, &mut Window, &mut App) + 'static,
    ) {
        self.invalidator.debug_assert_paint();

        self.next_frame.dispatch_tree.on_modifiers_changed(Rc::new(
            move |event: &ModifiersChangedEvent, window: &mut Window, cx: &mut App| {
                listener(event, window, cx)
            },
        ));
    }

    /// Register a listener to be called when the given focus handle or one of its descendants receives focus.
    /// This does not fire if the given focus handle - or one of its descendants - was previously focused.
    /// Returns a subscription and persists until the subscription is dropped.
    pub fn on_focus_in(
        &mut self,
        handle: &FocusHandle,
        cx: &mut App,
        mut listener: impl FnMut(&mut Window, &mut App) + 'static,
    ) -> Subscription {
        let focus_id = handle.id;
        let (subscription, activate) =
            self.new_focus_listener(Box::new(move |event, window, cx| {
                if event.is_focus_in(focus_id) {
                    listener(window, cx);
                }
                true
            }));
        cx.defer(move |_| activate());
        subscription
    }

    /// Register a listener to be called when the given focus handle or one of its descendants loses focus.
    /// Returns a subscription and persists until the subscription is dropped.
    pub fn on_focus_out(
        &mut self,
        handle: &FocusHandle,
        cx: &mut App,
        mut listener: impl FnMut(FocusOutEvent, &mut Window, &mut App) + 'static,
    ) -> Subscription {
        let focus_id = handle.id;
        let (subscription, activate) =
            self.new_focus_listener(Box::new(move |event, window, cx| {
                if let Some(blurred_id) = event.previous_focus_path.last().copied()
                    && event.is_focus_out(focus_id)
                {
                    let event = FocusOutEvent {
                        blurred: WeakFocusHandle {
                            id: blurred_id,
                            handles: Arc::downgrade(&cx.focus_handles),
                        },
                    };
                    listener(event, window, cx)
                }
                true
            }));
        cx.defer(move |_| activate());
        subscription
    }

    fn reset_cursor_style(&self, cx: &mut App) {
        // Set the cursor only if we're the active window.
        if self.is_window_hovered() {
            let style = self
                .rendered_frame
                .cursor_style(self)
                .unwrap_or(CursorStyle::Arrow);
            cx.platform.set_cursor_style(style);
        }
    }

    /// Dispatch a given keystroke as though the user had typed it.
    /// You can create a keystroke with Keystroke::parse("").
    pub fn dispatch_keystroke(&mut self, keystroke: Keystroke, cx: &mut App) -> bool {
        let keystroke = keystroke.with_simulated_ime();
        let result = self.dispatch_event(
            PlatformInput::KeyDown(KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            }),
            cx,
        );
        if !result.propagate {
            return true;
        }

        if let Some(input) = keystroke.key_char
            && let Some(mut input_handler) = self.platform_window.take_input_handler()
        {
            input_handler.dispatch_input(&input, self, cx);
            self.platform_window.set_input_handler(input_handler);
            return true;
        }

        false
    }

    /// Return a key binding string for an action, to display in the UI. Uses the highest precedence
    /// binding for the action (last binding added to the keymap).
    pub fn keystroke_text_for(&self, action: &dyn Action) -> String {
        self.highest_precedence_binding_for_action(action)
            .map(|binding| {
                binding
                    .keystrokes()
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_else(|| action.name().to_string())
    }

    /// Dispatch a mouse or keyboard event on the window.
    #[profiling::function]
    pub fn dispatch_event(&mut self, event: PlatformInput, cx: &mut App) -> DispatchEventResult {
        // Track whether this input was keyboard-based for focus-visible styling
        self.last_input_modality = match &event {
            PlatformInput::KeyDown(_) | PlatformInput::ModifiersChanged(_) => {
                InputModality::Keyboard
            }
            PlatformInput::MouseDown(e) if e.is_focusing() => InputModality::Mouse,
            _ => self.last_input_modality,
        };

        // Handlers may set this to false by calling `stop_propagation`.
        cx.propagate_event = true;
        // Handlers may set this to true by calling `prevent_default`.
        self.default_prevented = false;

        let event = match event {
            // Track the mouse position with our own state, since accessing the platform
            // API for the mouse position can only occur on the main thread.
            PlatformInput::MouseMove(mouse_move) => {
                self.mouse_position = mouse_move.position;
                self.modifiers = mouse_move.modifiers;
                PlatformInput::MouseMove(mouse_move)
            }
            PlatformInput::MouseDown(mouse_down) => {
                self.mouse_position = mouse_down.position;
                self.modifiers = mouse_down.modifiers;
                PlatformInput::MouseDown(mouse_down)
            }
            PlatformInput::MouseUp(mouse_up) => {
                self.mouse_position = mouse_up.position;
                self.modifiers = mouse_up.modifiers;
                PlatformInput::MouseUp(mouse_up)
            }
            PlatformInput::MousePressure(mouse_pressure) => {
                PlatformInput::MousePressure(mouse_pressure)
            }
            PlatformInput::MouseExited(mouse_exited) => {
                self.modifiers = mouse_exited.modifiers;
                PlatformInput::MouseExited(mouse_exited)
            }
            PlatformInput::ModifiersChanged(modifiers_changed) => {
                self.modifiers = modifiers_changed.modifiers;
                self.capslock = modifiers_changed.capslock;
                PlatformInput::ModifiersChanged(modifiers_changed)
            }
            PlatformInput::ScrollWheel(scroll_wheel) => {
                self.mouse_position = scroll_wheel.position;
                self.modifiers = scroll_wheel.modifiers;
                PlatformInput::ScrollWheel(scroll_wheel)
            }
            PlatformInput::Pinch(pinch) => {
                self.mouse_position = pinch.center;
                PlatformInput::Pinch(pinch)
            }
            PlatformInput::Rotation(rotation) => {
                self.mouse_position = rotation.center;
                PlatformInput::Rotation(rotation)
            }
            // Translate dragging and dropping of external files from the operating system
            // to internal drag and drop events.
            PlatformInput::FileDrop(file_drop) => match file_drop {
                FileDropEvent::Entered { position, paths } => {
                    self.mouse_position = position;
                    if cx.active_drag.is_none() {
                        cx.active_drag = Some(AnyDrag {
                            value: Arc::new(paths.clone()),
                            view: cx.new(|_| paths).into(),
                            cursor_offset: position,
                            cursor_style: None,
                        });
                    }
                    PlatformInput::MouseMove(MouseMoveEvent {
                        position,
                        pressed_button: Some(MouseButton::Left),
                        modifiers: Modifiers::default(),
                    })
                }
                FileDropEvent::Pending { position } => {
                    self.mouse_position = position;
                    PlatformInput::MouseMove(MouseMoveEvent {
                        position,
                        pressed_button: Some(MouseButton::Left),
                        modifiers: Modifiers::default(),
                    })
                }
                FileDropEvent::Submit { position } => {
                    cx.activate(true);
                    self.mouse_position = position;
                    PlatformInput::MouseUp(MouseUpEvent {
                        button: MouseButton::Left,
                        position,
                        modifiers: Modifiers::default(),
                        click_count: 1,
                    })
                }
                FileDropEvent::Exited => {
                    cx.active_drag.take();
                    PlatformInput::FileDrop(FileDropEvent::Exited)
                }
            },
            PlatformInput::KeyDown(_) | PlatformInput::KeyUp(_) => event,
        };

        if let Some(any_mouse_event) = event.mouse_event() {
            self.dispatch_mouse_event(any_mouse_event, cx);
        } else if let Some(any_key_event) = event.keyboard_event() {
            self.dispatch_key_event(any_key_event, cx);
        }

        if self.invalidator.is_dirty() {
            self.input_rate_tracker.borrow_mut().record_input();
        }

        DispatchEventResult {
            propagate: cx.propagate_event,
            default_prevented: self.default_prevented,
        }
    }

    fn dispatch_mouse_event(&mut self, event: &dyn Any, cx: &mut App) {
        let hit_test = self.rendered_frame.hit_test(self.mouse_position());
        if hit_test != self.mouse_hit_test {
            self.mouse_hit_test = hit_test;
            self.reset_cursor_style(cx);
        }

        #[cfg(any(feature = "inspector", debug_assertions))]
        if self.is_inspector_picking(cx) {
            self.handle_inspector_mouse_event(event, cx);
            // When inspector is picking, all other mouse handling is skipped.
            return;
        }

        let mut mouse_listeners = mem::take(&mut self.rendered_frame.mouse_listeners);

        // Capture phase, events bubble from back to front. Handlers for this phase are used for
        // special purposes, such as detecting events outside of a given Bounds.
        for listener in &mut mouse_listeners {
            let listener = listener.as_mut().unwrap();
            listener(event, DispatchPhase::Capture, self, cx);
            if !cx.propagate_event {
                break;
            }
        }

        // Bubble phase, where most normal handlers do their work.
        if cx.propagate_event {
            for listener in mouse_listeners.iter_mut().rev() {
                let listener = listener.as_mut().unwrap();
                listener(event, DispatchPhase::Bubble, self, cx);
                if !cx.propagate_event {
                    break;
                }
            }
        }

        self.rendered_frame.mouse_listeners = mouse_listeners;

        if cx.has_active_drag() {
            if event.is::<MouseMoveEvent>() {
                // If this was a mouse move event, redraw the window so that the
                // active drag can follow the mouse cursor.
                self.refresh();
            } else if event.is::<MouseUpEvent>() {
                // If this was a mouse up event, cancel the active drag and redraw
                // the window.
                cx.active_drag = None;
                self.refresh();
            }
        }
    }

    /// Dispatch a mouse event that originated from a secondary rendering
    /// surface. Temporarily swaps the surface's hitboxes and mouse position
    /// into the window so that `Hitbox::is_hovered` and friends work
    /// correctly for listeners registered by the surface's element tree.
    #[cfg(target_os = "macos")]
    fn dispatch_surface_event(
        &mut self,
        native_view_ptr: *mut std::ffi::c_void,
        event: PlatformInput,
        cx: &mut App,
    ) {
        // Find the surface that owns this native view.
        let surface_id = self.surfaces.iter().find_map(|(id, s)| {
            if s.gpui_surface.native_view_ptr() == native_view_ptr {
                Some(*id)
            } else {
                None
            }
        });
        let Some(surface_id) = surface_id else { return };

        // Extract the mouse position from the event.
        let event_position = match &event {
            PlatformInput::MouseDown(e) => e.position,
            PlatformInput::MouseUp(e) => e.position,
            PlatformInput::MouseMove(e) => e.position,
            PlatformInput::ScrollWheel(e) => e.position,
            PlatformInput::MouseExited(_) => self.mouse_position,
            _ => return,
        };

        cx.propagate_event = true;

        // Save main frame state that we'll temporarily override.
        let saved_mouse_position = self.mouse_position;
        let saved_hit_test = self.mouse_hit_test.clone();

        // Set the mouse position to the surface-local coordinates.
        self.mouse_position = event_position;

        // Compute hit test against the surface's hitboxes.
        let surface = self.surfaces.get(&surface_id).unwrap();
        let hit_test = {
            let mut set_hover_hitbox_count = false;
            let mut ht = HitTest::default();
            for hitbox in surface.hitboxes.iter().rev() {
                let bounds = hitbox.bounds.intersect(&hitbox.content_mask.bounds);
                if bounds.contains(&event_position) {
                    ht.ids.push(hitbox.id);
                    if !set_hover_hitbox_count
                        && hitbox.behavior == HitboxBehavior::BlockMouseExceptScroll
                    {
                        ht.hover_hitbox_count = ht.ids.len();
                        set_hover_hitbox_count = true;
                    }
                    if hitbox.behavior == HitboxBehavior::BlockMouse {
                        break;
                    }
                }
            }
            if !set_hover_hitbox_count {
                ht.hover_hitbox_count = ht.ids.len();
            }
            ht
        };
        self.mouse_hit_test = hit_test;

        // Take the surface's mouse listeners for dispatch.
        let surface = self.surfaces.get_mut(&surface_id).unwrap();
        let mut mouse_listeners = mem::take(&mut surface.mouse_listeners);

        if let Some(any_mouse_event) = event.mouse_event() {
            // Capture phase (front to back).
            for listener in &mut mouse_listeners {
                let listener = listener.as_mut().unwrap();
                listener(any_mouse_event, DispatchPhase::Capture, self, cx);
                if !cx.propagate_event {
                    break;
                }
            }

            // Bubble phase (back to front).
            if cx.propagate_event {
                for listener in mouse_listeners.iter_mut().rev() {
                    let listener = listener.as_mut().unwrap();
                    listener(any_mouse_event, DispatchPhase::Bubble, self, cx);
                    if !cx.propagate_event {
                        break;
                    }
                }
            }
        }

        // Put listeners back and persist the surface's hover state so that
        // is_hovered() works correctly during the next surface render.
        let hit_test_changed = {
            let surface = self.surfaces.get_mut(&surface_id).unwrap();
            surface.mouse_listeners = mouse_listeners;
            let changed = surface.mouse_hit_test != self.mouse_hit_test;
            surface.mouse_position = event_position;
            surface.mouse_hit_test = self.mouse_hit_test.clone();
            changed
        };

        // Restore main frame state.
        self.mouse_position = saved_mouse_position;
        self.mouse_hit_test = saved_hit_test;

        // If the surface's hover state changed, force a redraw so hover styles update.
        if hit_test_changed || self.invalidator.is_dirty() {
            self.refresh();
        }
    }

    fn dispatch_key_event(&mut self, event: &dyn Any, cx: &mut App) {
        if self.invalidator.is_dirty() {
            self.draw(cx).clear();
        }

        let node_id = self.focus_node_id_in_rendered_frame(self.focus);
        let dispatch_path = self.rendered_frame.dispatch_tree.dispatch_path(node_id);

        let mut keystroke: Option<Keystroke> = None;

        if let Some(event) = event.downcast_ref::<ModifiersChangedEvent>() {
            if event.modifiers.number_of_modifiers() == 0
                && self.pending_modifier.modifiers.number_of_modifiers() == 1
                && !self.pending_modifier.saw_keystroke
            {
                let key = match self.pending_modifier.modifiers {
                    modifiers if modifiers.shift => Some("shift"),
                    modifiers if modifiers.control => Some("control"),
                    modifiers if modifiers.alt => Some("alt"),
                    modifiers if modifiers.platform => Some("platform"),
                    modifiers if modifiers.function => Some("function"),
                    _ => None,
                };
                if let Some(key) = key {
                    keystroke = Some(Keystroke {
                        key: key.to_string(),
                        key_char: None,
                        modifiers: Modifiers::default(),
                        native_key_code: None,
                    });
                }
            }

            if self.pending_modifier.modifiers.number_of_modifiers() == 0
                && event.modifiers.number_of_modifiers() == 1
            {
                self.pending_modifier.saw_keystroke = false
            }
            self.pending_modifier.modifiers = event.modifiers
        } else if let Some(key_down_event) = event.downcast_ref::<KeyDownEvent>() {
            self.pending_modifier.saw_keystroke = true;
            keystroke = Some(key_down_event.keystroke.clone());
        }

        let Some(keystroke) = keystroke else {
            self.finish_dispatch_key_event(event, dispatch_path, self.context_stack(), cx);
            return;
        };

        cx.propagate_event = true;
        self.dispatch_keystroke_interceptors(event, self.context_stack(), cx);
        if !cx.propagate_event {
            self.finish_dispatch_key_event(event, dispatch_path, self.context_stack(), cx);
            return;
        }

        let mut currently_pending = self.pending_input.take().unwrap_or_default();
        if currently_pending.focus.is_some() && currently_pending.focus != self.focus {
            currently_pending = PendingInput::default();
        }

        let match_result = self.rendered_frame.dispatch_tree.dispatch_key(
            currently_pending.keystrokes,
            keystroke,
            &dispatch_path,
        );

        if !match_result.to_replay.is_empty() {
            self.replay_pending_input(match_result.to_replay, cx);
            cx.propagate_event = true;
        }

        if !match_result.pending.is_empty() {
            currently_pending.timer.take();
            currently_pending.keystrokes = match_result.pending;
            currently_pending.focus = self.focus;

            let text_input_requires_timeout = event
                .downcast_ref::<KeyDownEvent>()
                .filter(|key_down| key_down.keystroke.key_char.is_some())
                .and_then(|_| self.platform_window.take_input_handler())
                .map_or(false, |mut input_handler| {
                    let accepts = input_handler.accepts_text_input(self, cx);
                    self.platform_window.set_input_handler(input_handler);
                    accepts
                });

            currently_pending.needs_timeout |=
                match_result.pending_has_binding || text_input_requires_timeout;

            if currently_pending.needs_timeout {
                currently_pending.timer = Some(self.spawn(cx, async move |cx| {
                    cx.background_executor.timer(Duration::from_secs(1)).await;
                    cx.update(move |window, cx| {
                        let Some(currently_pending) = window
                            .pending_input
                            .take()
                            .filter(|pending| pending.focus == window.focus)
                        else {
                            return;
                        };

                        let node_id = window.focus_node_id_in_rendered_frame(window.focus);
                        let dispatch_path =
                            window.rendered_frame.dispatch_tree.dispatch_path(node_id);

                        let to_replay = window
                            .rendered_frame
                            .dispatch_tree
                            .flush_dispatch(currently_pending.keystrokes, &dispatch_path);

                        window.pending_input_changed(cx);
                        window.replay_pending_input(to_replay, cx)
                    })
                    .log_err();
                }));
            } else {
                currently_pending.timer = None;
            }
            self.pending_input = Some(currently_pending);
            self.pending_input_changed(cx);
            cx.propagate_event = false;
            return;
        }

        let skip_bindings = event
            .downcast_ref::<KeyDownEvent>()
            .filter(|key_down_event| key_down_event.prefer_character_input)
            .map(|_| {
                self.platform_window
                    .take_input_handler()
                    .map_or(false, |mut input_handler| {
                        let accepts = input_handler.accepts_text_input(self, cx);
                        self.platform_window.set_input_handler(input_handler);
                        // If modifiers are not excessive (e.g. AltGr), and the input handler is accepting text input,
                        // we prefer the text input over bindings.
                        accepts
                    })
            })
            .unwrap_or(false);

        if !skip_bindings {
            for binding in match_result.bindings {
                self.dispatch_action_on_node(node_id, binding.action.as_ref(), cx);
                if !cx.propagate_event {
                    self.dispatch_keystroke_observers(
                        event,
                        Some(binding.action),
                        match_result.context_stack,
                        cx,
                    );
                    self.pending_input_changed(cx);
                    return;
                }
            }
        }

        self.finish_dispatch_key_event(event, dispatch_path, match_result.context_stack, cx);
        self.pending_input_changed(cx);
    }

    fn finish_dispatch_key_event(
        &mut self,
        event: &dyn Any,
        dispatch_path: SmallVec<[DispatchNodeId; 32]>,
        context_stack: Vec<KeyContext>,
        cx: &mut App,
    ) {
        self.dispatch_key_down_up_event(event, &dispatch_path, cx);
        if !cx.propagate_event {
            return;
        }

        self.dispatch_modifiers_changed_event(event, &dispatch_path, cx);
        if !cx.propagate_event {
            return;
        }

        self.dispatch_keystroke_observers(event, None, context_stack, cx);
    }

    pub(crate) fn pending_input_changed(&mut self, cx: &mut App) {
        self.pending_input_observers
            .clone()
            .retain(&(), |callback| callback(self, cx));
    }

    fn dispatch_key_down_up_event(
        &mut self,
        event: &dyn Any,
        dispatch_path: &SmallVec<[DispatchNodeId; 32]>,
        cx: &mut App,
    ) {
        // Capture phase
        for node_id in dispatch_path {
            let node = self.rendered_frame.dispatch_tree.node(*node_id);

            for key_listener in node.key_listeners.clone() {
                key_listener(event, DispatchPhase::Capture, self, cx);
                if !cx.propagate_event {
                    return;
                }
            }
        }

        // Bubble phase
        for node_id in dispatch_path.iter().rev() {
            // Handle low level key events
            let node = self.rendered_frame.dispatch_tree.node(*node_id);
            for key_listener in node.key_listeners.clone() {
                key_listener(event, DispatchPhase::Bubble, self, cx);
                if !cx.propagate_event {
                    return;
                }
            }
        }
    }

    fn dispatch_modifiers_changed_event(
        &mut self,
        event: &dyn Any,
        dispatch_path: &SmallVec<[DispatchNodeId; 32]>,
        cx: &mut App,
    ) {
        let Some(event) = event.downcast_ref::<ModifiersChangedEvent>() else {
            return;
        };
        for node_id in dispatch_path.iter().rev() {
            let node = self.rendered_frame.dispatch_tree.node(*node_id);
            for listener in node.modifiers_changed_listeners.clone() {
                listener(event, self, cx);
                if !cx.propagate_event {
                    return;
                }
            }
        }
    }

    /// Determine whether a potential multi-stroke key binding is in progress on this window.
    pub fn has_pending_keystrokes(&self) -> bool {
        self.pending_input.is_some()
    }

    pub(crate) fn clear_pending_keystrokes(&mut self) {
        self.pending_input.take();
    }

    /// Returns the currently pending input keystrokes that might result in a multi-stroke key binding.
    pub fn pending_input_keystrokes(&self) -> Option<&[Keystroke]> {
        self.pending_input
            .as_ref()
            .map(|pending_input| pending_input.keystrokes.as_slice())
    }

    fn replay_pending_input(&mut self, replays: SmallVec<[Replay; 1]>, cx: &mut App) {
        let node_id = self.focus_node_id_in_rendered_frame(self.focus);
        let dispatch_path = self.rendered_frame.dispatch_tree.dispatch_path(node_id);

        'replay: for replay in replays {
            let event = KeyDownEvent {
                keystroke: replay.keystroke.clone(),
                is_held: false,
                prefer_character_input: true,
            };

            cx.propagate_event = true;
            for binding in replay.bindings {
                self.dispatch_action_on_node(node_id, binding.action.as_ref(), cx);
                if !cx.propagate_event {
                    self.dispatch_keystroke_observers(
                        &event,
                        Some(binding.action),
                        Vec::default(),
                        cx,
                    );
                    continue 'replay;
                }
            }

            self.dispatch_key_down_up_event(&event, &dispatch_path, cx);
            if !cx.propagate_event {
                continue 'replay;
            }
            if let Some(input) = replay.keystroke.key_char.as_ref().cloned()
                && let Some(mut input_handler) = self.platform_window.take_input_handler()
            {
                input_handler.dispatch_input(&input, self, cx);
                self.platform_window.set_input_handler(input_handler)
            }
        }
    }

    fn focus_node_id_in_rendered_frame(&self, focus_id: Option<FocusId>) -> DispatchNodeId {
        focus_id
            .and_then(|focus_id| {
                self.rendered_frame
                    .dispatch_tree
                    .focusable_node_id(focus_id)
            })
            .unwrap_or_else(|| self.rendered_frame.dispatch_tree.root_node_id())
    }

    fn dispatch_action_on_node(
        &mut self,
        node_id: DispatchNodeId,
        action: &dyn Action,
        cx: &mut App,
    ) {
        let dispatch_path = self.rendered_frame.dispatch_tree.dispatch_path(node_id);

        // Capture phase for global actions.
        cx.propagate_event = true;
        if let Some(mut global_listeners) = cx
            .global_action_listeners
            .remove(&action.as_any().type_id())
        {
            for listener in &global_listeners {
                listener(action.as_any(), DispatchPhase::Capture, cx);
                if !cx.propagate_event {
                    break;
                }
            }

            global_listeners.extend(
                cx.global_action_listeners
                    .remove(&action.as_any().type_id())
                    .unwrap_or_default(),
            );

            cx.global_action_listeners
                .insert(action.as_any().type_id(), global_listeners);
        }

        if !cx.propagate_event {
            return;
        }

        // Capture phase for window actions.
        for node_id in &dispatch_path {
            let node = self.rendered_frame.dispatch_tree.node(*node_id);
            for DispatchActionListener {
                action_type,
                listener,
            } in node.action_listeners.clone()
            {
                let any_action = action.as_any();
                if action_type == any_action.type_id() {
                    listener(any_action, DispatchPhase::Capture, self, cx);

                    if !cx.propagate_event {
                        return;
                    }
                }
            }
        }

        // Bubble phase for window actions.
        for node_id in dispatch_path.iter().rev() {
            let node = self.rendered_frame.dispatch_tree.node(*node_id);
            for DispatchActionListener {
                action_type,
                listener,
            } in node.action_listeners.clone()
            {
                let any_action = action.as_any();
                if action_type == any_action.type_id() {
                    cx.propagate_event = false; // Actions stop propagation by default during the bubble phase
                    listener(any_action, DispatchPhase::Bubble, self, cx);

                    if !cx.propagate_event {
                        return;
                    }
                }
            }
        }

        // Bubble phase for global actions.
        if let Some(mut global_listeners) = cx
            .global_action_listeners
            .remove(&action.as_any().type_id())
        {
            for listener in global_listeners.iter().rev() {
                cx.propagate_event = false; // Actions stop propagation by default during the bubble phase

                listener(action.as_any(), DispatchPhase::Bubble, cx);
                if !cx.propagate_event {
                    break;
                }
            }

            global_listeners.extend(
                cx.global_action_listeners
                    .remove(&action.as_any().type_id())
                    .unwrap_or_default(),
            );

            cx.global_action_listeners
                .insert(action.as_any().type_id(), global_listeners);
        }
    }

    /// Register the given handler to be invoked whenever the global of the given type
    /// is updated.
    pub fn observe_global<G: Global>(
        &mut self,
        cx: &mut App,
        f: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Subscription {
        let window_handle = self.handle;
        let (subscription, activate) = cx.global_observers.insert(
            TypeId::of::<G>(),
            Box::new(move |cx| {
                window_handle
                    .update(cx, |_, window, cx| f(window, cx))
                    .is_ok()
            }),
        );
        cx.defer(move |_| activate());
        subscription
    }

    /// Focus the current window and bring it to the foreground at the platform level.
    pub fn activate_window(&self) {
        self.platform_window.activate();
    }

    /// Minimize the current window at the platform level.
    pub fn minimize_window(&self) {
        self.platform_window.minimize();
    }

    /// Toggle full screen status on the current window at the platform level.
    pub fn toggle_fullscreen(&self) {
        self.platform_window.toggle_fullscreen();
    }

    /// Updates the IME panel position suggestions for languages like japanese, chinese.
    pub fn invalidate_character_coordinates(&self) {
        self.on_next_frame(|window, cx| {
            if let Some(mut input_handler) = window.platform_window.take_input_handler() {
                if let Some(bounds) = input_handler.selected_bounds(window, cx) {
                    window.platform_window.update_ime_position(bounds);
                }
                window.platform_window.set_input_handler(input_handler);
            }
        });
    }

    /// Present a platform dialog.
    /// The provided message will be presented, along with buttons for each answer.
    /// When a button is clicked, the returned Receiver will receive the index of the clicked button.
    pub fn prompt<T>(
        &mut self,
        level: PromptLevel,
        message: &str,
        detail: Option<&str>,
        answers: &[T],
        cx: &mut App,
    ) -> oneshot::Receiver<usize>
    where
        T: Clone + Into<PromptButton>,
    {
        let prompt_builder = cx.prompt_builder.take();
        let Some(prompt_builder) = prompt_builder else {
            unreachable!("Re-entrant window prompting is not supported by GPUI");
        };

        let answers = answers
            .iter()
            .map(|answer| answer.clone().into())
            .collect::<Vec<_>>();

        let receiver = match &prompt_builder {
            PromptBuilder::Default => self
                .platform_window
                .prompt(level, message, detail, &answers)
                .unwrap_or_else(|| {
                    self.build_custom_prompt(&prompt_builder, level, message, detail, &answers, cx)
                }),
            PromptBuilder::Custom(_) => {
                self.build_custom_prompt(&prompt_builder, level, message, detail, &answers, cx)
            }
        };

        cx.prompt_builder = Some(prompt_builder);

        receiver
    }

    fn build_custom_prompt(
        &mut self,
        prompt_builder: &PromptBuilder,
        level: PromptLevel,
        message: &str,
        detail: Option<&str>,
        answers: &[PromptButton],
        cx: &mut App,
    ) -> oneshot::Receiver<usize> {
        let (sender, receiver) = oneshot::channel();
        let handle = PromptHandle::new(sender);
        let handle = (prompt_builder)(level, message, detail, answers, handle, self, cx);
        self.prompt = Some(handle);
        receiver
    }

    /// Returns the current context stack.
    pub fn context_stack(&self) -> Vec<KeyContext> {
        let node_id = self.focus_node_id_in_rendered_frame(self.focus);
        let dispatch_tree = &self.rendered_frame.dispatch_tree;
        dispatch_tree
            .dispatch_path(node_id)
            .iter()
            .filter_map(move |&node_id| dispatch_tree.node(node_id).context.clone())
            .collect()
    }

    /// Returns all available actions for the focused element.
    pub fn available_actions(&self, cx: &App) -> Vec<Box<dyn Action>> {
        let node_id = self.focus_node_id_in_rendered_frame(self.focus);
        let mut actions = self.rendered_frame.dispatch_tree.available_actions(node_id);
        for action_type in cx.global_action_listeners.keys() {
            if let Err(ix) = actions.binary_search_by_key(action_type, |a| a.as_any().type_id()) {
                let action = cx.actions.build_action_type(action_type).ok();
                if let Some(action) = action {
                    actions.insert(ix, action);
                }
            }
        }
        actions
    }

    /// Returns key bindings that invoke an action on the currently focused element. Bindings are
    /// returned in the order they were added. For display, the last binding should take precedence.
    pub fn bindings_for_action(&self, action: &dyn Action) -> Vec<KeyBinding> {
        self.rendered_frame
            .dispatch_tree
            .bindings_for_action(action, &self.rendered_frame.dispatch_tree.context_stack)
    }

    /// Returns the highest precedence key binding that invokes an action on the currently focused
    /// element. This is more efficient than getting the last result of `bindings_for_action`.
    pub fn highest_precedence_binding_for_action(&self, action: &dyn Action) -> Option<KeyBinding> {
        self.rendered_frame
            .dispatch_tree
            .highest_precedence_binding_for_action(
                action,
                &self.rendered_frame.dispatch_tree.context_stack,
            )
    }

    /// Returns the key bindings for an action in a context.
    pub fn bindings_for_action_in_context(
        &self,
        action: &dyn Action,
        context: KeyContext,
    ) -> Vec<KeyBinding> {
        let dispatch_tree = &self.rendered_frame.dispatch_tree;
        dispatch_tree.bindings_for_action(action, &[context])
    }

    /// Returns the highest precedence key binding for an action in a context. This is more
    /// efficient than getting the last result of `bindings_for_action_in_context`.
    pub fn highest_precedence_binding_for_action_in_context(
        &self,
        action: &dyn Action,
        context: KeyContext,
    ) -> Option<KeyBinding> {
        let dispatch_tree = &self.rendered_frame.dispatch_tree;
        dispatch_tree.highest_precedence_binding_for_action(action, &[context])
    }

    /// Returns any bindings that would invoke an action on the given focus handle if it were
    /// focused. Bindings are returned in the order they were added. For display, the last binding
    /// should take precedence.
    pub fn bindings_for_action_in(
        &self,
        action: &dyn Action,
        focus_handle: &FocusHandle,
    ) -> Vec<KeyBinding> {
        let dispatch_tree = &self.rendered_frame.dispatch_tree;
        let Some(context_stack) = self.context_stack_for_focus_handle(focus_handle) else {
            return vec![];
        };
        dispatch_tree.bindings_for_action(action, &context_stack)
    }

    /// Returns the highest precedence key binding that would invoke an action on the given focus
    /// handle if it were focused. This is more efficient than getting the last result of
    /// `bindings_for_action_in`.
    pub fn highest_precedence_binding_for_action_in(
        &self,
        action: &dyn Action,
        focus_handle: &FocusHandle,
    ) -> Option<KeyBinding> {
        let dispatch_tree = &self.rendered_frame.dispatch_tree;
        let context_stack = self.context_stack_for_focus_handle(focus_handle)?;
        dispatch_tree.highest_precedence_binding_for_action(action, &context_stack)
    }

    /// Find the bindings that can follow the current input sequence for the current context stack.
    pub fn possible_bindings_for_input(&self, input: &[Keystroke]) -> Vec<KeyBinding> {
        self.rendered_frame
            .dispatch_tree
            .possible_next_bindings_for_input(input, &self.context_stack())
    }

    fn context_stack_for_focus_handle(
        &self,
        focus_handle: &FocusHandle,
    ) -> Option<Vec<KeyContext>> {
        let dispatch_tree = &self.rendered_frame.dispatch_tree;
        let node_id = dispatch_tree.focusable_node_id(focus_handle.id)?;
        let context_stack: Vec<_> = dispatch_tree
            .dispatch_path(node_id)
            .into_iter()
            .filter_map(|node_id| dispatch_tree.node(node_id).context.clone())
            .collect();
        Some(context_stack)
    }

    /// Returns a generic event listener that invokes the given listener with the view and context associated with the given view handle.
    pub fn listener_for<T: 'static, E>(
        &self,
        view: &Entity<T>,
        f: impl Fn(&mut T, &E, &mut Window, &mut Context<T>) + 'static,
    ) -> impl Fn(&E, &mut Window, &mut App) + 'static {
        let view = view.downgrade();
        move |e: &E, window: &mut Window, cx: &mut App| {
            view.update(cx, |view, cx| f(view, e, window, cx)).ok();
        }
    }

    /// Returns a generic handler that invokes the given handler with the view and context associated with the given view handle.
    pub fn handler_for<E: 'static, Callback: Fn(&mut E, &mut Window, &mut Context<E>) + 'static>(
        &self,
        entity: &Entity<E>,
        f: Callback,
    ) -> impl Fn(&mut Window, &mut App) + 'static {
        let entity = entity.downgrade();
        move |window: &mut Window, cx: &mut App| {
            entity.update(cx, |entity, cx| f(entity, window, cx)).ok();
        }
    }

    /// Register a callback that can interrupt the closing of the current window based the returned boolean.
    /// If the callback returns false, the window won't be closed.
    pub fn on_window_should_close(
        &self,
        cx: &App,
        f: impl Fn(&mut Window, &mut App) -> bool + 'static,
    ) {
        let mut cx = self.to_async(cx);
        self.platform_window.on_should_close(Box::new(move || {
            cx.update(|window, cx| f(window, cx)).unwrap_or(true)
        }))
    }

    /// Register an action listener on this node for the next frame. The type of action
    /// is determined by the first parameter of the given listener. When the next frame is rendered
    /// the listener will be cleared.
    ///
    /// This is a fairly low-level method, so prefer using action handlers on elements unless you have
    /// a specific need to register a listener yourself.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn on_action(
        &mut self,
        action_type: TypeId,
        listener: impl Fn(&dyn Any, DispatchPhase, &mut Window, &mut App) + 'static,
    ) {
        self.invalidator.debug_assert_paint();

        self.next_frame
            .dispatch_tree
            .on_action(action_type, Rc::new(listener));
    }

    /// Register a capturing action listener on this node for the next frame if the condition is true.
    /// The type of action is determined by the first parameter of the given listener. When the next
    /// frame is rendered the listener will be cleared.
    ///
    /// This is a fairly low-level method, so prefer using action handlers on elements unless you have
    /// a specific need to register a listener yourself.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn on_action_when(
        &mut self,
        condition: bool,
        action_type: TypeId,
        listener: impl Fn(&dyn Any, DispatchPhase, &mut Window, &mut App) + 'static,
    ) {
        self.invalidator.debug_assert_paint();

        if condition {
            self.next_frame
                .dispatch_tree
                .on_action(action_type, Rc::new(listener));
        }
    }

    /// Read information about the GPU backing this window.
    /// Currently returns None on Mac and Windows.
    pub fn gpu_specs(&self) -> Option<GpuSpecs> {
        self.platform_window.gpu_specs()
    }

    /// Perform titlebar double-click action.
    /// This is macOS specific.
    pub fn titlebar_double_click(&self) {
        self.platform_window.titlebar_double_click();
    }

    /// Gets the window's title at the platform level.
    /// This is macOS specific.
    pub fn window_title(&self) -> String {
        self.platform_window.get_title()
    }

    /// Returns a list of all tabbed windows and their titles.
    /// This is macOS specific.
    pub fn tabbed_windows(&self) -> Option<Vec<SystemWindowTab>> {
        self.platform_window.tabbed_windows()
    }

    /// Returns the tab bar visibility.
    /// This is macOS specific.
    pub fn tab_bar_visible(&self) -> bool {
        self.platform_window.tab_bar_visible()
    }

    /// Merges all open windows into a single tabbed window.
    /// This is macOS specific.
    pub fn merge_all_windows(&self) {
        self.platform_window.merge_all_windows()
    }

    /// Moves the tab to a new containing window.
    /// This is macOS specific.
    pub fn move_tab_to_new_window(&self) {
        self.platform_window.move_tab_to_new_window()
    }

    /// Shows or hides the window tab overview.
    /// This is macOS specific.
    pub fn toggle_window_tab_overview(&self) {
        self.platform_window.toggle_window_tab_overview()
    }

    /// Sets the tabbing identifier for the window.
    /// This is macOS specific.
    pub fn set_tabbing_identifier(&self, tabbing_identifier: Option<String>) {
        self.platform_window
            .set_tabbing_identifier(tabbing_identifier)
    }

    /// Toggles the inspector mode on this window.
    #[cfg(any(feature = "inspector", debug_assertions))]
    pub fn toggle_inspector(&mut self, cx: &mut App) {
        self.inspector = match self.inspector {
            None => Some(cx.new(|_| Inspector::new())),
            Some(_) => None,
        };
        self.refresh();
    }

    /// Returns true if the window is in inspector mode.
    pub fn is_inspector_picking(&self, _cx: &App) -> bool {
        #[cfg(any(feature = "inspector", debug_assertions))]
        {
            if let Some(inspector) = &self.inspector {
                return inspector.read(_cx).is_picking();
            }
        }
        false
    }

    /// Executes the provided function with mutable access to an inspector state.
    #[cfg(any(feature = "inspector", debug_assertions))]
    pub fn with_inspector_state<T: 'static, R>(
        &mut self,
        _inspector_id: Option<&crate::InspectorElementId>,
        cx: &mut App,
        f: impl FnOnce(&mut Option<T>, &mut Self) -> R,
    ) -> R {
        if let Some(inspector_id) = _inspector_id
            && let Some(inspector) = &self.inspector
        {
            let inspector = inspector.clone();
            let active_element_id = inspector.read(cx).active_element_id();
            if Some(inspector_id) == active_element_id {
                return inspector.update(cx, |inspector, _cx| {
                    inspector.with_active_element_state(self, f)
                });
            }
        }
        f(&mut None, self)
    }

    #[cfg(any(feature = "inspector", debug_assertions))]
    pub(crate) fn build_inspector_element_id(
        &mut self,
        path: crate::InspectorElementPath,
    ) -> crate::InspectorElementId {
        self.invalidator.debug_assert_paint_or_prepaint();
        let path = Rc::new(path);
        let next_instance_id = self
            .next_frame
            .next_inspector_instance_ids
            .entry(path.clone())
            .or_insert(0);
        let instance_id = *next_instance_id;
        *next_instance_id += 1;
        crate::InspectorElementId { path, instance_id }
    }

    #[cfg(any(feature = "inspector", debug_assertions))]
    fn prepaint_inspector(&mut self, inspector_width: Pixels, cx: &mut App) -> Option<AnyElement> {
        if let Some(inspector) = self.inspector.take() {
            let mut inspector_element = AnyView::from(inspector.clone()).into_any_element();
            inspector_element.prepaint_as_root(
                point(self.viewport_size.width - inspector_width, px(0.0)),
                size(inspector_width, self.viewport_size.height).into(),
                self,
                cx,
            );
            self.inspector = Some(inspector);
            Some(inspector_element)
        } else {
            None
        }
    }

    #[cfg(any(feature = "inspector", debug_assertions))]
    fn paint_inspector(&mut self, mut inspector_element: Option<AnyElement>, cx: &mut App) {
        if let Some(mut inspector_element) = inspector_element {
            inspector_element.paint(self, cx);
        };
    }

    /// Registers a hitbox that can be used for inspector picking mode, allowing users to select and
    /// inspect UI elements by clicking on them.
    #[cfg(any(feature = "inspector", debug_assertions))]
    pub fn insert_inspector_hitbox(
        &mut self,
        hitbox_id: HitboxId,
        inspector_id: Option<&crate::InspectorElementId>,
        cx: &App,
    ) {
        self.invalidator.debug_assert_paint_or_prepaint();
        if !self.is_inspector_picking(cx) {
            return;
        }
        if let Some(inspector_id) = inspector_id {
            self.next_frame
                .inspector_hitboxes
                .insert(hitbox_id, inspector_id.clone());
        }
    }

    #[cfg(any(feature = "inspector", debug_assertions))]
    fn paint_inspector_hitbox(&mut self, cx: &App) {
        if let Some(inspector) = self.inspector.as_ref() {
            let inspector = inspector.read(cx);
            if let Some((hitbox_id, _)) = self.hovered_inspector_hitbox(inspector, &self.next_frame)
                && let Some(hitbox) = self
                    .next_frame
                    .hitboxes
                    .iter()
                    .find(|hitbox| hitbox.id == hitbox_id)
            {
                self.paint_quad(crate::fill(hitbox.bounds, crate::rgba(0x61afef4d)));
            }
        }
    }

    #[cfg(any(feature = "inspector", debug_assertions))]
    fn handle_inspector_mouse_event(&mut self, event: &dyn Any, cx: &mut App) {
        let Some(inspector) = self.inspector.clone() else {
            return;
        };
        if event.downcast_ref::<MouseMoveEvent>().is_some() {
            inspector.update(cx, |inspector, _cx| {
                if let Some((_, inspector_id)) =
                    self.hovered_inspector_hitbox(inspector, &self.rendered_frame)
                {
                    inspector.hover(inspector_id, self);
                }
            });
        } else if event.downcast_ref::<crate::MouseDownEvent>().is_some() {
            inspector.update(cx, |inspector, _cx| {
                if let Some((_, inspector_id)) =
                    self.hovered_inspector_hitbox(inspector, &self.rendered_frame)
                {
                    inspector.select(inspector_id, self);
                }
            });
        } else if let Some(event) = event.downcast_ref::<crate::ScrollWheelEvent>() {
            // This should be kept in sync with SCROLL_LINES in x11 platform.
            const SCROLL_LINES: f32 = 3.0;
            const SCROLL_PIXELS_PER_LAYER: f32 = 36.0;
            let delta_y = event
                .delta
                .pixel_delta(px(SCROLL_PIXELS_PER_LAYER / SCROLL_LINES))
                .y;
            if let Some(inspector) = self.inspector.clone() {
                inspector.update(cx, |inspector, _cx| {
                    if let Some(depth) = inspector.pick_depth.as_mut() {
                        *depth += f32::from(delta_y) / SCROLL_PIXELS_PER_LAYER;
                        let max_depth = self.mouse_hit_test.ids.len() as f32 - 0.5;
                        if *depth < 0.0 {
                            *depth = 0.0;
                        } else if *depth > max_depth {
                            *depth = max_depth;
                        }
                        if let Some((_, inspector_id)) =
                            self.hovered_inspector_hitbox(inspector, &self.rendered_frame)
                        {
                            inspector.set_active_element_id(inspector_id, self);
                        }
                    }
                });
            }
        }
    }

    #[cfg(any(feature = "inspector", debug_assertions))]
    fn hovered_inspector_hitbox(
        &self,
        inspector: &Inspector,
        frame: &Frame,
    ) -> Option<(HitboxId, crate::InspectorElementId)> {
        if let Some(pick_depth) = inspector.pick_depth {
            let depth = (pick_depth as i64).try_into().unwrap_or(0);
            let max_skipped = self.mouse_hit_test.ids.len().saturating_sub(1);
            let skip_count = (depth as usize).min(max_skipped);
            for hitbox_id in self.mouse_hit_test.ids.iter().skip(skip_count) {
                if let Some(inspector_id) = frame.inspector_hitboxes.get(hitbox_id) {
                    return Some((*hitbox_id, inspector_id.clone()));
                }
            }
        }
        None
    }

    /// For testing: set the current modifier keys state.
    /// This does not generate any events.
    #[cfg(any(test, feature = "test-support"))]
    pub fn set_modifiers(&mut self, modifiers: Modifiers) {
        self.modifiers = modifiers;
    }

    /// For testing: simulate a mouse move event to the given position.
    /// This dispatches the event through the normal event handling path,
    /// which will trigger hover states and tooltips.
    #[cfg(any(test, feature = "test-support"))]
    pub fn simulate_mouse_move(&mut self, position: Point<Pixels>, cx: &mut App) {
        let event = PlatformInput::MouseMove(MouseMoveEvent {
            position,
            modifiers: self.modifiers,
            pressed_button: None,
        });
        let _ = self.dispatch_event(event, cx);
    }
}

// #[derive(Clone, Copy, Eq, PartialEq, Hash)]
slotmap::new_key_type! {
    /// A unique identifier for a window.
    pub struct WindowId;
}

impl WindowId {
    /// Converts this window ID to a `u64`.
    pub fn as_u64(&self) -> u64 {
        self.0.as_ffi()
    }
}

impl From<u64> for WindowId {
    fn from(value: u64) -> Self {
        WindowId(slotmap::KeyData::from_ffi(value))
    }
}

/// A handle to a window with a specific root view type.
/// Note that this does not keep the window alive on its own.
#[derive(Deref, DerefMut)]
pub struct WindowHandle<V> {
    #[deref]
    #[deref_mut]
    pub(crate) any_handle: AnyWindowHandle,
    state_type: PhantomData<fn(V) -> V>,
}

impl<V> Debug for WindowHandle<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowHandle")
            .field("any_handle", &self.any_handle.id.as_u64())
            .finish()
    }
}

impl<V: 'static + Render> WindowHandle<V> {
    /// Creates a new handle from a window ID.
    /// This does not check if the root type of the window is `V`.
    pub fn new(id: WindowId) -> Self {
        WindowHandle {
            any_handle: AnyWindowHandle {
                id,
                state_type: TypeId::of::<V>(),
            },
            state_type: PhantomData,
        }
    }

    /// Get the root view out of this window.
    ///
    /// This will fail if the window is closed or if the root view's type does not match `V`.
    #[cfg(any(test, feature = "test-support"))]
    pub fn root<C>(&self, cx: &mut C) -> Result<Entity<V>>
    where
        C: AppContext,
    {
        cx.update_window(self.any_handle, |root_view, _, _| {
            root_view
                .downcast::<V>()
                .map_err(|_| anyhow!("the type of the window's root view has changed"))
        })?
    }

    /// Updates the root view of this window.
    ///
    /// This will fail if the window has been closed or if the root view's type does not match
    pub fn update<C, R>(
        &self,
        cx: &mut C,
        update: impl FnOnce(&mut V, &mut Window, &mut Context<V>) -> R,
    ) -> Result<R>
    where
        C: AppContext,
    {
        cx.update_window(self.any_handle, |root_view, window, cx| {
            let view = root_view
                .downcast::<V>()
                .map_err(|_| anyhow!("the type of the window's root view has changed"))?;

            Ok(view.update(cx, |view, cx| update(view, window, cx)))
        })?
    }

    /// Read the root view out of this window.
    ///
    /// This will fail if the window is closed or if the root view's type does not match `V`.
    pub fn read<'a>(&self, cx: &'a App) -> Result<&'a V> {
        let x = cx
            .windows
            .get(self.id)
            .and_then(|window| {
                window
                    .as_deref()
                    .and_then(|window| window.root.clone())
                    .map(|root_view| root_view.downcast::<V>())
            })
            .context("window not found")?
            .map_err(|_| anyhow!("the type of the window's root view has changed"))?;

        Ok(x.read(cx))
    }

    /// Read the root view out of this window, with a callback
    ///
    /// This will fail if the window is closed or if the root view's type does not match `V`.
    pub fn read_with<C, R>(&self, cx: &C, read_with: impl FnOnce(&V, &App) -> R) -> Result<R>
    where
        C: AppContext,
    {
        cx.read_window(self, |root_view, cx| read_with(root_view.read(cx), cx))
    }

    /// Read the root view pointer off of this window.
    ///
    /// This will fail if the window is closed or if the root view's type does not match `V`.
    pub fn entity<C>(&self, cx: &C) -> Result<Entity<V>>
    where
        C: AppContext,
    {
        cx.read_window(self, |root_view, _cx| root_view)
    }

    /// Check if this window is 'active'.
    ///
    /// Will return `None` if the window is closed or currently
    /// borrowed.
    pub fn is_active(&self, cx: &mut App) -> Option<bool> {
        cx.update_window(self.any_handle, |_, window, _| window.is_window_active())
            .ok()
    }
}

impl<V> Copy for WindowHandle<V> {}

impl<V> Clone for WindowHandle<V> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<V> PartialEq for WindowHandle<V> {
    fn eq(&self, other: &Self) -> bool {
        self.any_handle == other.any_handle
    }
}

impl<V> Eq for WindowHandle<V> {}

impl<V> Hash for WindowHandle<V> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.any_handle.hash(state);
    }
}

impl<V: 'static> From<WindowHandle<V>> for AnyWindowHandle {
    fn from(val: WindowHandle<V>) -> Self {
        val.any_handle
    }
}

/// A handle to a window with any root view type, which can be downcast to a window with a specific root view type.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct AnyWindowHandle {
    pub(crate) id: WindowId,
    state_type: TypeId,
}

impl AnyWindowHandle {
    /// Get the ID of this window.
    pub fn window_id(&self) -> WindowId {
        self.id
    }

    /// Attempt to convert this handle to a window handle with a specific root view type.
    /// If the types do not match, this will return `None`.
    pub fn downcast<T: 'static>(&self) -> Option<WindowHandle<T>> {
        if TypeId::of::<T>() == self.state_type {
            Some(WindowHandle {
                any_handle: *self,
                state_type: PhantomData,
            })
        } else {
            None
        }
    }

    /// Updates the state of the root view of this window.
    ///
    /// This will fail if the window has been closed.
    pub fn update<C, R>(
        self,
        cx: &mut C,
        update: impl FnOnce(AnyView, &mut Window, &mut App) -> R,
    ) -> Result<R>
    where
        C: AppContext,
    {
        cx.update_window(self, update)
    }

    /// Read the state of the root view of this window.
    ///
    /// This will fail if the window has been closed.
    pub fn read<T, C, R>(self, cx: &C, read: impl FnOnce(Entity<T>, &App) -> R) -> Result<R>
    where
        C: AppContext,
        T: 'static,
    {
        let view = self
            .downcast::<T>()
            .context("the type of the window's root view has changed")?;

        cx.read_window(&view, read)
    }
}

impl HasWindowHandle for Window {
    fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, HandleError> {
        self.platform_window.window_handle()
    }
}

impl HasDisplayHandle for Window {
    fn display_handle(
        &self,
    ) -> std::result::Result<raw_window_handle::DisplayHandle<'_>, HandleError> {
        self.platform_window.display_handle()
    }
}

/// An identifier for an [`Element`].
///
/// Can be constructed with a string, a number, or both, as well
/// as other internal representations.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum ElementId {
    /// The ID of a View element
    View(EntityId),
    /// An integer ID.
    Integer(u64),
    /// A string based ID.
    Name(SharedString),
    /// A UUID.
    Uuid(Uuid),
    /// An ID that's equated with a focus handle.
    FocusHandle(FocusId),
    /// A combination of a name and an integer.
    NamedInteger(SharedString, u64),
    /// A path.
    Path(Arc<std::path::Path>),
    /// A code location.
    CodeLocation(core::panic::Location<'static>),
    /// A labeled child of an element.
    NamedChild(Arc<ElementId>, SharedString),
}

impl ElementId {
    /// Constructs an `ElementId::NamedInteger` from a name and `usize`.
    pub fn named_usize(name: impl Into<SharedString>, integer: usize) -> ElementId {
        Self::NamedInteger(name.into(), integer as u64)
    }
}

impl Display for ElementId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ElementId::View(entity_id) => write!(f, "view-{}", entity_id)?,
            ElementId::Integer(ix) => write!(f, "{}", ix)?,
            ElementId::Name(name) => write!(f, "{}", name)?,
            ElementId::FocusHandle(_) => write!(f, "FocusHandle")?,
            ElementId::NamedInteger(s, i) => write!(f, "{}-{}", s, i)?,
            ElementId::Uuid(uuid) => write!(f, "{}", uuid)?,
            ElementId::Path(path) => write!(f, "{}", path.display())?,
            ElementId::CodeLocation(location) => write!(f, "{}", location)?,
            ElementId::NamedChild(id, name) => write!(f, "{}-{}", id, name)?,
        }

        Ok(())
    }
}

impl TryInto<SharedString> for ElementId {
    type Error = anyhow::Error;

    fn try_into(self) -> anyhow::Result<SharedString> {
        if let ElementId::Name(name) = self {
            Ok(name)
        } else {
            anyhow::bail!("element id is not string")
        }
    }
}

impl From<usize> for ElementId {
    fn from(id: usize) -> Self {
        ElementId::Integer(id as u64)
    }
}

impl From<i32> for ElementId {
    fn from(id: i32) -> Self {
        Self::Integer(id as u64)
    }
}

impl From<SharedString> for ElementId {
    fn from(name: SharedString) -> Self {
        ElementId::Name(name)
    }
}

impl From<String> for ElementId {
    fn from(name: String) -> Self {
        ElementId::Name(name.into())
    }
}

impl From<Arc<str>> for ElementId {
    fn from(name: Arc<str>) -> Self {
        ElementId::Name(name.into())
    }
}

impl From<Arc<std::path::Path>> for ElementId {
    fn from(path: Arc<std::path::Path>) -> Self {
        ElementId::Path(path)
    }
}

impl From<&'static str> for ElementId {
    fn from(name: &'static str) -> Self {
        ElementId::Name(name.into())
    }
}

impl<'a> From<&'a FocusHandle> for ElementId {
    fn from(handle: &'a FocusHandle) -> Self {
        ElementId::FocusHandle(handle.id)
    }
}

impl From<(&'static str, EntityId)> for ElementId {
    fn from((name, id): (&'static str, EntityId)) -> Self {
        ElementId::NamedInteger(name.into(), id.as_u64())
    }
}

impl From<(&'static str, usize)> for ElementId {
    fn from((name, id): (&'static str, usize)) -> Self {
        ElementId::NamedInteger(name.into(), id as u64)
    }
}

impl From<(SharedString, usize)> for ElementId {
    fn from((name, id): (SharedString, usize)) -> Self {
        ElementId::NamedInteger(name, id as u64)
    }
}

impl From<(&'static str, u64)> for ElementId {
    fn from((name, id): (&'static str, u64)) -> Self {
        ElementId::NamedInteger(name.into(), id)
    }
}

impl From<Uuid> for ElementId {
    fn from(value: Uuid) -> Self {
        Self::Uuid(value)
    }
}

impl From<(&'static str, u32)> for ElementId {
    fn from((name, id): (&'static str, u32)) -> Self {
        ElementId::NamedInteger(name.into(), id.into())
    }
}

impl<T: Into<SharedString>> From<(ElementId, T)> for ElementId {
    fn from((id, name): (ElementId, T)) -> Self {
        ElementId::NamedChild(Arc::new(id), name.into())
    }
}

impl From<&'static core::panic::Location<'static>> for ElementId {
    fn from(location: &'static core::panic::Location<'static>) -> Self {
        ElementId::CodeLocation(*location)
    }
}

/// A rectangle to be rendered in the window at the given position and size.
/// Passed as an argument [`Window::paint_quad`].
#[derive(Clone)]
pub struct PaintQuad {
    /// The bounds of the quad within the window.
    pub bounds: Bounds<Pixels>,
    /// The radii of the quad's corners.
    pub corner_radii: Corners<Pixels>,
    /// The background color of the quad.
    pub background: Background,
    /// The widths of the quad's borders.
    pub border_widths: Edges<Pixels>,
    /// The color of the quad's borders.
    pub border_color: Hsla,
    /// The style of the quad's borders.
    pub border_style: BorderStyle,
}

impl PaintQuad {
    /// Sets the corner radii of the quad.
    pub fn corner_radii(self, corner_radii: impl Into<Corners<Pixels>>) -> Self {
        PaintQuad {
            corner_radii: corner_radii.into(),
            ..self
        }
    }

    /// Sets the border widths of the quad.
    pub fn border_widths(self, border_widths: impl Into<Edges<Pixels>>) -> Self {
        PaintQuad {
            border_widths: border_widths.into(),
            ..self
        }
    }

    /// Sets the border color of the quad.
    pub fn border_color(self, border_color: impl Into<Hsla>) -> Self {
        PaintQuad {
            border_color: border_color.into(),
            ..self
        }
    }

    /// Sets the background color of the quad.
    pub fn background(self, background: impl Into<Background>) -> Self {
        PaintQuad {
            background: background.into(),
            ..self
        }
    }
}

/// Creates a quad with the given parameters.
pub fn quad(
    bounds: Bounds<Pixels>,
    corner_radii: impl Into<Corners<Pixels>>,
    background: impl Into<Background>,
    border_widths: impl Into<Edges<Pixels>>,
    border_color: impl Into<Hsla>,
    border_style: BorderStyle,
) -> PaintQuad {
    PaintQuad {
        bounds,
        corner_radii: corner_radii.into(),
        background: background.into(),
        border_widths: border_widths.into(),
        border_color: border_color.into(),
        border_style,
    }
}

/// Creates a filled quad with the given bounds and background color.
pub fn fill(bounds: impl Into<Bounds<Pixels>>, background: impl Into<Background>) -> PaintQuad {
    PaintQuad {
        bounds: bounds.into(),
        corner_radii: (0.).into(),
        background: background.into(),
        border_widths: (0.).into(),
        border_color: transparent_black(),
        border_style: BorderStyle::default(),
    }
}

/// Creates a rectangle outline with the given bounds, border color, and a 1px border width
pub fn outline(
    bounds: impl Into<Bounds<Pixels>>,
    border_color: impl Into<Hsla>,
    border_style: BorderStyle,
) -> PaintQuad {
    PaintQuad {
        bounds: bounds.into(),
        corner_radii: (0.).into(),
        background: transparent_black().into(),
        border_widths: (1.).into(),
        border_color: border_color.into(),
        border_style,
    }
}
