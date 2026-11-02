use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::rgba(r, g, b, 255)
    }

    /// Blends this color over a background RGB pixel:
    /// out = (color * a + bg * (255 - a)) / 255
    pub fn blend_over(&self, bg_r: u8, bg_g: u8, bg_b: u8) -> (u8, u8, u8) {
        if self.a == 255 {
            return (self.r, self.g, self.b);
        }
        if self.a == 0 {
            return (bg_r, bg_g, bg_b);
        }

        let a = self.a as u32;
        let inv_a = 255 - a;

        let r = ((self.r as u32 * a + bg_r as u32 * inv_a) / 255) as u8;
        let g = ((self.g as u32 * a + bg_g as u32 * inv_a) / 255) as u8;
        let b = ((self.b as u32 * a + bg_b as u32 * inv_a) / 255) as u8;

        (r, g, b)
    }

    /// Parses a color string in various formats:
    /// - Hex: #rgb, #rgba, #rrggbb, #rrggbbaa (with or without leading '#')
    /// - Functional: rgb(r, g, b)
    /// - Functional: rgba(r, g, b, a) where a can be float (0.0-1.0), percentage (0-100%), or int (0-255)
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();

        // 1. Functional notation: rgb(r, g, b)
        if let Some(inner) = s.strip_prefix("rgb(").and_then(|rem| rem.strip_suffix(')')) {
            let parts: Vec<&str> = inner.split(',').collect();
            if parts.len() != 3 {
                return Err(format!("Expected 3 components in rgb(), found {}", parts.len()));
            }
            let r = parse_component(parts[0])?;
            let g = parse_component(parts[1])?;
            let b = parse_component(parts[2])?;
            return Ok(Self::rgb(r, g, b));
        }

        // 2. Functional notation: rgba(r, g, b, a)
        if let Some(inner) = s.strip_prefix("rgba(").and_then(|rem| rem.strip_suffix(')')) {
            let parts: Vec<&str> = inner.split(',').collect();
            if parts.len() != 4 {
                return Err(format!("Expected 4 components in rgba(), found {}", parts.len()));
            }
            let r = parse_component(parts[0])?;
            let g = parse_component(parts[1])?;
            let b = parse_component(parts[2])?;
            let a = parse_alpha(parts[3])?;
            return Ok(Self::rgba(r, g, b, a));
        }

        // 3. Hex notation: #rgb, #rgba, #rrggbb, #rrggbbaa
        let hex = s.strip_prefix('#').unwrap_or(s);
        match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).map_err(|e| e.to_string())?;
                let g = u8::from_str_radix(&hex[1..2], 16).map_err(|e| e.to_string())?;
                let b = u8::from_str_radix(&hex[2..3], 16).map_err(|e| e.to_string())?;
                Ok(Self::rgb(r * 17, g * 17, b * 17))
            }
            4 => {
                let r = u8::from_str_radix(&hex[0..1], 16).map_err(|e| e.to_string())?;
                let g = u8::from_str_radix(&hex[1..2], 16).map_err(|e| e.to_string())?;
                let b = u8::from_str_radix(&hex[2..3], 16).map_err(|e| e.to_string())?;
                let a = u8::from_str_radix(&hex[3..4], 16).map_err(|e| e.to_string())?;
                Ok(Self::rgba(r * 17, g * 17, b * 17, a * 17))
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).map_err(|e| e.to_string())?;
                let g = u8::from_str_radix(&hex[2..4], 16).map_err(|e| e.to_string())?;
                let b = u8::from_str_radix(&hex[4..6], 16).map_err(|e| e.to_string())?;
                Ok(Self::rgb(r, g, b))
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).map_err(|e| e.to_string())?;
                let g = u8::from_str_radix(&hex[2..4], 16).map_err(|e| e.to_string())?;
                let b = u8::from_str_radix(&hex[4..6], 16).map_err(|e| e.to_string())?;
                let a = u8::from_str_radix(&hex[6..8], 16).map_err(|e| e.to_string())?;
                Ok(Self::rgba(r, g, b, a))
            }
            _ => Err(format!("Unrecognized color format: '{}'", s)),
        }
    }
}

fn parse_component(s: &str) -> Result<u8, String> {
    s.trim()
        .parse::<u8>()
        .map_err(|e| format!("Invalid color component '{}': {}", s, e))
}

