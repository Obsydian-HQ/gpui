use crate::{Bounds, Pixels};
use std::ffi::c_void;

// =============================================================================
// NativeControlState — opaque handle stored by elements, cleaned up on Drop
// =============================================================================

/// Opaque state for a platform-native control. Elements store this in their
/// element state. The platform implementation populates it during `update_*`
/// calls. Cleanup happens automatically on Drop via the stored cleanup function.
pub struct NativeControlState {
    pub(crate) view: *mut c_void,
    pub(crate) target: *mut c_void,
    cleanup: unsafe fn(*mut c_void, *mut c_void),
}

impl Default for NativeControlState {
    fn default() -> Self {
        Self {
            view: std::ptr::null_mut(),
            target: std::ptr::null_mut(),
            cleanup: noop_cleanup,
        }
    }
}

impl Drop for NativeControlState {
    fn drop(&mut self) {
        unsafe { (self.cleanup)(self.view, self.target) }
    }
}

unsafe impl Send for NativeControlState {}

unsafe fn noop_cleanup(_view: *mut c_void, _target: *mut c_void) {}

impl NativeControlState {
    pub fn new(
        view: *mut c_void,
        target: *mut c_void,
        cleanup: unsafe fn(*mut c_void, *mut c_void),
    ) -> Self {
        Self {
            view,
            target,
            cleanup,
        }
    }

    pub fn view(&self) -> *mut c_void {
        self.view
    }

    pub fn is_initialized(&self) -> bool {
        !self.view.is_null()
    }

    pub fn set_target(&mut self, target: *mut c_void) {
        self.target = target;
    }

    pub fn target(&self) -> *mut c_void {
        self.target
    }
}

// =============================================================================
// Per-control config structs — pure Rust, no platform dependencies
// =============================================================================

pub struct ButtonConfig<'a> {
    pub title: &'a str,
    pub sf_symbol: Option<&'a str>,
    pub tooltip: Option<&'a str>,
    pub style: ButtonStyle,
    pub tint: Option<(f64, f64, f64, f64)>,
    pub enabled: bool,
    pub on_click: Option<Box<dyn Fn()>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle {
    Rounded,
    Filled,
    Inline,
    Borderless,
}

pub struct CheckboxConfig<'a> {
    pub title: &'a str,
    pub checked: bool,
    pub enabled: bool,
    pub on_change: Option<Box<dyn Fn(bool)>>,
}

pub struct SwitchConfig {
    pub checked: bool,
    pub enabled: bool,
    pub on_change: Option<Box<dyn Fn(bool)>>,
}

pub struct SliderConfig {
    pub min: f64,
    pub max: f64,
    pub value: f64,
    pub continuous: bool,
    pub tick_mark_count: i64,
    pub snap_to_ticks: bool,
    pub enabled: bool,
    pub on_change: Option<Box<dyn Fn(f64)>>,
}

pub struct StepperConfig {
    pub min: f64,
    pub max: f64,
    pub value: f64,
    pub increment: f64,
    pub wraps: bool,
    pub autorepeat: bool,
    pub enabled: bool,
    pub on_change: Option<Box<dyn Fn(f64)>>,
}

pub struct SegmentedControlConfig<'a> {
    pub labels: &'a [&'a str],
    pub selected_index: Option<usize>,
    pub border_shape: i64,
    pub control_size: u64,
    pub images: &'a [(usize, &'a str)],
    pub enabled: bool,
    pub on_select: Option<Box<dyn Fn(usize)>>,
}

pub struct PopupButtonConfig<'a> {
    pub items: &'a [&'a str],
    pub selected_index: usize,
    pub enabled: bool,
    pub on_select: Option<Box<dyn Fn(usize)>>,
}

pub struct ProgressConfig {
    pub style: i64,
    pub indeterminate: bool,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub animating: bool,
    pub display_when_stopped: bool,
}

