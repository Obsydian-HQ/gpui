use crate::{
    Bounds, DevicePixels, Font, FontFallbacks, FontFeatures, FontId, FontMetrics, FontRun,
    FontStyle, FontWeight, GlyphId, LineLayout, Pixels, PlatformTextSystem, Point,
    RenderGlyphParams, Result, SUBPIXEL_VARIANTS_X, ShapedGlyph, ShapedRun, SharedString, Size,
    TextRenderingMode, point, px, size, swap_rgba_pa_to_bgra,
};
use anyhow::anyhow;
use collections::HashMap;
use core_foundation::{
    array::{CFArray, CFArrayRef},
    attributed_string::CFMutableAttributedString,
    base::{CFRange, TCFType},
    number::CFNumber,
    string::CFString,
};
use core_graphics::{
    base::{CGFloat, CGGlyph, kCGImageAlphaPremultipliedLast},
    color_space::CGColorSpace,
    context::{CGContext, CGTextDrawingMode},
    geometry::CGPoint,
};
use core_text::{
    font::CTFont,
    font_collection::CTFontCollectionRef,
    font_descriptor::{
        CTFontDescriptor, kCTFontSlantTrait, kCTFontSymbolicTrait, kCTFontWeightTrait,
        kCTFontWidthTrait,
    },
    line::CTLine,
    string_attributes::kCTFontAttributeName,
};
use font_kit::{
    font::Font as FontKitFont,
    handle::Handle,
    hinting::HintingOptions,
    metrics::Metrics,
    properties::{Style as FontkitStyle, Weight as FontkitWeight},
    source::SystemSource,
    sources::mem::MemSource,
};
use parking_lot::{RwLock, RwLockUpgradableReadGuard};
use pathfinder_geometry::{
    rect::{RectF, RectI},
    transform2d::Transform2F,
    vector::{Vector2F, Vector2I},
};
use smallvec::SmallVec;
use std::{borrow::Cow, char, convert::TryFrom, sync::Arc};

use super::open_type::apply_features_and_fallbacks;

#[allow(non_upper_case_globals)]
const kCGImageAlphaOnly: u32 = 7;

pub(crate) struct IosTextSystem(RwLock<IosTextSystemState>);

#[derive(Clone, PartialEq, Eq, Hash)]
struct FontKey {
    font_family: SharedString,
    font_features: FontFeatures,
    font_fallbacks: Option<FontFallbacks>,
}

struct IosTextSystemState {
    memory_source: MemSource,
    system_source: SystemSource,
    fonts: Vec<FontKitFont>,
    font_selections: HashMap<Font, FontId>,
    font_ids_by_postscript_name: HashMap<String, FontId>,
    font_ids_by_font_key: HashMap<FontKey, SmallVec<[FontId; 4]>>,
    postscript_names_by_font_id: HashMap<FontId, String>,
}

impl IosTextSystem {
    pub(crate) fn new() -> Self {
        log::info!("[IosTextSystem] initializing");
        let system_source = SystemSource::new();
        log::info!("[IosTextSystem] SystemSource created");

        // Probe: can we find any fonts at all?
        match system_source.select_family_by_name("Helvetica") {
            Ok(family) => log::info!(
                "[IosTextSystem] probe: Helvetica found ({} fonts)",
                family.fonts().len()
            ),
            Err(e) => log::warn!("[IosTextSystem] probe: Helvetica NOT found: {}", e),
        }
        match system_source.select_family_by_name(".AppleSystemUIFont") {
            Ok(family) => log::info!(
                "[IosTextSystem] probe: .AppleSystemUIFont found ({} fonts)",
                family.fonts().len()
            ),
            Err(e) => log::warn!(
                "[IosTextSystem] probe: .AppleSystemUIFont NOT found: {}",
                e
            ),
        }

        Self(RwLock::new(IosTextSystemState {
            memory_source: MemSource::empty(),
            system_source,
            fonts: Vec::new(),
            font_selections: HashMap::default(),
            font_ids_by_postscript_name: HashMap::default(),
            font_ids_by_font_key: HashMap::default(),
            postscript_names_by_font_id: HashMap::default(),
        }))
    }
}

