use refineable::Refineable as _;
use std::{ffi::c_void, fs, future::Future, io, sync::Arc};

use crate::{
    AbsoluteLength, App, Asset, AssetLogger, Bounds, DefiniteLength, Element, ElementId,
    GlobalElementId, InspectorElementId, IntoElement, LayoutId, Length, Pixels, Resource,
    SharedString, SharedUri, Style, StyleRefinement, Styled, Window, px,
};
use anyhow::Context as _;
use futures::AsyncReadExt;
use thiserror::Error;

/// SF Symbol weight values matching NSFontWeight constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeImageSymbolWeight {
    /// Ultra-light weight.
    UltraLight,
    /// Thin weight.
    Thin,
    /// Light weight.
    Light,
    /// Regular weight (default).
    #[default]
    Regular,
    /// Medium weight.
    Medium,
    /// Semibold weight.
    Semibold,
    /// Bold weight.
    Bold,
    /// Heavy weight.
    Heavy,
    /// Black weight.
    Black,
}

impl NativeImageSymbolWeight {
    fn to_raw(self) -> i64 {
        match self {
            Self::UltraLight => 1,
            Self::Thin => 2,
            Self::Light => 3,
            Self::Regular => 4,
            Self::Medium => 5,
            Self::Semibold => 6,
            Self::Bold => 7,
            Self::Heavy => 8,
            Self::Black => 9,
        }
    }
}

/// Image scaling modes for NSImageView.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeImageScaling {
    /// No scaling applied.
    #[default]
    None,
    /// Scale image proportionally down to fit.
    ScaleDown,
    /// Scale axes independently to fill.
    ScaleAxesIndependently,
    /// Scale proportionally up or down to fit.
    ScaleUpOrDown,
}

impl NativeImageScaling {
    fn to_raw(self) -> i64 {
        match self {
            Self::None => 0,
            Self::ScaleDown => 1,
            Self::ScaleAxesIndependently => 2,
            Self::ScaleUpOrDown => 3,
        }
    }
}

/// The image source for a NativeImageView.
#[derive(Debug, Clone, PartialEq)]
pub enum NativeImageSource {
    /// An SF Symbol by name, with optional point size and weight.
    SfSymbol {
        /// The SF Symbol name (e.g., "globe", "star.fill").
        name: SharedString,
        /// Point size for the symbol (None = system default).
        point_size: Option<f64>,
        /// Font weight for the symbol.
        weight: NativeImageSymbolWeight,
    },
    /// Raw image data (PNG, JPEG, etc.).
    Data(Vec<u8>),
    /// An image loaded from a URI, file path, or embedded asset resource.
    Resource(Resource),
}

impl From<Resource> for NativeImageSource {
    fn from(value: Resource) -> Self {
        Self::Resource(value)
    }
}

impl From<SharedUri> for NativeImageSource {
    fn from(value: SharedUri) -> Self {
        Self::Resource(Resource::Uri(value))
    }
}

/// Creates a native NSImageView element.
pub fn native_image_view(id: impl Into<ElementId>) -> NativeImageView {
    NativeImageView {
        id: id.into(),
        source: None,
        scaling: NativeImageScaling::default(),
        tint_color: None,
        style: StyleRefinement::default(),
    }
}

/// A GPUI element wrapping NSImageView for displaying SF Symbols and images.
pub struct NativeImageView {
    id: ElementId,
    source: Option<NativeImageSource>,
    scaling: NativeImageScaling,
    tint_color: Option<(f64, f64, f64, f64)>,
    style: StyleRefinement,
}

impl NativeImageView {
    /// Sets the image source.
    pub fn source(mut self, source: NativeImageSource) -> Self {
        self.source = Some(source);
        self
    }

    /// Sets an SF Symbol by name with default size and weight.
    pub fn sf_symbol(mut self, name: impl Into<SharedString>) -> Self {
        self.source = Some(NativeImageSource::SfSymbol {
            name: name.into(),
            point_size: None,
            weight: NativeImageSymbolWeight::default(),
        });
        self
    }

    /// Sets an SF Symbol with specific point size and weight.
    pub fn sf_symbol_config(
        mut self,
        name: impl Into<SharedString>,
        point_size: f64,
        weight: NativeImageSymbolWeight,
    ) -> Self {
        self.source = Some(NativeImageSource::SfSymbol {
            name: name.into(),
            point_size: Some(point_size),
            weight,
        });
        self
    }