fn parse_alpha(s: &str) -> Result<u8, String> {
    let s = s.trim();

    // Check for percentage, e.g. "50%"
    if let Some(pct) = s.strip_suffix('%') {
        let val: f32 = pct
            .trim()
            .parse()
            .map_err(|e| format!("Invalid percentage alpha '{}': {}", s, e))?;
        return Ok(((val / 100.0) * 255.0).round().clamp(0.0, 255.0) as u8);
    }

    // Check for decimal dot, e.g. "0.5"
    if s.contains('.') {
        let val: f32 = s
            .parse()
            .map_err(|e| format!("Invalid float alpha '{}': {}", s, e))?;
        return Ok((val * 255.0).round().clamp(0.0, 255.0) as u8);
    }

    // Integer string
    let val: f32 = s
        .parse()
        .map_err(|e| format!("Invalid alpha '{}': {}", s, e))?;
    if val <= 1.0 {
        Ok((val * 255.0).round().clamp(0.0, 255.0) as u8)
    } else {
        Ok(val.round().clamp(0.0, 255.0) as u8)
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ColorVisitor;

        impl<'de> serde::de::Visitor<'de> for ColorVisitor {
            type Value = Color;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str(
                    "a color string (#hex, rgb(), rgba()) or array ([r, g, b] or [r, g, b, a])",
                )
            }

            fn visit_str<E>(self, value: &str) -> Result<Color, E>
            where
                E: serde::de::Error,
            {
                Color::parse(value).map_err(serde::de::Error::custom)
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Color, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let r: u8 = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
                let g: u8 = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;
                let b: u8 = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(2, &self))?;
                let a: Option<u8> = seq.next_element()?;

                Ok(Color {
                    r,
                    g,
                    b,
                    a: a.unwrap_or(255),
                })
            }
        }

        deserializer.deserialize_any(ColorVisitor)
    }
}

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.a == 255 {
            write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            write!(
                f,
                "#{:02x}{:02x}{:02x}{:02x}",
                self.r, self.g, self.b, self.a
            )
        }
    }
}

impl Serialize for Color {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex() {
        assert_eq!(Color::parse("#fff").unwrap(), Color::rgb(255, 255, 255));
        assert_eq!(Color::parse("fff").unwrap(), Color::rgb(255, 255, 255));
        assert_eq!(Color::parse("#ffffff").unwrap(), Color::rgb(255, 255, 255));
        assert_eq!(Color::parse("ffffff").unwrap(), Color::rgb(255, 255, 255));
        assert_eq!(Color::parse("#ff0000").unwrap(), Color::rgb(255, 0, 0));
        assert_eq!(Color::parse("#00ff00").unwrap(), Color::rgb(0, 255, 0));
        assert_eq!(Color::parse("#0000ff").unwrap(), Color::rgb(0, 0, 255));
        assert_eq!(Color::parse("#ffffff80").unwrap(), Color::rgba(255, 255, 255, 128));
        assert_eq!(Color::parse("#f008").unwrap(), Color::rgba(255, 0, 0, 136));
    }

    #[test]
    fn test_parse_functional_rgb() {
        assert_eq!(
            Color::parse("rgb(255, 255, 255)").unwrap(),
            Color::rgb(255, 255, 255)
        );
        assert_eq!(
            Color::parse("rgb( 10 , 20 , 30 )").unwrap(),
            Color::rgb(10, 20, 30)
        );
    }

    #[test]
    fn test_parse_functional_rgba() {
        assert_eq!(
            Color::parse("rgba(255, 255, 255, 1.0)").unwrap(),
            Color::rgba(255, 255, 255, 255)
        );
        assert_eq!(
            Color::parse("rgba(255, 255, 255, 0.5)").unwrap(),
            Color::rgba(255, 255, 255, 128)
        );
        assert_eq!(
            Color::parse("rgba(0, 0, 0, 50%)").unwrap(),
            Color::rgba(0, 0, 0, 128)
        );
        assert_eq!(
            Color::parse("rgba(0, 0, 0, 255)").unwrap(),
            Color::rgba(0, 0, 0, 255)
        );
    }

    #[test]
    fn test_blend_over() {
        let black_half = Color::rgba(0, 0, 0, 128);
        let (r, g, b) = black_half.blend_over(200, 100, 50);
        assert!((r as i32 - 100).abs() <= 1);
        assert!((g as i32 - 50).abs() <= 1);
        assert!((b as i32 - 25).abs() <= 1);
    }

    #[test]
    fn test_serde_deserialize() {
        #[derive(Deserialize)]
        struct TestConfig {
            c1: Color,
            c2: Color,
            c3: Color,
            c4: Color,
        }

        let toml_str = r##"
            c1 = "#ff00aa"
            c2 = "rgba(10, 20, 30, 0.5)"
            c3 = [255, 128, 0]
            c4 = [0, 255, 0, 128]
        "##;

        let cfg: TestConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.c1, Color::rgb(255, 0, 170));
        assert_eq!(cfg.c2, Color::rgba(10, 20, 30, 128));
        assert_eq!(cfg.c3, Color::rgb(255, 128, 0));
        assert_eq!(cfg.c4, Color::rgba(0, 255, 0, 128));
    }
}
