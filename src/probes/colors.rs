use crate::parser::Event;
use crate::probe::{Category, Probe, ProbeStatus};

fn normalize_color(value: &str) -> String {
    if let Some(rest) = value.strip_prefix("rgb:") {
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.len() == 3
            && let (Some(r), Some(g), Some(b)) = (
                u16::from_str_radix(&parts[0][..parts[0].len().min(2)], 16).ok(),
                u16::from_str_radix(&parts[1][..parts[1].len().min(2)], 16).ok(),
                u16::from_str_radix(&parts[2][..parts[2].len().min(2)], 16).ok(),
            )
        {
            return format!("#{r:02x}{g:02x}{b:02x}");
        }
    }
    value.to_string()
}

// Relative luminance per IEC 61966-2-1 (sRGB) with WCAG 2.x contrast threshold.
fn srgb_luminance(value: &str) -> Option<f64> {
    let rest = value.strip_prefix("rgb:")?;
    let parts: Vec<&str> = rest.split('/').collect();
    if parts.len() != 3 {
        return None;
    }
    let parse = |s: &str| -> Option<f64> {
        let v = u16::from_str_radix(s, 16).ok()? as f64;
        let max = match s.len() {
            1 => 0xF as f64,
            2 => 0xFF as f64,
            3 => 0xFFF as f64,
            4 => 0xFFFF as f64,
            _ => return None,
        };
        Some(v / max)
    };
    let (r, g, b) = (parse(parts[0])?, parse(parts[1])?, parse(parts[2])?);
    let lin = |c: f64| -> f64 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    Some(0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b))
}

fn decrqss_probe(
    name: &'static str,
    category: Category,
    sgr_set: &'static str,
    marker: &'static str,
) -> Probe {
    Probe {
        name,
        label: None,
        category,
        query: format!("{sgr_set}\x1bP$qm\x1b\\\x1b[0m").into_bytes(),
        is_sentinel: false,
        interpret: Box::new(move |events| {
            let mut saw_valid = false;
            for event in events {
                if let Event::Decrqss { valid, payload } = event
                    && *valid
                {
                    if payload.contains(marker) {
                        return (ProbeStatus::Supported, Some(payload.clone()));
                    }
                    saw_valid = true;
                }
            }
            if saw_valid {
                (ProbeStatus::Unsupported, None)
            } else {
                (ProbeStatus::Unknown, None)
            }
        }),
    }
}

fn osc_color_probe(name: &'static str, osc_index: u16) -> Probe {
    Probe {
        name,
        label: None,
        category: Category::Colors,
        query: format!("\x1b]{osc_index};?\x1b\\").into_bytes(),
        is_sentinel: false,
        interpret: Box::new(move |events| {
            for event in events {
                if let Event::OscColor { index, value, .. } = event
                    && *index == osc_index
                {
                    return (ProbeStatus::Supported, Some(normalize_color(value)));
                }
            }
            (ProbeStatus::Unknown, None)
        }),
    }
}

pub fn probes() -> Vec<Probe> {
    vec![
        osc_color_probe("foreground-color", 10),
        osc_color_probe("background-color", 11),
        osc_color_probe("cursor-color", 12),
        Probe {
            name: "palette-color",
            label: None,
            category: Category::Colors,
            is_sentinel: false,
            // Query palette index 1 (red in standard ANSI palette)
            query: b"\x1b]4;1;?\x1b\\".to_vec(),
            interpret: Box::new(|events| {
                for event in events {
                    if let Event::OscColor {
                        index,
                        sub_index: Some(sub),
                        value,
                    } = event
                        && *index == 4
                        && sub == "1"
                    {
                        return (ProbeStatus::Supported, Some(normalize_color(value)));
                    }
                }
                (ProbeStatus::Unknown, None)
            }),
        },
        Probe {
            name: "dark-light-theme",
            label: None,
            category: Category::Colors,
            is_sentinel: false,
            query: b"\x1b]11;?\x1b\\".to_vec(),
            interpret: Box::new(|events| {
                for event in events {
                    if let Event::OscColor { index, value, .. } = event
                        && *index == 11
                        && let Some(lum) = srgb_luminance(value)
                    {
                        let theme = if lum < 0.179 { "dark" } else { "light" };
                        return (
                            ProbeStatus::Supported,
                            Some(format!("{theme} (L={lum:.3})")),
                        );
                    }
                }
                (ProbeStatus::Unknown, None)
            }),
        },
        Probe {
            name: "osc52-clipboard",
            label: None,
            category: Category::Features,
            is_sentinel: false,
            query: b"\x1b]52;c;?\x1b\\".to_vec(),
            interpret: Box::new(|events| {
                for event in events {
                    if let Event::OscColor {
                        index,
                        sub_index: Some(_),
                        ..
                    } = event
                        && *index == 52
                    {
                        return (ProbeStatus::Supported, None);
                    }
                }
                (ProbeStatus::Unknown, None)
            }),
        },
        decrqss_probe(
            "true-color",
            Category::Colors,
            "\x1b[48;2;150;150;150m",
            "150",
        ),
        decrqss_probe("styled-underline", Category::Styling, "\x1b[4:3m", "4:3"),
        decrqss_probe(
            "underline-color",
            Category::Styling,
            "\x1b[58:2::170:170:170m",
            "170",
        ),
        decrqss_probe("strikethrough", Category::Styling, "\x1b[9m", "9"),
        decrqss_probe("overline", Category::Styling, "\x1b[53m", "53"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_16bit_rgb() {
        assert_eq!(normalize_color("rgb:ffff/0000/8080"), "#ff0080");
    }

    #[test]
    fn normalize_8bit_rgb() {
        assert_eq!(normalize_color("rgb:ff/00/80"), "#ff0080");
    }

    #[test]
    fn passthrough_non_rgb() {
        assert_eq!(normalize_color("cmyk:0/0/0/0"), "cmyk:0/0/0/0");
    }

    #[test]
    fn luminance_black() {
        let l = srgb_luminance("rgb:0000/0000/0000").unwrap();
        assert!((l - 0.0).abs() < 0.001);
    }

    #[test]
    fn luminance_white() {
        let l = srgb_luminance("rgb:ffff/ffff/ffff").unwrap();
        assert!((l - 1.0).abs() < 0.001);
    }

    #[test]
    fn luminance_dark_background() {
        // Typical dark terminal: rgb:1c1c/1c1c/1c1c → L ≈ 0.010
        let l = srgb_luminance("rgb:1c1c/1c1c/1c1c").unwrap();
        assert!(l < 0.179, "expected dark, got L={l}");
    }

    #[test]
    fn luminance_light_background() {
        // Typical light terminal: rgb:f5f5/f5f5/f5f5 → L ≈ 0.91
        let l = srgb_luminance("rgb:f5f5/f5f5/f5f5").unwrap();
        assert!(l > 0.179, "expected light, got L={l}");
    }

    #[test]
    fn luminance_8bit_rgb() {
        let l = srgb_luminance("rgb:ff/ff/ff").unwrap();
        assert!((l - 1.0).abs() < 0.001);
    }
}
