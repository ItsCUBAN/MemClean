//! Dibuja el icono de la bandeja: un cuadrado de color con el % de RAM en cifras.
//!
//! No usamos fuentes del sistema: cada cifra es una rejilla de 3x5 "cuadritos"
//! que se amplía. Así el número se lee nítido incluso a 16x16 píxeles.

/// Lado del icono en píxeles (Windows lo reduce a 16x16 si la pantalla es al 100 %).
pub const SIZE: u32 = 32;

const CORNER_RADIUS: i32 = 6;

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

/// Cada cifra: 5 filas de 3 bits (1 = cuadrito pintado).
const DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111], // 0
    [0b010, 0b110, 0b010, 0b010, 0b111], // 1
    [0b111, 0b001, 0b111, 0b100, 0b111], // 2
    [0b111, 0b001, 0b111, 0b001, 0b111], // 3
    [0b101, 0b101, 0b111, 0b001, 0b001], // 4
    [0b111, 0b100, 0b111, 0b001, 0b111], // 5
    [0b111, 0b100, 0b111, 0b101, 0b111], // 6
    [0b111, 0b001, 0b001, 0b001, 0b001], // 7
    [0b111, 0b101, 0b111, 0b101, 0b111], // 8
    [0b111, 0b101, 0b111, 0b001, 0b111], // 9
];

/// Devuelve los píxeles RGBA (SIZE x SIZE) del icono para un % dado (0–100).
pub fn render(percent: u8) -> Vec<u8> {
    let percent = percent.min(100);
    let (bg, fg) = Level::from_percent(percent).colors();
    let mut pixels = vec![0u8; (SIZE * SIZE * 4) as usize];

    for y in 0..SIZE as i32 {
        for x in 0..SIZE as i32 {
            if inside_rounded_square(x, y) {
                put(&mut pixels, x, y, bg);
            }
        }
    }

    // Con 3 cifras ("100") hay que encogerlas un poco para que quepan.
    let text = percent.to_string();
    let count = text.len() as i32;
    let scale: i32 = if count >= 3 { 3 } else { 4 };
    let gap = if count >= 3 { 2 } else { scale };
    let (glyph_w, glyph_h) = (3 * scale, 5 * scale);
    let total_w = glyph_w * count + gap * (count - 1);
    let mut left = (SIZE as i32 - total_w) / 2;
    let top = (SIZE as i32 - glyph_h) / 2;

    for ch in text.bytes() {
        let glyph = &DIGITS[usize::from(ch - b'0')];
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..3 {
                if bits & (0b100 >> col) != 0 {
                    let x0 = left + col * scale;
                    let y0 = top + row as i32 * scale;
                    for dy in 0..scale {
                        for dx in 0..scale {
                            put(&mut pixels, x0 + dx, y0 + dy, fg);
                        }
                    }
                }
            }
        }
        left += glyph_w + gap;
    }
    pixels
}

fn put(pixels: &mut [u8], x: i32, y: i32, color: [u8; 4]) {
    if (0..SIZE as i32).contains(&x) && (0..SIZE as i32).contains(&y) {
        let i = ((y as u32 * SIZE + x as u32) * 4) as usize;
        pixels[i..i + 4].copy_from_slice(&color);
    }
}

/// ¿Está el píxel dentro del cuadrado con esquinas redondeadas?
fn inside_rounded_square(x: i32, y: i32) -> bool {
    let max = SIZE as i32 - 1;
    let r = CORNER_RADIUS;
    // Centro del círculo de la esquina más cercana (si el píxel está en una esquina).
    let cx = if x < r { r } else if x > max - r { max - r } else { return true };
    let cy = if y < r { r } else if y > max - r { max - r } else { return true };
    let (dx, dy) = (x - cx, y - cy);
    dx * dx + dy * dy <= r * r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
        let i = ((y * SIZE + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
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
        for p in [0, 7, 42, 99, 100, 255] {
            assert_eq!(render(p).len(), (SIZE * SIZE * 4) as usize);
        }
    }

    #[test]
    fn corners_are_transparent_and_center_has_color() {
        let pixels = render(42);
        assert_eq!(pixel(&pixels, 0, 0)[3], 0);
        assert_eq!(pixel(&pixels, SIZE - 1, SIZE - 1)[3], 0);
        assert_eq!(pixel(&pixels, 1, SIZE / 2)[3], 255);
    }

    #[test]
    fn background_color_follows_level() {
        // El píxel (1, 16) es fondo, nunca cifra.
        assert_eq!(pixel(&render(30), 1, 16), Level::Green.colors().0);
        assert_eq!(pixel(&render(65), 1, 16), Level::Yellow.colors().0);
        assert_eq!(pixel(&render(85), 1, 16), Level::Red.colors().0);
    }

    #[test]
    fn digits_are_drawn() {
        let pixels = render(88);
        let fg = Level::Red.colors().1;
        let painted = (0..SIZE * SIZE)
            .filter(|i| pixel(&pixels, i % SIZE, i / SIZE) == fg)
            .count();
        // "8" usa 13 cuadritos de 4x4 píxeles; dos ochos = 2 * 13 * 16.
        assert_eq!(painted, 2 * 13 * 16);
    }
}
