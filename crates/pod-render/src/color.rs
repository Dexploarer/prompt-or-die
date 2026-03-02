//! Color utilities and conversions

use glam::FloatExt;
use serde::{Deserialize, Serialize};

/// RGBA color representation
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    /// Create a color from RGBA values (0.0-1.0)
    pub fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Create a color from RGBA values (0-255)
    pub fn rgba_u8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    /// Create a color from hex string
    /// Supports: "#RRGGBB", "#RRGGBBAA", "0xRRGGBB", "0xRRGGBBAA"
    pub fn hex(hex_str: &str) -> Result<Self, String> {
        let hex_str = hex_str.trim().to_lowercase();

        // Remove prefix if present
        let hex_digits = if hex_str.starts_with("#") {
            &hex_str[1..]
        } else if hex_str.starts_with("0x") {
            &hex_str[2..]
        } else {
            &hex_str
        };

        let value = u32::from_str_radix(hex_digits, 16)
            .map_err(|_| format!("Invalid hex color: {}", hex_str))?;

        match hex_digits.len() {
            6 => {
                // RGB
                Ok(Self {
                    r: ((value >> 16) & 0xFF) as f32 / 255.0,
                    g: ((value >> 8) & 0xFF) as f32 / 255.0,
                    b: (value & 0xFF) as f32 / 255.0,
                    a: 1.0,
                })
            }
            8 => {
                // RGBA
                Ok(Self {
                    r: ((value >> 24) & 0xFF) as f32 / 255.0,
                    g: ((value >> 16) & 0xFF) as f32 / 255.0,
                    b: ((value >> 8) & 0xFF) as f32 / 255.0,
                    a: (value & 0xFF) as f32 / 255.0,
                })
            }
            _ => Err(format!(
                "Invalid hex color length: {} (expected 6 or 8 digits)",
                hex_digits.len()
            )),
        }
    }

    /// Create a color from HSL(A) values
    /// h: 0-360 degrees, s: 0-1, l: 0-1, a: 0-1
    pub fn hsla(h: f32, s: f32, l: f32, a: f32) -> Self {
        let h = (h % 360.0) / 360.0;
        let s = s.clamp(0.0, 1.0);
        let l = l.clamp(0.0, 1.0);

        if s == 0.0 {
            // Grayscale
            let gray = l;
            return Self {
                r: gray,
                g: gray,
                b: gray,
                a,
            };
        }

        let q = if l < 0.5 {
            l * (1.0 + s)
        } else {
            l + s - l * s
        };
        let p = 2.0 * l - q;

        Self {
            r: hue_to_rgb(p, q, h + 1.0 / 3.0),
            g: hue_to_rgb(p, q, h),
            b: hue_to_rgb(p, q, h - 1.0 / 3.0),
            a,
        }
    }

    /// Create a color from HSV(A) values
    /// h: 0-360 degrees, s: 0-1, v: 0-1, a: 0-1
    pub fn hsva(h: f32, s: f32, v: f32, a: f32) -> Self {
        let h = (h % 360.0) / 60.0;
        let s = s.clamp(0.0, 1.0);
        let v = v.clamp(0.0, 1.0);

        let c = v * s;
        let x = c * (1.0 - (h % 2.0 - 1.0).abs());
        let m = v - c;

        let (r, g, b) = if h < 1.0 {
            (c, x, 0.0)
        } else if h < 2.0 {
            (x, c, 0.0)
        } else if h < 3.0 {
            (0.0, c, x)
        } else if h < 4.0 {
            (0.0, x, c)
        } else if h < 5.0 {
            (x, 0.0, c)
        } else {
            (c, 0.0, x)
        };

        Self {
            r: r + m,
            g: g + m,
            b: b + m,
            a,
        }
    }

    /// Convert to array for GPU passing
    pub fn to_array(&self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Convert to array of u8 (0-255)
    pub fn to_u8_array(&self) -> [u8; 4] {
        [
            (self.r * 255.0) as u8,
            (self.g * 255.0) as u8,
            (self.b * 255.0) as u8,
            (self.a * 255.0) as u8,
        ]
    }

    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        let [r, g, b, a] = self.to_u8_array();
        format!("#{:02x}{:02x}{:02x}{:02x}", r, g, b, a)
    }

    /// Set alpha channel
    pub fn with_alpha(mut self, a: f32) -> Self {
        self.a = a.clamp(0.0, 1.0);
        self
    }

    /// Multiply by another color (component-wise)
    pub fn multiply(self, other: Color) -> Self {
        Self {
            r: self.r * other.r,
            g: self.g * other.g,
            b: self.b * other.b,
            a: self.a * other.a,
        }
    }

    /// Linear interpolation between two colors
    pub fn lerp(self, other: Color, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: self.r.lerp(other.r, t),
            g: self.g.lerp(other.g, t),
            b: self.b.lerp(other.b, t),
            a: self.a.lerp(other.a, t),
        }
    }

    /// Invert the color
    pub fn invert(self) -> Self {
        Self {
            r: 1.0 - self.r,
            g: 1.0 - self.g,
            b: 1.0 - self.b,
            a: self.a,
        }
    }

    /// Get brightness (perceived luminance)
    pub fn brightness(&self) -> f32 {
        0.299 * self.r + 0.587 * self.g + 0.114 * self.b
    }

    // Predefined colors
    pub const WHITE: Color = Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    pub const BLACK: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const RED: Color = Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const GREEN: Color = Color {
        r: 0.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    };
    pub const BLUE: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };
    pub const YELLOW: Color = Color {
        r: 1.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    };
    pub const CYAN: Color = Color {
        r: 0.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    pub const MAGENTA: Color = Color {
        r: 1.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };
    pub const GRAY: Color = Color {
        r: 0.5,
        g: 0.5,
        b: 0.5,
        a: 1.0,
    };
    pub const LIGHT_GRAY: Color = Color {
        r: 0.75,
        g: 0.75,
        b: 0.75,
        a: 1.0,
    };
    pub const DARK_GRAY: Color = Color {
        r: 0.25,
        g: 0.25,
        b: 0.25,
        a: 1.0,
    };
    pub const TRANSPARENT: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };
}