impl PlatformTextSystem for IosTextSystem {
    fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> Result<()> {
        self.0.write().add_fonts(fonts)
    }

    fn all_font_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        let collection = core_text::font_collection::create_for_all_families();
        unsafe extern "C" {
            fn CTFontCollectionCreateMatchingFontDescriptors(
                collection: CTFontCollectionRef,
            ) -> CFArrayRef;
        }
        let descriptors: Option<CFArray<CTFontDescriptor>> = unsafe {
            let array_ref =
                CTFontCollectionCreateMatchingFontDescriptors(collection.as_concrete_TypeRef());
            if array_ref.is_null() {
                None
            } else {
                Some(CFArray::wrap_under_create_rule(array_ref))
            }
        };
        let Some(descriptors) = descriptors else {
            return names;
        };
        for descriptor in descriptors.into_iter() {
            names.extend(lenient_font_attributes::family_name(&descriptor));
        }
        if let Ok(fonts_in_memory) = self.0.read().memory_source.all_families() {
            names.extend(fonts_in_memory);
        }
        names
    }

    fn font_id(&self, font: &Font) -> Result<FontId> {
        let lock = self.0.upgradable_read();
        if let Some(font_id) = lock.font_selections.get(font) {
            Ok(*font_id)
        } else {
            log::debug!("[IosTextSystem] font_id lookup: family={:?}", font.family);
            let mut lock = RwLockUpgradableReadGuard::upgrade(lock);
            let font_key = FontKey {
                font_family: font.family.clone(),
                font_features: font.features.clone(),
                font_fallbacks: font.fallbacks.clone(),
            };
            let candidates = if let Some(font_ids) = lock.font_ids_by_font_key.get(&font_key) {
                font_ids.as_slice()
            } else {
                match lock.load_family(&font.family, &font.features, font.fallbacks.as_ref()) {
                    Ok(ids) => {
                        log::info!(
                            "[IosTextSystem] loaded family {:?}: {} candidates",
                            font.family,
                            ids.len()
                        );
                        lock.font_ids_by_font_key.insert(font_key.clone(), ids);
                    }
                    Err(e) => {
                        log::error!(
                            "[IosTextSystem] FAILED to load family {:?}: {}",
                            font.family,
                            e
                        );
                        return Err(e);
                    }
                }
                lock.font_ids_by_font_key[&font_key].as_ref()
            };

            let candidate_properties = candidates
                .iter()
                .map(|font_id| lock.fonts[font_id.0].properties())
                .collect::<SmallVec<[_; 4]>>();

            let ix = font_kit::matching::find_best_match(
                &candidate_properties,
                &font_kit::properties::Properties {
                    style: font.style.into(),
                    weight: font.weight.into(),
                    stretch: Default::default(),
                },
            )?;

            let font_id = candidates[ix];
            lock.font_selections.insert(font.clone(), font_id);
            Ok(font_id)
        }
    }

    fn font_metrics(&self, font_id: FontId) -> FontMetrics {
        self.0.read().fonts[font_id.0].metrics().into()
    }

    fn typographic_bounds(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Bounds<f32>> {
        Ok(self.0.read().fonts[font_id.0]
            .typographic_bounds(glyph_id.0)?
            .into())
    }

    fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Size<f32>> {
        self.0.read().advance(font_id, glyph_id)
    }

    fn glyph_for_char(&self, font_id: FontId, ch: char) -> Option<GlyphId> {
        self.0.read().glyph_for_char(font_id, ch)
    }

    fn glyph_raster_bounds(&self, params: &RenderGlyphParams) -> Result<Bounds<DevicePixels>> {
        self.0.read().raster_bounds(params)
    }

    fn rasterize_glyph(
        &self,
        glyph_id: &RenderGlyphParams,
        raster_bounds: Bounds<DevicePixels>,
    ) -> Result<(Size<DevicePixels>, Vec<u8>)> {
        self.0.read().rasterize_glyph(glyph_id, raster_bounds)
    }

    fn layout_line(&self, text: &str, font_size: Pixels, font_runs: &[FontRun]) -> LineLayout {
        self.0.write().layout_line(text, font_size, font_runs)
    }

    fn recommended_rendering_mode(
        &self,
        _font_id: FontId,
        _font_size: Pixels,
    ) -> TextRenderingMode {
        TextRenderingMode::Grayscale
    }
}

