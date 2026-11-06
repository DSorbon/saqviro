//! Color Converter.

use saqviro_jobs::Params;

fn numbers(text: &str) -> Vec<f64> {
    text.split(|c: char| !c.is_ascii_digit() && c != '.')
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect()
}

fn from_hsl(h: f64, s: f64, l: f64) -> [u8; 3] {
    let (s, l) = (s / 100.0, l / 100.0);
    let chroma = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let part = chroma * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match (h.rem_euclid(360.0) / 60.0) as u32 {
        0 => (chroma, part, 0.0),
        1 => (part, chroma, 0.0),
        2 => (0.0, chroma, part),
        3 => (0.0, part, chroma),
        4 => (part, 0.0, chroma),
        _ => (chroma, 0.0, part),
    };
    let lift = l - chroma / 2.0;
    [r, g, b].map(|c| ((c + lift) * 255.0).round() as u8)
}

fn to_hsl([r, g, b]: [u8; 3]) -> (f64, f64, f64) {
    let [r, g, b] = [r, g, b].map(|c| f64::from(c) / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let delta = max - min;
    let l = (max + min) / 2.0;
    if delta == 0.0 {
        return (0.0, 0.0, l * 100.0);
    }
    let s = delta / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    (h * 60.0, s * 100.0, l * 100.0)
}

fn parse(text: &str) -> Result<[u8; 3], String> {
    let lower = text.trim().to_lowercase();
    let wrong = || format!("`{}` is not a color, try #ff8800 or rgb(255, 136, 0)", text.trim());
    if lower.starts_with("hsl") {
        let [h, s, l, ..] = numbers(&lower)[..] else {
            return Err(wrong());
        };
        return Ok(from_hsl(h, s, l));
    }
    if lower.starts_with("rgb") || lower.contains(',') {
        let [r, g, b, ..] = numbers(&lower)[..] else {
            return Err(wrong());
        };
        if r > 255.0 || g > 255.0 || b > 255.0 {
            return Err(wrong());
        }
        return Ok([r as u8, g as u8, b as u8]);
    }
    crate::images::tools::parse_color(&lower).map_err(|_| wrong())
}

pub fn run(text: &str, _: &Params) -> Result<String, String> {
    let color = parse(text)?;
    let [r, g, b] = color;
    let (h, s, l) = to_hsl(color);
    Ok(format!(
        "Hex: #{r:02x}{g:02x}{b:02x}\nRGB: rgb({r}, {g}, {b})\nHSL: hsl({h:.0}, {s:.0}%, {l:.0}%)"
    ))
}
