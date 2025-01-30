use std::fmt;

use glam::{Vec3, Vec4};

type Tup3 = (f32, f32, f32);
type Tup4 = (f32, f32, f32, f32);

// === RgbaColor === //

#[derive(Copy, Clone, PartialEq)]
pub struct RgbaColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl fmt::Debug for RgbaColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("RgbaColor")
            .field(&self.r)
            .field(&self.g)
            .field(&self.b)
            .field(&self.a)
            .finish()
    }
}

// === Discrete parameters === //

impl RgbaColor {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self::new_vec(Vec4::new(r, g, b, a))
    }
}

// === Array of floats === //

impl RgbaColor {
    pub const fn new_arr([r, g, b, a]: [f32; 4]) -> Self {
        Self::new(r, g, b, a)
    }

    pub const fn new_arr_rgb([r, g, b]: [f32; 3], a: f32) -> Self {
        Self::new(r, g, b, a)
    }

    pub fn arr(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    pub fn arr_rgb(self) -> [f32; 3] {
        [self.r, self.g, self.b]
    }
}

impl From<[f32; 4]> for RgbaColor {
    fn from(value: [f32; 4]) -> Self {
        Self::new_arr(value)
    }
}

impl From<RgbaColor> for [f32; 4] {
    fn from(value: RgbaColor) -> Self {
        value.arr()
    }
}

impl From<[f32; 3]> for RgbaColor {
    fn from(value: [f32; 3]) -> Self {
        Self::new_arr_rgb(value, 1.0)
    }
}

impl From<RgbaColor> for [f32; 3] {
    fn from(value: RgbaColor) -> Self {
        value.arr_rgb()
    }
}

// === Tuple of floats === //

impl RgbaColor {
    pub const fn new_tup((r, g, b, a): Tup4) -> Self {
        Self::new(r, g, b, a)
    }

    pub const fn new_tup_rgb((r, g, b): Tup3, a: f32) -> Self {
        Self::new(r, g, b, a)
    }

    pub fn tup(self) -> Tup4 {
        (self.r, self.g, self.b, self.a)
    }

    pub fn tup_rgb(self) -> Tup3 {
        (self.r, self.g, self.b)
    }
}

impl From<Tup4> for RgbaColor {
    fn from(value: Tup4) -> Self {
        Self::new_tup(value)
    }
}

impl From<RgbaColor> for Tup4 {
    fn from(value: RgbaColor) -> Self {
        value.tup()
    }
}

impl From<Tup3> for RgbaColor {
    fn from(value: Tup3) -> Self {
        Self::new_tup_rgb(value, 1.0)
    }
}

impl From<RgbaColor> for Tup3 {
    fn from(value: RgbaColor) -> Self {
        value.tup_rgb()
    }
}

// === Vectors === //

impl RgbaColor {
    pub const fn new_vec(color: Vec4) -> Self {
        Self {
            r: color.x,
            g: color.y,
            b: color.z,
            a: color.w,
        }
    }

    pub const fn new_vec_rgb(color: Vec3, alpha: f32) -> Self {
        Self::new_vec(Vec4::new(color.x, color.y, color.z, alpha))
    }

    pub const fn vec(self) -> Vec4 {
        Vec4::new(self.r, self.g, self.b, self.a)
    }

    pub const fn vec_rgb(self) -> Vec3 {
        Vec3::new(self.r, self.g, self.b)
    }
}

impl From<Vec4> for RgbaColor {
    fn from(value: Vec4) -> Self {
        Self::new_vec(value)
    }
}

impl From<RgbaColor> for Vec4 {
    fn from(value: RgbaColor) -> Self {
        value.vec()
    }
}

impl From<Vec3> for RgbaColor {
    fn from(value: Vec3) -> Self {
        Self::new_vec_rgb(value, 1.0)
    }
}

impl From<RgbaColor> for Vec3 {
    fn from(value: RgbaColor) -> Self {
        value.vec_rgb()
    }
}

// === Array of bytes === //

impl RgbaColor {
    pub const fn new_bytes([r, g, b, a]: [u8; 4]) -> Self {
        Self::new_vec(Vec4::new(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        ))
    }

    pub const fn new_bytes_rgb([r, g, b]: [u8; 3], alpha: f32) -> Self {
        Self::new_vec_rgb(
            Vec3::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0),
            alpha,
        )
    }

    pub const fn bytes(self) -> [u8; 4] {
        [
            (self.r * 255.0) as u8,
            (self.g * 255.0) as u8,
            (self.b * 255.0) as u8,
            (self.a * 255.0) as u8,
        ]
    }

    pub const fn bytes_rgb(self) -> [u8; 3] {
        [
            (self.r * 255.0) as u8,
            (self.g * 255.0) as u8,
            (self.b * 255.0) as u8,
        ]
    }
}

impl From<[u8; 4]> for RgbaColor {
    fn from(value: [u8; 4]) -> Self {
        Self::new_bytes(value)
    }
}

impl From<RgbaColor> for [u8; 4] {
    fn from(value: RgbaColor) -> Self {
        value.bytes()
    }
}

impl From<[u8; 3]> for RgbaColor {
    fn from(value: [u8; 3]) -> Self {
        Self::new_bytes_rgb(value, 1.0)
    }
}

impl From<RgbaColor> for [u8; 3] {
    fn from(value: RgbaColor) -> Self {
        value.bytes_rgb()
    }
}

// === HSL === //