impl IosTextSystemState {
    fn add_fonts(&mut self, fonts: Vec<Cow<'static, [u8]>>) -> Result<()> {
        let fonts = fonts
            .into_iter()
            .map(|bytes| match bytes {
                Cow::Borrowed(embedded_font) => {
                    let data_provider = unsafe {
                        core_graphics::data_provider::CGDataProvider::from_slice(embedded_font)
                    };
                    let font = core_graphics::font::CGFont::from_data_provider(data_provider)
                        .map_err(|()| anyhow!("Could not load an embedded font."))?;
                    let font = font_kit::loaders::core_text::Font::from_core_graphics_font(font);
                    Ok(Handle::from_native(&font))
                }
                Cow::Owned(bytes) => Ok(Handle::from_memory(Arc::new(bytes), 0)),
            })
            .collect::<Result<Vec<_>>>()?;
        self.memory_source.add_fonts(fonts.into_iter())?;
        Ok(())
    }

    fn load_family(
        &mut self,
        name: &str,
        features: &FontFeatures,
        fallbacks: Option<&FontFallbacks>,
    ) -> Result<SmallVec<[FontId; 4]>> {
        let name = crate::text_system::font_name_with_fallbacks(name, ".AppleSystemUIFont");

        let mut font_ids = SmallVec::new();
        let family = self
            .memory_source
            .select_family_by_name(name)
            .or_else(|_| self.system_source.select_family_by_name(name))?;
        for font in family.fonts() {
            let mut font = font.load()?;

            apply_features_and_fallbacks(&mut font, features, fallbacks)?;

            {
                let has_m_glyph = font.glyph_for_char('m').is_some();
                let is_segoe_fluent_icons = font.full_name() == "Segoe Fluent Icons";

                if !has_m_glyph && !is_segoe_fluent_icons {
                    log::warn!(
                        "font '{}' has no 'm' character and was not loaded",
                        font.full_name()
                    );
                    continue;
                }
            }

            let traits = font.native_font().all_traits();
            if unsafe {
                !(traits
                    .get(kCTFontSymbolicTrait)
                    .downcast::<CFNumber>()
                    .is_some()
                    && traits
                        .get(kCTFontWidthTrait)
                        .downcast::<CFNumber>()
                        .is_some()
                    && traits
                        .get(kCTFontWeightTrait)
                        .downcast::<CFNumber>()
                        .is_some()
                    && traits
                        .get(kCTFontSlantTrait)
                        .downcast::<CFNumber>()
                        .is_some())
            } {
                log::error!(
                    "Failed to read traits for font {:?}",
                    font.postscript_name().unwrap()
                );
                continue;
            }

            let font_id = FontId(self.fonts.len());
            font_ids.push(font_id);
            let postscript_name = font.postscript_name().unwrap();
            self.font_ids_by_postscript_name
                .insert(postscript_name.clone(), font_id);
            self.postscript_names_by_font_id
                .insert(font_id, postscript_name);
            self.fonts.push(font);
        }
        Ok(font_ids)
    }

    fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Size<f32>> {
        Ok(self.fonts[font_id.0].advance(glyph_id.0)?.into())
    }

    fn glyph_for_char(&self, font_id: FontId, ch: char) -> Option<GlyphId> {
        self.fonts[font_id.0].glyph_for_char(ch).map(GlyphId)
    }