pub struct SearchFieldConfig<'a> {
    pub placeholder: &'a str,
    pub value: &'a str,
    pub identifier: Option<&'a str>,
    pub sends_immediately: bool,
    pub sends_whole_string: bool,
    pub enabled: bool,
    pub callbacks: TextFieldCallbacks,
}

pub struct TextFieldConfig<'a> {
    pub placeholder: &'a str,
    pub value: &'a str,
    pub secure: bool,
    pub font_size: Option<f64>,
    pub alignment: Option<u64>,
    pub bezel_style: Option<i64>,
    pub enabled: bool,
    pub callbacks: TextFieldCallbacks,
}

pub struct TextFieldCallbacks {
    pub on_change: Option<Box<dyn Fn(String)>>,
    pub on_begin_editing: Option<Box<dyn Fn()>>,
    pub on_end_editing: Option<Box<dyn Fn(String)>>,
    pub on_submit: Option<Box<dyn Fn(String)>>,
    pub on_move_up: Option<Box<dyn Fn()>>,
    pub on_move_down: Option<Box<dyn Fn()>>,
    pub on_cancel: Option<Box<dyn Fn()>>,
}

pub struct ComboBoxConfig<'a> {
    pub items: &'a [&'a str],
    pub selected_index: usize,
    pub editable: bool,
    pub completes: bool,
    pub value: Option<&'a str>,
    pub enabled: bool,
    pub callbacks: ComboBoxCallbacks,
}

pub struct ComboBoxCallbacks {
    pub on_select: Option<Box<dyn Fn(usize)>>,
    pub on_change: Option<Box<dyn Fn(String)>>,
    pub on_submit: Option<Box<dyn Fn(String)>>,
}

pub struct ImageViewConfig<'a> {
    pub sf_symbol: Option<&'a str>,
    pub sf_symbol_config: Option<(&'a str, f64, i64)>,
    pub image_data: Option<&'a [u8]>,
    pub scaling: Option<i64>,
    pub tint_color: Option<(f64, f64, f64, f64)>,
    pub enabled: bool,
}

pub struct TabViewConfig<'a> {
    pub labels: &'a [&'a str],
    pub selected_index: usize,
    pub enabled: bool,
    pub on_select: Option<Box<dyn Fn(usize)>>,
}

pub struct TableViewConfig<'a> {
    pub column_title: Option<&'a str>,
    pub column_width: Option<f64>,
    pub items: &'a [&'a str],
    pub selected_index: Option<usize>,
    pub row_height: Option<f64>,
    pub row_size_style: Option<i64>,
    pub style: Option<i64>,
    pub highlight_style: Option<i64>,
    pub grid_style: Option<u64>,
    pub alternating_rows: bool,
    pub multiple_selection: bool,
    pub show_header: bool,
    pub on_select: Option<Box<dyn Fn(usize)>>,
}

pub struct OutlineViewConfig<'a> {
    pub nodes: &'a [NativeOutlineNodeData],
    pub selected_row: Option<usize>,
    pub expand_all: bool,
    pub highlight_style: Option<i64>,
    pub row_height: Option<f64>,
    pub on_select: Option<Box<dyn Fn((usize, String))>>,
}

pub struct NativeOutlineNodeData {
    pub title: String,
    pub children: Vec<NativeOutlineNodeData>,
}

pub struct CollectionViewConfig<'a> {
    pub width: f64,
    pub columns: usize,
    pub item_height: f64,
    pub spacing: f64,
    pub items: &'a [&'a str],
    pub selected: Option<usize>,
    pub item_style: CollectionItemStyle,
    pub on_select: Option<Box<dyn Fn(usize)>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionItemStyle {
    Label,
    Card,
}

pub struct MenuButtonConfig<'a> {
    pub title: &'a str,
    pub context_menu: bool,
    pub items: &'a [NativeMenuItemData],
    pub enabled: bool,
    pub on_select: Option<Box<dyn Fn(usize)>>,
}