    /// Sets the image from raw bytes (PNG, JPEG, etc.).
    pub fn image_data(mut self, data: Vec<u8>) -> Self {
        self.source = Some(NativeImageSource::Data(data));
        self
    }

    /// Sets the image from a GPUI resource.
    pub fn image_resource(mut self, resource: impl Into<Resource>) -> Self {
        self.source = Some(NativeImageSource::Resource(resource.into()));
        self
    }

    /// Sets the image from a URI.
    pub fn image_uri(mut self, uri: impl Into<SharedUri>) -> Self {
        self.source = Some(NativeImageSource::Resource(Resource::Uri(uri.into())));
        self
    }

    /// Sets the image scaling mode.
    pub fn scaling(mut self, scaling: NativeImageScaling) -> Self {
        self.scaling = scaling;
        self
    }

    /// Sets the content tint color (applies to SF Symbols).
    pub fn tint_color(mut self, r: f64, g: f64, b: f64, a: f64) -> Self {
        self.tint_color = Some((r, g, b, a));
        self
    }
}

struct NativeImageViewState {
    view_ptr: *mut c_void,
    current_source: Option<NativeImageSource>,
    current_scaling: NativeImageScaling,
    current_tint: Option<(f64, f64, f64, f64)>,
    attached: bool,
}

impl Drop for NativeImageViewState {
    fn drop(&mut self) {
        if self.attached {
            #[cfg(target_os = "macos")]
            unsafe {
                use crate::platform::native_controls;
                native_controls::release_native_image_view(self.view_ptr as cocoa::base::id);
            }
        }
    }
}

unsafe impl Send for NativeImageViewState {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeImageApplyState {
    Applied,
    Pending,
    Failed,
}

type NativeImageResourceLoader = AssetLogger<NativeImageAssetLoader>;

#[derive(Clone)]
enum NativeImageAssetLoader {}

impl Asset for NativeImageAssetLoader {
    type Source = Resource;
    type Output = Result<Arc<Vec<u8>>, NativeImageLoadError>;

    fn load(
        source: Self::Source,
        cx: &mut App,
    ) -> impl Future<Output = Self::Output> + Send + 'static {
        let client = cx.http_client();
        let asset_source = cx.asset_source().clone();
        async move {
            match source.clone() {
                Resource::Path(path) => Ok(Arc::new(fs::read(path.as_ref())?)),
                Resource::Uri(uri) => {
                    let mut response = client
                        .get(uri.as_ref(), ().into(), true)
                        .await
                        .with_context(|| format!("loading native image resource from {uri:?}"))?;
                    let mut body = Vec::new();
                    response.body_mut().read_to_end(&mut body).await?;
                    if !response.status().is_success() {
                        let mut response_body = String::from_utf8_lossy(&body).into_owned();
                        let first_line = response_body.lines().next().unwrap_or("").trim_end();
                        response_body.truncate(first_line.len());
                        return Err(NativeImageLoadError::BadStatus {
                            uri,
                            status: response.status(),
                            body: response_body,
                        });
                    }
                    Ok(Arc::new(body))
                }
                Resource::Embedded(path) => {
                    let data = asset_source.load(&path).ok().flatten();
                    if let Some(data) = data {
                        Ok(Arc::new(data.to_vec()))
                    } else {
                        Err(NativeImageLoadError::Asset(
                            format!("Embedded resource not found: {path}").into(),
                        ))
                    }
                }
            }
        }
    }
}

#[derive(Debug, Error, Clone)]
enum NativeImageLoadError {
    #[error("error: {0}")]
    Other(Arc<anyhow::Error>),
    #[error("io error: {0}")]
    Io(Arc<io::Error>),
    #[error("unexpected http status for {uri}: {status}, body: {body}")]
    BadStatus {
        uri: SharedUri,
        status: http_client::StatusCode,
        body: String,
    },
    #[error("asset error: {0}")]
    Asset(SharedString),
}

impl From<anyhow::Error> for NativeImageLoadError {
    fn from(value: anyhow::Error) -> Self {
        Self::Other(Arc::new(value))
    }
}

impl From<io::Error> for NativeImageLoadError {
    fn from(value: io::Error) -> Self {
        Self::Io(Arc::new(value))
    }
}

