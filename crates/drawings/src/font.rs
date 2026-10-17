//! Embedded sheet font (Ubuntu Light from egui's default fonts, Ubuntu Font
//! Licence 1.0) with Cyrillic coverage; one metric source for PDF output,
//! the egui preview and layout checks.
use skrifa::{FontRef, GlyphId, MetadataProvider, instance::Size, prelude::LocationRef};

pub const FONT_NAME: &str = "Ubuntu-Light";
pub const FONT: &[u8] = epaint_default_fonts::UBUNTU_LIGHT;

pub struct Font {
    font: FontRef<'static>,
    pub units_per_em: f64,
    pub ascent: f64,
    pub descent: f64,
}

impl Font {
    pub fn load() -> Result<Self, String> {
        let font = FontRef::new(FONT).map_err(|e| format!("Шрифт: {e}"))?;
        let metrics = font.metrics(Size::unscaled(), LocationRef::default());
        Ok(Self {
            units_per_em: metrics.units_per_em as f64,
            ascent: metrics.ascent as f64,
            descent: metrics.descent as f64,
            font,
        })
    }
    pub fn glyph(&self, c: char) -> u16 {
        self.font
            .charmap()
            .map(c)
            .map_or(0, |g| g.to_u32().min(u16::MAX as u32) as u16)
    }
    /// Advance in font units.
    pub fn advance(&self, glyph: u16) -> f64 {
        self.font
            .glyph_metrics(Size::unscaled(), LocationRef::default())
            .advance_width(GlyphId::new(glyph as u32))
            .unwrap_or(0.) as f64
    }
    /// Text width in millimetres for a character height (font size) in mm.
    pub fn width(&self, text: &str, height: f64) -> f64 {
        text.chars()
            .map(|c| self.advance(self.glyph(c)))
            .sum::<f64>()
            / self.units_per_em
            * height
    }
    /// Characters the font cannot draw (mapped to .notdef).
    pub fn missing(&self, text: &str) -> Vec<char> {
        text.chars()
            .filter(|c| !c.is_whitespace() && self.glyph(*c) == 0)
            .collect()
    }
}