    fn id_for_native_font(&mut self, requested_font: CTFont) -> FontId {
        let postscript_name = requested_font.postscript_name();
        if let Some(font_id) = self.font_ids_by_postscript_name.get(&postscript_name) {
            *font_id
        } else {
            let font_id = FontId(self.fonts.len());
            self.font_ids_by_postscript_name
                .insert(postscript_name.clone(), font_id);
            self.postscript_names_by_font_id
                .insert(font_id, postscript_name);
            self.fonts
                .push(font_kit::font::Font::from_core_graphics_font(
                    requested_font.copy_to_CGFont(),
                ));
            font_id
        }
    }

    fn is_emoji(&self, font_id: FontId) -> bool {
        self.postscript_names_by_font_id
            .get(&font_id)
            .is_some_and(|postscript_name| {
                postscript_name == "AppleColorEmoji" || postscript_name == ".AppleColorEmojiUI"
            })
    }

    fn raster_bounds(&self, params: &RenderGlyphParams) -> Result<Bounds<DevicePixels>> {
        let font = &self.fonts[params.font_id.0];
        let scale = Transform2F::from_scale(params.scale_factor);
        Ok(font
            .raster_bounds(
                params.glyph_id.0,
                params.font_size.into(),
                scale,
                HintingOptions::None,
                font_kit::canvas::RasterizationOptions::GrayscaleAa,
            )?
            .into())
    }

    fn rasterize_glyph(
        &self,
        params: &RenderGlyphParams,
        glyph_bounds: Bounds<DevicePixels>,
    ) -> Result<(Size<DevicePixels>, Vec<u8>)> {
        if glyph_bounds.size.width.0 == 0 || glyph_bounds.size.height.0 == 0 {
            anyhow::bail!("glyph bounds are empty");
        } else {
            let mut bitmap_size = glyph_bounds.size;
            if params.subpixel_variant.x > 0 {
                bitmap_size.width += DevicePixels(1);
            }
            if params.subpixel_variant.y > 0 {
                bitmap_size.height += DevicePixels(1);
            }
            let bitmap_size = bitmap_size;

            let mut bytes;
            let cx;
            if params.is_emoji {
                bytes = vec![0; bitmap_size.width.0 as usize * 4 * bitmap_size.height.0 as usize];
                cx = CGContext::create_bitmap_context(
                    Some(bytes.as_mut_ptr() as *mut _),
                    bitmap_size.width.0 as usize,
                    bitmap_size.height.0 as usize,
                    8,
                    bitmap_size.width.0 as usize * 4,
                    &CGColorSpace::create_device_rgb(),
                    kCGImageAlphaPremultipliedLast,
                );
            } else {
                bytes = vec![0; bitmap_size.width.0 as usize * bitmap_size.height.0 as usize];
                cx = CGContext::create_bitmap_context(
                    Some(bytes.as_mut_ptr() as *mut _),
                    bitmap_size.width.0 as usize,
                    bitmap_size.height.0 as usize,
                    8,
                    bitmap_size.width.0 as usize,
                    &CGColorSpace::create_device_gray(),
                    kCGImageAlphaOnly,
                );
            }

            cx.translate(
                -glyph_bounds.origin.x.0 as CGFloat,
                (glyph_bounds.origin.y.0 + glyph_bounds.size.height.0) as CGFloat,
            );
            cx.scale(
                params.scale_factor as CGFloat,
                params.scale_factor as CGFloat,
            );

            let subpixel_shift = params
                .subpixel_variant
                .map(|v| v as f32 / SUBPIXEL_VARIANTS_X as f32);
            cx.set_text_drawing_mode(CGTextDrawingMode::CGTextFill);
            cx.set_gray_fill_color(0.0, 1.0);
            cx.set_allows_antialiasing(true);
            cx.set_should_antialias(true);
            cx.set_allows_font_subpixel_positioning(true);
            cx.set_should_subpixel_position_fonts(true);
            cx.set_allows_font_subpixel_quantization(false);
            cx.set_should_subpixel_quantize_fonts(false);
            self.fonts[params.font_id.0]
                .native_font()
                .clone_with_font_size(f32::from(params.font_size) as CGFloat)
                .draw_glyphs(
                    &[params.glyph_id.0 as CGGlyph],
                    &[CGPoint::new(
                        (subpixel_shift.x / params.scale_factor) as CGFloat,
                        (subpixel_shift.y / params.scale_factor) as CGFloat,
                    )],
                    cx,
                );

            if params.is_emoji {
                for pixel in bytes.chunks_exact_mut(4) {
                    swap_rgba_pa_to_bgra(pixel);
                }
            }

            Ok((bitmap_size, bytes))
        }
    }