#[cfg(target_os = "macos")]
unsafe fn apply_image_source(
    view: cocoa::base::id,
    source: &NativeImageSource,
    window: &mut Window,
    cx: &mut App,
) -> NativeImageApplyState {
    unsafe {
        use crate::platform::native_controls;
        match source {
            NativeImageSource::SfSymbol {
                name,
                point_size,
                weight,
            } => {
                if let Some(pt) = point_size {
                    native_controls::set_native_image_view_sf_symbol_config(
                        view,
                        name.as_ref(),
                        *pt,
                        weight.to_raw(),
                    );
                } else {
                    native_controls::set_native_image_view_sf_symbol(view, name.as_ref());
                }
                NativeImageApplyState::Applied
            }
            NativeImageSource::Data(data) => {
                native_controls::set_native_image_view_image_from_data(view, data);
                NativeImageApplyState::Applied
            }
            NativeImageSource::Resource(resource) => {
                match window.use_asset::<NativeImageResourceLoader>(resource, cx) {
                    Some(Ok(data)) => {
                        native_controls::set_native_image_view_image_from_data(
                            view,
                            data.as_ref().as_slice(),
                        );
                        NativeImageApplyState::Applied
                    }
                    Some(Err(_)) => {
                        native_controls::clear_native_image_view_image(view);
                        NativeImageApplyState::Failed
                    }
                    None => {
                        native_controls::clear_native_image_view_image(view);
                        NativeImageApplyState::Pending
                    }
                }
            }
        }
    }
}

impl IntoElement for NativeImageView {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeImageView {
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
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(24.0))));
        }
        if matches!(style.size.height, Length::Auto) {
            style.size.height =
                Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(24.0))));
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
        cx: &mut App,
    ) {
        #[cfg(target_os = "macos")]
        {
            use crate::platform::native_controls;

            let native_view = window.raw_native_view_ptr();
            if native_view.is_null() {
                return;
            }

            let source = self.source.take();
            let scaling = self.scaling;
            let tint_color = self.tint_color;

            window.with_optional_element_state::<NativeImageViewState, _>(
                id,
                |prev_state, window| {
                    let state = if let Some(Some(mut element_state)) = prev_state {
                        unsafe {
                            native_controls::set_native_view_frame(
                                element_state.view_ptr as cocoa::base::id,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );

                            if element_state.current_source != source {
                                let source_state = if let Some(ref src) = source {
                                    apply_image_source(
                                        element_state.view_ptr as cocoa::base::id,
                                        src,
                                        window,
                                        cx,
                                    )
                                } else {
                                    native_controls::clear_native_image_view_image(
                                        element_state.view_ptr as cocoa::base::id,
                                    );
                                    NativeImageApplyState::Applied
                                };

                                match source_state {
                                    NativeImageApplyState::Applied
                                    | NativeImageApplyState::Failed => {
                                        element_state.current_source = source;
                                    }
                                    NativeImageApplyState::Pending => {
                                        element_state.current_source = None;
                                    }
                                }
                            }

                            if element_state.current_scaling != scaling {
                                native_controls::set_native_image_view_scaling(
                                    element_state.view_ptr as cocoa::base::id,
                                    scaling.to_raw(),
                                );
                                element_state.current_scaling = scaling;
                            }

                            if element_state.current_tint != tint_color {
                                if let Some((r, g, b, a)) = tint_color {
                                    native_controls::set_native_image_view_content_tint_color(
                                        element_state.view_ptr as cocoa::base::id,
                                        r,
                                        g,
                                        b,
                                        a,
                                    );
                                }
                                element_state.current_tint = tint_color;
                            }
                        }

                        element_state
                    } else {
                        unsafe {
                            let view = native_controls::create_native_image_view();
                            let current_source = if let Some(ref src) = source {
                                match apply_image_source(view, src, window, cx) {
                                    NativeImageApplyState::Applied
                                    | NativeImageApplyState::Failed => source.clone(),
                                    NativeImageApplyState::Pending => None,
                                }
                            } else {
                                None
                            };
                            native_controls::set_native_image_view_scaling(view, scaling.to_raw());
                            if let Some((r, g, b, a)) = tint_color {
                                native_controls::set_native_image_view_content_tint_color(
                                    view, r, g, b, a,
                                );
                            }

                            native_controls::attach_native_view_to_parent(
                                view,
                                native_view as cocoa::base::id,
                            );
                            native_controls::set_native_view_frame(
                                view,
                                bounds,
                                native_view as cocoa::base::id,
                                window.scale_factor(),
                            );

                            NativeImageViewState {
                                view_ptr: view as *mut c_void,
                                current_source,
                                current_scaling: scaling,
                                current_tint: tint_color,
                                attached: true,
                            }
                        }
                    };

                    ((), Some(state))
                },
            );
        }
    }
}

impl Styled for NativeImageView {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