impl Default for Color {
    fn default() -> Self {
        Color::WHITE
    }
}

impl From<[f32; 4]> for Color {
    fn from(arr: [f32; 4]) -> Self {
        Color::rgba(arr[0], arr[1], arr[2], arr[3])
    }
}

impl From<[f32; 3]> for Color {
    fn from(arr: [f32; 3]) -> Self {
        Color::rgba(arr[0], arr[1], arr[2], 1.0)
    }
}

impl From<[u8; 4]> for Color {
    fn from(arr: [u8; 4]) -> Self {
        Color::rgba_u8(arr[0], arr[1], arr[2], arr[3])
    }
}

impl From<[u8; 3]> for Color {
    fn from(arr: [u8; 3]) -> Self {
        Color::rgba_u8(arr[0], arr[1], arr[2], 255)
    }
}

// Helper for HSL conversion
fn hue_to_rgb(p: f32, q: f32, t: f32) -> f32 {
    let t = if t < 0.0 { t + 1.0 } else if t > 1.0 { t - 1.0 } else { t };

    if t < 1.0 / 6.0 {
        p + (q - p) * 6.0 * t
    } else if t < 1.0 / 2.0 {
        q
    } else if t < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - t) * 6.0
    } else {
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_rgb() {
        let color = Color::hex("#FF0000").unwrap();
        assert!((color.r - 1.0).abs() < 0.01);
        assert!((color.g - 0.0).abs() < 0.01);
        assert!((color.b - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_hex_rgba() {
        let color = Color::hex("#FF0000FF").unwrap();
        assert!((color.a - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_hsla_conversion() {
        let red = Color::hsla(0.0, 1.0, 0.5, 1.0);
        assert!((red.r - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_lerp() {
        let white = Color::WHITE;
        let black = Color::BLACK;
        let mid = white.lerp(black, 0.5);
        assert!((mid.r - 0.5).abs() < 0.01);
    }
}