    fn layout_line(&mut self, text: &str, font_size: Pixels, font_runs: &[FontRun]) -> LineLayout {
        let mut string = CFMutableAttributedString::new();
        let mut max_ascent = 0.0f32;
        let mut max_descent = 0.0f32;

        {
            let mut text = text;
            let mut break_ligature = true;
            for run in font_runs {
                let text_run;
                (text_run, text) = text.split_at(run.len);

                let utf16_start = string.char_len();
                string.replace_str(&CFString::new(text_run), CFRange::init(utf16_start, 0));
                let utf16_end = string.char_len();

                let length = utf16_end - utf16_start;
                let cf_range = CFRange::init(utf16_start, length);
                let font = &self.fonts[run.font_id.0];

                let font_metrics = font.metrics();
                let font_scale = font_size.0 / font_metrics.units_per_em as f32;
                max_ascent = max_ascent.max(font_metrics.ascent * font_scale);
                max_descent = max_descent.max(-font_metrics.descent * font_scale);

                let font_size = if break_ligature {
                    px(font_size.0.next_up())
                } else {
                    font_size
                };
                unsafe {
                    string.set_attribute(
                        cf_range,
                        kCTFontAttributeName,
                        &font.native_font().clone_with_font_size(font_size.into()),
                    );
                }
                break_ligature = !break_ligature;
            }
        }

        let line = CTLine::new_with_attributed_string(string.as_concrete_TypeRef());
        let glyph_runs = line.glyph_runs();
        let mut runs = <Vec<ShapedRun>>::with_capacity(glyph_runs.len() as usize);
        let mut ix_converter = StringIndexConverter::new(text);
        for run in glyph_runs.into_iter() {
            let attributes = run.attributes().unwrap();
            let font = unsafe {
                attributes
                    .get(kCTFontAttributeName)
                    .downcast::<CTFont>()
                    .unwrap()
            };
            let font_id = self.id_for_native_font(font);

            let mut glyphs = match runs.last_mut() {
                Some(run) if run.font_id == font_id => &mut run.glyphs,
                _ => {
                    runs.push(ShapedRun {
                        font_id,
                        glyphs: Vec::with_capacity(run.glyph_count().try_into().unwrap_or(0)),
                    });
                    &mut runs.last_mut().unwrap().glyphs
                }
            };
            for ((&glyph_id, position), &glyph_utf16_ix) in run
                .glyphs()
                .iter()
                .zip(run.positions().iter())
                .zip(run.string_indices().iter())
            {
                let glyph_utf16_ix = usize::try_from(glyph_utf16_ix).unwrap();
                if ix_converter.utf16_ix > glyph_utf16_ix {
                    ix_converter = StringIndexConverter::new(text);
                }
                ix_converter.advance_to_utf16_ix(glyph_utf16_ix);
                glyphs.push(ShapedGlyph {
                    id: GlyphId(glyph_id as u32),
                    position: point(position.x as f32, position.y as f32).map(px),
                    index: ix_converter.utf8_ix,
                    is_emoji: self.is_emoji(font_id),
                });
            }
        }
        let typographic_bounds = line.get_typographic_bounds();
        LineLayout {
            runs,
            font_size,
            width: typographic_bounds.width.into(),
            ascent: max_ascent.into(),
            descent: max_descent.into(),
            len: text.len(),
        }
    }
}

#[derive(Debug, Clone)]
struct StringIndexConverter<'a> {
    text: &'a str,
    utf8_ix: usize,
    utf16_ix: usize,
}