impl RgbaColor {
    pub const fn new_hsla(h: f32, s: f32, l: f32, a: f32) -> Self {
        Self::new_tup_rgb(hsl_to_rgb(h, s, l), a)
    }

    pub const fn hsla(self) -> Vec4 {
        let (r, g, b) = rgb_to_hsl(self);
        Vec4::new(r, g, b, self.a)
    }

    pub const fn hsl(self) -> Vec3 {
        let (r, g, b) = rgb_to_hsl(self);
        Vec3::new(r, g, b)
    }
}

const fn hsl_to_rgb(h: f32, s: f32, l: f32) -> Tup3 {
    let r;
    let g;
    let b;

    if s == 0.0 {
        r = l;
        g = l;
        b = l;
    } else {
        const fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
            if t < 0.0 {
                t += 1.0
            }
            if t > 1.0 {
                t -= 1.0
            }
            if t < 1.0 / 6.0 {
                return p + (q - p) * 6.0 * t;
            }
            if t < 1.0 / 2.0 {
                return q;
            }
            if t < 2.0 / 3.0 {
                return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
            }
            p
        }

        let q = if l < 0.5 {
            l * (1.0 + s)
        } else {
            l + s - l * s
        };
        let p = 2.0 * l - q;
        r = hue_to_rgb(p, q, h + 1.0 / 3.0);
        g = hue_to_rgb(p, q, h);
        b = hue_to_rgb(p, q, h - 1.0 / 3.0);
    }

    (r, g, b)
}

const fn rgb_to_hsl(color: RgbaColor) -> Tup3 {
    const fn max(a: f32, b: f32) -> f32 {
        if a > b {
            a
        } else {
            b
        }
    }

    const fn min(a: f32, b: f32) -> f32 {
        if a < b {
            a
        } else {
            b
        }
    }

    let mut h: f32;
    let s: f32;
    let l: f32;

    let Vec3 { x: r, y: g, z: b } = color.vec_rgb();

    let max = max(max(r, g), b);
    let min = min(min(r, g), b);

    // Luminosity is the average of the max and min rgb color intensities.
    l = (max + min) / 2.0;

    // Saturation
    let delta: f32 = max - min;
    if delta == 0.0 {
        // it's gray
        return (0.0, 0.0, l);
    }

    // it's not gray
    if l < 0.5 {
        s = delta / (max + min);
    } else {
        s = delta / (2.0 - max - min);
    }

    // Hue
    let r2 = (((max - r) / 6.0) + (delta / 2.0)) / delta;
    let g2 = (((max - g) / 6.0) + (delta / 2.0)) / delta;
    let b2 = (((max - b) / 6.0) + (delta / 2.0)) / delta;

    h = match max {
        x if x == r => b2 - g2,
        x if x == g => (1.0 / 3.0) + r2 - b2,
        _ => (2.0 / 3.0) + g2 - r2,
    };

    // Fix wraparounds
    if h < 0 as f32 {
        h += 1.0;
    } else if h > 1 as f32 {
        h -= 1.0;
    }

    (h, s, l)
}

// === Palette === //

impl RgbaColor {
    pub const LIGHTGRAY: Self = Self::new(0.78, 0.78, 0.78, 1.00);
    pub const GRAY: Self = Self::new(0.51, 0.51, 0.51, 1.00);
    pub const DARKGRAY: Self = Self::new(0.31, 0.31, 0.31, 1.00);
    pub const YELLOW: Self = Self::new(0.99, 0.98, 0.00, 1.00);
    pub const GOLD: Self = Self::new(1.00, 0.80, 0.00, 1.00);
    pub const ORANGE: Self = Self::new(1.00, 0.63, 0.00, 1.00);
    pub const PINK: Self = Self::new(1.00, 0.43, 0.76, 1.00);
    pub const RED: Self = Self::new(0.90, 0.16, 0.22, 1.00);
    pub const MAROON: Self = Self::new(0.75, 0.13, 0.22, 1.00);
    pub const GREEN: Self = Self::new(0.00, 0.89, 0.19, 1.00);
    pub const LIME: Self = Self::new(0.00, 0.62, 0.18, 1.00);
    pub const DARKGREEN: Self = Self::new(0.00, 0.46, 0.17, 1.00);
    pub const SKYBLUE: Self = Self::new(0.40, 0.75, 1.00, 1.00);
    pub const BLUE: Self = Self::new(0.00, 0.47, 0.95, 1.00);
    pub const DARKBLUE: Self = Self::new(0.00, 0.32, 0.67, 1.00);
    pub const PURPLE: Self = Self::new(0.78, 0.48, 1.00, 1.00);
    pub const VIOLET: Self = Self::new(0.53, 0.24, 0.75, 1.00);
    pub const DARKPURPLE: Self = Self::new(0.44, 0.12, 0.49, 1.00);
    pub const BEIGE: Self = Self::new(0.83, 0.69, 0.51, 1.00);
    pub const BROWN: Self = Self::new(0.50, 0.42, 0.31, 1.00);
    pub const DARKBROWN: Self = Self::new(0.30, 0.25, 0.18, 1.00);
    pub const WHITE: Self = Self::new(1.00, 1.00, 1.00, 1.00);
    pub const BLACK: Self = Self::new(0.00, 0.00, 0.00, 1.00);
    pub const BLANK: Self = Self::new(0.00, 0.00, 0.00, 0.00);
    pub const MAGENTA: Self = Self::new(1.00, 0.00, 1.00, 1.00);
}
