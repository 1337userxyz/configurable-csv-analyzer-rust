use std::sync::{Mutex, OnceLock};
use kit_slint::{parse_hex_with_alpha, Color, ColorCache as KitColorCache};
use crate::core::model::{AppConfig, BrushDefinition, TextMarkerColor};

pub struct ColorCache {
    inner: KitColorCache,
}

impl ColorCache {
    pub fn new() -> Self {
        let mut inst = Self {
            inner: KitColorCache::new(),
        };
        inst.prewarm_defaults();
        inst
    }

    pub fn prewarm_defaults(&mut self) {
        // Prewarm all default brushes from AppConfig
        for b in AppConfig::default_brushes() {
            if let Some(c) = Self::parse_hex(&b.hex_value, 255) {
                self.inner.register(&b.key, c);
                self.inner.register(&b.hex_value, c);
            }
            if let Some(c_semi) = Self::parse_hex(&b.hex_value, 180) {
                self.inner.register(format!("{}_180", b.key.to_lowercase()), c_semi);
                self.inner.register(format!("{}_180", b.hex_value.to_lowercase()), c_semi);
            }
        }

        // Prewarm marker colors (alpha 180 for cell background)
        let markers = [
            TextMarkerColor::Yellow,
            TextMarkerColor::Green,
            TextMarkerColor::Blue,
            TextMarkerColor::Red,
            TextMarkerColor::Orange,
            TextMarkerColor::Purple,
        ];
        for m in markers {
            let hex = m.hex();
            if !hex.is_empty() {
                if let Some(c) = Self::parse_hex(hex, 180) {
                    self.inner.register(format!("{}_180", hex.to_lowercase()), c);
                    self.inner.register(hex.to_lowercase(), c);
                }
            }
        }
    }

    pub fn update_from_brushes(&mut self, brushes: &[BrushDefinition]) {
        for b in brushes {
            if let Some(c) = Self::parse_hex(&b.hex_value, 255) {
                self.inner.register(&b.key, c);
                self.inner.register(&b.hex_value, c);
                if let Some(c_semi) = Self::parse_hex(&b.hex_value, 180) {
                    self.inner.register(format!("{}_180", b.key.to_lowercase()), c_semi);
                    self.inner.register(format!("{}_180", b.hex_value.to_lowercase()), c_semi);
                }
            }
        }
    }

    pub fn get_or_parse(&mut self, key_or_hex: &str, alpha: u8) -> Color {
        self.inner.get_or_parse(key_or_hex, alpha)
    }

    pub fn parse_hex(hex: &str, alpha: u8) -> Option<Color> {
        parse_hex_with_alpha(hex, alpha)
    }
}

static GLOBAL_COLOR_CACHE: OnceLock<Mutex<ColorCache>> = OnceLock::new();

pub fn get_global_color_cache() -> &'static Mutex<ColorCache> {
    GLOBAL_COLOR_CACHE.get_or_init(|| Mutex::new(ColorCache::new()))
}

pub fn cached_color(key_or_hex: &str, alpha: u8) -> Color {
    if let Ok(mut cache) = get_global_color_cache().lock() {
        cache.get_or_parse(key_or_hex, alpha)
    } else {
        ColorCache::parse_hex(key_or_hex, alpha).unwrap_or_else(|| Color::from_argb_u8(0, 0, 0, 0))
    }
}