impl<'a> StringIndexConverter<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            utf8_ix: 0,
            utf16_ix: 0,
        }
    }

    fn advance_to_utf16_ix(&mut self, utf16_target: usize) {
        for (ix, c) in self.text[self.utf8_ix..].char_indices() {
            if self.utf16_ix >= utf16_target {
                self.utf8_ix += ix;
                return;
            }
            self.utf16_ix += c.len_utf16();
        }
        self.utf8_ix = self.text.len();
    }
}

impl From<Metrics> for FontMetrics {
    fn from(metrics: Metrics) -> Self {
        FontMetrics {
            units_per_em: metrics.units_per_em,
            ascent: metrics.ascent,
            descent: metrics.descent,
            line_gap: metrics.line_gap,
            underline_position: metrics.underline_position,
            underline_thickness: metrics.underline_thickness,
            cap_height: metrics.cap_height,
            x_height: metrics.x_height,
            bounding_box: metrics.bounding_box.into(),
        }
    }
}

impl From<RectF> for Bounds<f32> {
    fn from(rect: RectF) -> Self {
        Bounds {
            origin: point(rect.origin_x(), rect.origin_y()),
            size: size(rect.width(), rect.height()),
        }
    }
}

impl From<RectI> for Bounds<DevicePixels> {
    fn from(rect: RectI) -> Self {
        Bounds {
            origin: point(DevicePixels(rect.origin_x()), DevicePixels(rect.origin_y())),
            size: size(DevicePixels(rect.width()), DevicePixels(rect.height())),
        }
    }
}

impl From<Vector2I> for Size<DevicePixels> {
    fn from(value: Vector2I) -> Self {
        size(value.x().into(), value.y().into())
    }
}

impl From<RectI> for Bounds<i32> {
    fn from(rect: RectI) -> Self {
        Bounds {
            origin: point(rect.origin_x(), rect.origin_y()),
            size: size(rect.width(), rect.height()),
        }
    }
}

impl From<Point<u32>> for Vector2I {
    fn from(size: Point<u32>) -> Self {
        Vector2I::new(size.x as i32, size.y as i32)
    }
}

impl From<Vector2F> for Size<f32> {
    fn from(vec: Vector2F) -> Self {
        size(vec.x(), vec.y())
    }
}

impl From<FontWeight> for FontkitWeight {
    fn from(value: FontWeight) -> Self {
        FontkitWeight(value.0)
    }
}

impl From<FontStyle> for FontkitStyle {
    fn from(style: FontStyle) -> Self {
        match style {
            FontStyle::Normal => FontkitStyle::Normal,
            FontStyle::Italic => FontkitStyle::Italic,
            FontStyle::Oblique => FontkitStyle::Oblique,
        }
    }
}

mod lenient_font_attributes {
    use core_foundation::{
        base::{CFRetain, CFType, TCFType},
        string::{CFString, CFStringRef},
    };
    use core_text::font_descriptor::{
        CTFontDescriptor, CTFontDescriptorCopyAttribute, kCTFontFamilyNameAttribute,
    };

    pub fn family_name(descriptor: &CTFontDescriptor) -> Option<String> {
        unsafe { get_string_attribute(descriptor, kCTFontFamilyNameAttribute) }
    }

    fn get_string_attribute(
        descriptor: &CTFontDescriptor,
        attribute: CFStringRef,
    ) -> Option<String> {
        unsafe {
            let value = CTFontDescriptorCopyAttribute(descriptor.as_concrete_TypeRef(), attribute);
            if value.is_null() {
                return None;
            }

            let value = CFType::wrap_under_create_rule(value);
            assert!(value.instance_of::<CFString>());
            let s = wrap_under_get_rule(value.as_CFTypeRef() as CFStringRef);
            Some(s.to_string())
        }
    }

    unsafe fn wrap_under_get_rule(reference: CFStringRef) -> CFString {
        unsafe {
            assert!(!reference.is_null(), "Attempted to create a NULL object.");
            let reference = CFRetain(reference as *const ::std::os::raw::c_void) as CFStringRef;
            TCFType::wrap_under_create_rule(reference)
        }
    }
}