pub enum NativeMenuItemData {
    Action {
        title: String,
        enabled: bool,
        icon: Option<String>,
    },
    Submenu {
        title: String,
        enabled: bool,
        icon: Option<String>,
        items: Vec<NativeMenuItemData>,
    },
    Separator,
}

pub struct VisualEffectViewConfig {
    pub material: i64,
    pub blending_mode: i64,
    pub state: i64,
    pub emphasized: bool,
    pub corner_radius: f64,
}

pub struct GlassEffectViewConfig {
    pub style: i64,
    pub corner_radius: f64,
    pub tint_color: Option<(f64, f64, f64, f64)>,
}

pub struct TrackingViewConfig {
    pub callbacks: TrackingViewCallbacks,
}

pub struct TrackingViewCallbacks {
    pub on_enter: Option<Box<dyn Fn()>>,
    pub on_exit: Option<Box<dyn Fn()>>,
    pub on_move: Option<Box<dyn Fn(f64, f64)>>,
}

pub struct StackViewConfig {
    pub orientation: i64,
    pub spacing: f64,
    pub alignment: i64,
    pub distribution: i64,
    pub edge_insets: (f64, f64, f64, f64),
    pub detach_hidden: bool,
    pub children: Vec<*mut c_void>,
}

pub struct SidebarViewConfig<'a> {
    pub sidebar_width: f64,
    pub min_width: f64,
    pub max_width: f64,
    pub collapsed: bool,
    pub expanded_width: f64,
    pub embed_in_host: bool,
    pub items: &'a [&'a str],
    pub selected_index: Option<usize>,
    pub header_title: Option<&'a str>,
    pub header_button_symbols: &'a [&'a str],
    pub background_color: Option<(f64, f64, f64, f64)>,
    pub on_select: Option<Box<dyn Fn((usize, String))>>,
    pub on_header_button: Option<Box<dyn Fn(usize)>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertStyle {
    Warning,
    Informational,
    Critical,
}

pub struct AlertConfig<'a> {
    pub style: AlertStyle,
    pub message: &'a str,
    pub informative_text: Option<&'a str>,
    pub button_titles: &'a [&'a str],
    pub shows_suppression_button: bool,
}

pub const ALERT_FIRST_BUTTON_RETURN: i64 = 1000;

pub enum PanelLevel {
    Normal,
    Floating,
    ModalPanel,
    PopUpMenu,
    Custom(i64),
}

pub enum PanelStyle {
    Titled,
    Borderless,
    Hud,
    Utility,
}

pub enum PanelMaterial {
    HudWindow,
    Popover,
    Sidebar,
    UnderWindow,
}

pub struct PanelConfig {
    pub width: f64,
    pub height: f64,
    pub style: PanelStyle,
    pub level: PanelLevel,
    pub non_activating: bool,
    pub has_shadow: bool,
    pub corner_radius: f64,
    pub material: Option<PanelMaterial>,
    pub on_close: Option<Box<dyn Fn()>>,
}

pub struct PopoverConfig {
    pub width: f64,
    pub height: f64,
    pub behavior: i64,
    pub on_close: Option<Box<dyn Fn()>>,
    pub on_show: Option<Box<dyn Fn()>>,
}

// =============================================================================
// The trait — platform crates implement this
// =============================================================================

/// Platform-native control operations. Each method creates or updates a control
/// in-place. The `NativeControlState` tracks the underlying native view — the
/// platform implementation handles creation on first call and updates on
/// subsequent calls by checking `state.is_initialized()`.
pub trait PlatformNativeControls {
    // ── Lifecycle helpers ────────────────────────────────────────────────

    /// Attach a control's view to a parent and position it.
    unsafe fn attach_and_position(
        &self,
        state: &NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
    );

    /// Remove a control from its parent view (called before re-parenting or destruction).
    unsafe fn remove_from_parent(&self, state: &NativeControlState);

