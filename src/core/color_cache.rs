use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use slint::Color;
use crate::core::model::{AppConfig, BrushDefinition, TextMarkerColor};

pub struct ColorCache {
    cache: HashMap<String, Color>,
}

impl ColorCache {
    pub fn new() -> Self {
        let mut inst = Self {
            cache: HashMap::new(),
        };
        inst.prewarm_defaults();
        inst
    }

    pub fn prewarm_defaults(&mut self) {
        // Prewarm all default brushes from AppConfig
        for b in AppConfig::default_brushes() {
            if let Some(c) = Self::parse_hex(&b.hex_value, 255) {
                self.cache.insert(b.key.to_lowercase(), c);
                self.cache.insert(b.hex_value.to_lowercase(), c);
            }
            if let Some(c_semi) = Self::parse_hex(&b.hex_value, 180) {
                self.cache.insert(format!("{}_180", b.key.to_lowercase()), c_semi);
                self.cache.insert(format!("{}_180", b.hex_value.to_lowercase()), c_semi);
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
                    self.cache.insert(format!("{}_180", hex.to_lowercase()), c);
                    self.cache.insert(hex.to_lowercase(), c);
                }
            }
        }

        // Transparent
        self.cache.insert("transparent".into(), Color::from_argb_u8(0, 0, 0, 0));
        self.cache.insert("none".into(), Color::from_argb_u8(0, 0, 0, 0));
        self.cache.insert("keine".into(), Color::from_argb_u8(0, 0, 0, 0));
        self.cache.insert("".into(), Color::from_argb_u8(0, 0, 0, 0));
    }

    pub fn update_from_brushes(&mut self, brushes: &[BrushDefinition]) {
        for b in brushes {
            if let Some(c) = Self::parse_hex(&b.hex_value, 255) {
                self.cache.insert(b.key.to_lowercase(), c);
                self.cache.insert(b.hex_value.to_lowercase(), c);
                if let Some(c_semi) = Self::parse_hex(&b.hex_value, 180) {
                    self.cache.insert(format!("{}_180", b.key.to_lowercase()), c_semi);
                    self.cache.insert(format!("{}_180", b.hex_value.to_lowercase()), c_semi);
                }
            }
        }
    }

    pub fn get_or_parse(&mut self, key_or_hex: &str, alpha: u8) -> Color {
        let trimmed = key_or_hex.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("keine") || trimmed.eq_ignore_ascii_case("none") || trimmed.eq_ignore_ascii_case("transparent") {
            return Color::from_argb_u8(0, 0, 0, 0);
        }
        let lower = trimmed.to_lowercase();
        let cache_key = if alpha == 255 {
            lower.clone()
        } else {
            format!("{}_{}", lower, alpha)
        };

        if let Some(&c) = self.cache.get(&cache_key) {
            return c;
        }

        // Check if we have the 255-alpha color for this named brush key
        if alpha != 255 {
            if let Some(&base) = self.cache.get(&lower) {
                if base.alpha() > 0 {
                    let derived = Color::from_argb_u8(alpha, base.red(), base.green(), base.blue());
                    self.cache.insert(cache_key, derived);
                    return derived;
                }
            }
        }

        let parsed = Self::parse_hex(trimmed, alpha).unwrap_or_else(|| Color::from_argb_u8(0, 0, 0, 0));
        self.cache.insert(cache_key, parsed);
        parsed
    }

    pub fn parse_hex(hex: &str, alpha: u8) -> Option<Color> {
        let clean = hex.trim();
        if clean.starts_with('#') {
            if clean.len() == 7 {
                let r = u8::from_str_radix(&clean[1..3], 16).ok()?;
                let g = u8::from_str_radix(&clean[3..5], 16).ok()?;
                let b = u8::from_str_radix(&clean[5..7], 16).ok()?;
                return Some(Color::from_argb_u8(alpha, r, g, b));
            } else if clean.len() == 9 {
                let a = u8::from_str_radix(&clean[1..3], 16).ok()?;
                let r = u8::from_str_radix(&clean[3..5], 16).ok()?;
                let g = u8::from_str_radix(&clean[5..7], 16).ok()?;
                let b = u8::from_str_radix(&clean[7..9], 16).ok()?;
                return Some(Color::from_argb_u8(a, r, g, b));
            }
        }
        None
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
