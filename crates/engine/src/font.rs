//! Bitmap font 8×8 – atlas textura + UV lookup.
//!
//! Atlas rozložení:
//!   - 16 znaků na řádek, 6 řádků = 96 znaků (ASCII 32–127)
//!   - Každý glyf: 8×8 px → atlas: 128×48 px RGBA
//!   - Pozice glyfů: index = (char as u8 - 32), row = idx/16, col = idx%16

use image::RgbaImage;
use crate::UvRect;

// ── Konstanty atlasu ──────────────────────────────────────────────────────────

pub const GLYPH_W:   u32 = 8;
pub const GLYPH_H:   u32 = 8;
pub const ATLAS_COLS: u32 = 16;
pub const ATLAS_ROWS: u32 = 6;
pub const ATLAS_W:   u32 = ATLAS_COLS * GLYPH_W;  // 128
pub const ATLAS_H:   u32 = ATLAS_ROWS * GLYPH_H;  // 48

// ── Generování atlasu ─────────────────────────────────────────────────────────

/// Vygeneruje RGBA atlas ze font8x8 dat.
///
/// Bílé pixely = nakreslená část glyfů → lze přebarvit color tintem v shaderu.
/// Průhledné pixely = pozadí → nezakrývá nic pod textem.
pub fn build_atlas() -> RgbaImage {
    
    let mut img = RgbaImage::new(ATLAS_W, ATLAS_H);

    // BASIC_LEGACY[i] = glyf pro ASCII kód i (0–127).
    // Nás zajímá 32–127 (printable ASCII).
    let legacy = &font8x8::legacy::BASIC_LEGACY;

    for char_code in 32u8..=127 {
        let idx = (char_code - 32) as u32;
        let atlas_col = idx % ATLAS_COLS;
        let atlas_row = idx / ATLAS_COLS;

        let glyph_bytes: [u8; 8] = legacy[char_code as usize];

        for row in 0..8u32 {
            let byte = glyph_bytes[row as usize];
            for col in 0..8u32 {
                // font8x8: bit 0 = levý pixel každého řádku
                let lit = (byte >> col) & 1 == 1;
                let px = atlas_col * GLYPH_W + col;
                let py = atlas_row * GLYPH_H + row;
                let pixel = if lit {
                    image::Rgba([255, 255, 255, 255]) // bílý = nakresleno
                } else {
                    image::Rgba([0, 0, 0, 0])         // průhledné = pozadí
                };
                img.put_pixel(px, py, pixel);
            }
        }
    }

    img
}

// ── UV lookup ─────────────────────────────────────────────────────────────────

/// Vrátí UV souřadnice v atlasu pro daný znak.
///
/// Neznámé znaky (mimo 32–127) se zobrazí jako '?' (kód 63).
#[inline]
pub fn glyph_uv(c: char) -> UvRect {
    let code = c as u32;
    let idx = if code >= 32 && code <= 127 {
        code - 32
    } else {
        63 - 32 // '?'
    };

    let atlas_col = idx % ATLAS_COLS;
    let atlas_row = idx / ATLAS_COLS;

    UvRect {
        u:  atlas_col as f32 / ATLAS_COLS as f32,
        v:  atlas_row as f32 / ATLAS_ROWS as f32,
        uw: 1.0 / ATLAS_COLS as f32,
        vh: 1.0 / ATLAS_ROWS as f32,
    }
}
