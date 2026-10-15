//! Dibuja el icono de la bandeja: un cuadrado de color con el % de RAM en cifras.
//!
//! No usamos fuentes del sistema: cada cifra es una rejilla de 4x7 píxeles.
//! El icono se dibuja ya al tamaño de la bandeja (16x16 con la pantalla al 100 %),
//! así Windows no tiene que reducirlo y las cifras se ven nítidas.

use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows::Win32::UI::WindowsAndMessaging::SM_CXSMICON;

/// Nivel de uso de RAM, que decide el color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Green,
    Yellow,
    Red,
}

impl Level {
    /// Verde < 60 %, amarillo 60–69 %, rojo >= 70 %.
    pub fn from_percent(percent: u8) -> Self {
        match percent {
            0..=59 => Level::Green,
            60..=69 => Level::Yellow,
            _ => Level::Red,
        }
    }

    /// (fondo, cifras) en RGBA. En amarillo las cifras son oscuras para que se lean bien.
    fn colors(self) -> ([u8; 4], [u8; 4]) {
        const WHITE: [u8; 4] = [255, 255, 255, 255];
        match self {
            Level::Green => ([31, 143, 78, 255], WHITE),
            Level::Yellow => ([242, 192, 55, 255], [27, 27, 27, 255]),
            Level::Red => ([196, 43, 28, 255], WHITE),
        }
    }
}

const GLYPH_W: u32 = 4;
const GLYPH_H: u32 = 7;

/// Cada cifra: 7 filas de 4 bits (1 = píxel pintado).
const DIGITS: [[u8; GLYPH_H as usize]; 10] = [
    [0b0110, 0b1001, 0b1001, 0b1001, 0b1001, 0b1001, 0b0110], // 0
    [0b0010, 0b0110, 0b0010, 0b0010, 0b0010, 0b0010, 0b0111], // 1
    [0b0110, 0b1001, 0b0001, 0b0010, 0b0100, 0b1000, 0b1111], // 2
    [0b0110, 0b1001, 0b0001, 0b0110, 0b0001, 0b1001, 0b0110], // 3
    [0b0010, 0b0110, 0b1010, 0b1010, 0b1111, 0b0010, 0b0010], // 4
    [0b1111, 0b1000, 0b1110, 0b0001, 0b0001, 0b1001, 0b0110], // 5
    [0b0110, 0b1000, 0b1000, 0b1110, 0b1001, 0b1001, 0b0110], // 6
    [0b1111, 0b0001, 0b0010, 0b0010, 0b0100, 0b0100, 0b0100], // 7
    [0b0110, 0b1001, 0b1001, 0b0110, 0b1001, 0b1001, 0b0110], // 8
    [0b0110, 0b1001, 0b1001, 0b0111, 0b0001, 0b0001, 0b0110], // 9
];

/// Lado del icono de la bandeja en píxeles: 16 con la pantalla al 100 %, 24 al 150 %…
pub fn tray_size() -> u32 {
    // SAFETY: solo leen ajustes de la pantalla; no reciben punteros.
    let size = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem()) };
    u32::try_from(size).ok().filter(|s| (16..=64).contains(s)).unwrap_or(16)
}

/// Devuelve los píxeles RGBA (size x size) del icono para un % dado (0–100).
pub fn render(percent: u8, size: u32) -> Vec<u8> {
    let percent = percent.min(100);
    let (bg, fg) = Level::from_percent(percent).colors();

    // Fondo: el cuadrado entero.
    let mut pixels = bg.repeat((size * size) as usize);

    // Las cifras se amplían (x2 al 200 %…) solo si caben enteras.
    let text = percent.to_string();
    let count = text.len() as u32;
    let text_w = count * GLYPH_W + (count - 1);
    let scale = (1..=(size / 16).max(1)).rev().find(|s| text_w * s <= size).unwrap_or(1);
    let mut left = (size.saturating_sub(text_w * scale)) / 2;
    let top = (size + 1).saturating_sub(GLYPH_H * scale) / 2;

    for ch in text.bytes() {
        let glyph = &DIGITS[usize::from(ch - b'0')];
        for (row, bits) in (0..).zip(glyph) {
            for col in 0..GLYPH_W {
                if bits & (0b1000 >> col) == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        put(&mut pixels, size, left + col * scale + dx, top + row * scale + dy, fg);
                    }
                }
            }
        }
        left += (GLYPH_W + 1) * scale;
    }
    pixels
}

fn put(pixels: &mut [u8], size: u32, x: u32, y: u32, color: [u8; 4]) {
    if x < size && y < size {
        let i = ((y * size + x) * 4) as usize;
        pixels[i..i + 4].copy_from_slice(&color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(pixels: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * size + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    }

    fn painted(percent: u8, size: u32) -> usize {
        let pixels = render(percent, size);
        let fg = Level::from_percent(percent).colors().1;
        (0..size * size).filter(|i| pixel(&pixels, size, i % size, i / size) == fg).count()
    }

    #[test]
    fn level_thresholds() {
        assert_eq!(Level::from_percent(0), Level::Green);
        assert_eq!(Level::from_percent(59), Level::Green);
        assert_eq!(Level::from_percent(60), Level::Yellow);
        assert_eq!(Level::from_percent(69), Level::Yellow);
        assert_eq!(Level::from_percent(70), Level::Red);
        assert_eq!(Level::from_percent(100), Level::Red);
    }

    #[test]
    fn buffer_has_right_size() {
        for size in [16, 24, 32] {
            for p in [0, 7, 42, 99, 100, 255] {
                assert_eq!(render(p, size).len(), (size * size * 4) as usize);
            }
        }
    }

    #[test]
    fn background_fills_the_whole_square_with_the_level_color() {
        for (percent, level) in [(30, Level::Green), (65, Level::Yellow), (85, Level::Red)] {
            let pixels = render(percent, 16);
            let bg = level.colors().0;
            for (x, y) in [(0, 0), (15, 0), (0, 15), (15, 15)] {
                assert_eq!(pixel(&pixels, 16, x, y), bg);
            }
        }
    }

    #[test]
    fn digits_are_drawn_whole() {
        // Un "8" pinta 14 píxeles; un "1", 10; un "0", 14.
        assert_eq!(painted(88, 16), 2 * 14);
        // "100" cabe entero a 16x16: no se pierde ningún píxel por el borde.
        assert_eq!(painted(100, 16), 10 + 14 + 14);
    }

    #[test]
    fn digits_grow_only_when_they_fit() {
        // Al 200 % (32x32) las cifras se dibujan al doble: 4 píxeles por cada uno.
        assert_eq!(painted(88, 32), 4 * 2 * 14);
        assert_eq!(painted(100, 32), 4 * (10 + 14 + 14));
        // Al 150 % (24x24), "100" al doble no cabría: se queda a tamaño normal.
        assert_eq!(painted(100, 24), 10 + 14 + 14);
    }

    #[test]
    fn tray_size_is_sane() {
        assert!((16..=64).contains(&tray_size()));
    }
}