    // ── Controls ─────────────────────────────────────────────────────────

    fn update_button(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: ButtonConfig,
    );

    fn update_checkbox(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: CheckboxConfig,
    );

    fn update_switch(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: SwitchConfig,
    );

    fn update_slider(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: SliderConfig,
    );

    fn update_stepper(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: StepperConfig,
    );

    fn update_segmented_control(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: SegmentedControlConfig,
    );

    fn update_popup_button(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: PopupButtonConfig,
    );

    fn update_progress(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: ProgressConfig,
    );

    fn update_search_field(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: SearchFieldConfig,
    );

    fn update_text_field(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: TextFieldConfig,
    );

    fn update_combo_box(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: ComboBoxConfig,
    );

    fn update_image_view(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: ImageViewConfig,
    );

    fn update_tab_view(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: TabViewConfig,
    );

    fn update_table_view(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: TableViewConfig,
    );

    fn update_outline_view(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: OutlineViewConfig,
    );

    fn update_collection_view(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: CollectionViewConfig,
    );

    fn update_menu_button(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: MenuButtonConfig,
    );

    fn update_visual_effect_view(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: VisualEffectViewConfig,
    );

    fn update_glass_effect_view(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: GlassEffectViewConfig,
    );

    fn update_tracking_view(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: TrackingViewConfig,
    );

    fn update_stack_view(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: StackViewConfig,
    );

    fn update_sidebar(
        &self,
        state: &mut NativeControlState,
        parent: *mut c_void,
        bounds: Bounds<Pixels>,
        scale: f32,
        config: SidebarViewConfig,
    );

    fn is_glass_effect_available(&self) -> bool;

    fn get_text_field_value(&self, state: &NativeControlState) -> String;

    fn get_combo_box_value(&self, state: &NativeControlState) -> String;

    // ── Context menu ─────────────────────────────────────────────────────

    fn show_context_menu(
        &self,
        items: &[NativeMenuItemData],
        view: *mut c_void,
        x: f64,
        y: f64,
        on_result: Box<dyn FnOnce(Option<usize>)>,
    );

    // ── Alert ────────────────────────────────────────────────────────────

    fn show_alert_modal(&self, config: AlertConfig) -> i64;

    fn show_alert_sheet(
        &self,
        config: AlertConfig,
        parent_window: *mut c_void,
        callback: Option<Box<dyn FnOnce(i64)>>,
    );

    // ── Panel ────────────────────────────────────────────────────────────

    fn create_panel(&self, config: PanelConfig) -> NativeControlState;

    fn get_panel_content_view(&self, state: &NativeControlState) -> *mut c_void;

    fn show_panel(&self, state: &NativeControlState);

    fn show_panel_centered(&self, state: &NativeControlState);

    fn set_panel_origin(&self, state: &NativeControlState, x: f64, y: f64);

    fn set_panel_top_left(&self, state: &NativeControlState, x: f64, y: f64);

    fn set_panel_size(&self, state: &NativeControlState, width: f64, height: f64);

    fn set_panel_frame(
        &self,
        state: &NativeControlState,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        animate: bool,
    );

    fn close_panel(&self, state: &NativeControlState);

    fn hide_panel(&self, state: &NativeControlState);

    fn is_panel_visible(&self, state: &NativeControlState) -> bool;

    fn get_toolbar_item_frame(&self, window: *mut c_void, item_id: &str) -> Option<Bounds<Pixels>>;

    // ── Popover ──────────────────────────────────────────────────────────

    fn create_popover(&self, config: PopoverConfig) -> NativeControlState;

    fn get_popover_content_view(&self, state: &NativeControlState) -> *mut c_void;

    fn show_popover_at_toolbar_item(&self, state: &NativeControlState, toolbar_item: *mut c_void);

    fn dismiss_popover(&self, state: &NativeControlState);

}
