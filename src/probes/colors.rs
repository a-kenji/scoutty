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

// Strict DECRQSS probe for simple SGR attributes (single/double digit).
// Checks that the marker is the sole non-zero parameter in the response,
// avoiding false positives from compound SGR payloads like "48;2;150;150;150m".
fn decrqss_sgr_attr_probe(
    name: &'static str,
    category: Category,
    sgr_set: &'static str,
    marker: &'static str,
) -> Probe {
    Probe::new(
        name,
        category,
        format!("{sgr_set}\x1bP$qm\x1b\\\x1b[0m").into_bytes(),
        Box::new(move |events| {
            let mut saw_valid = false;
            for event in events {
                if let Event::Decrqss { valid, payload } = event
                    && *valid
                {
                    let params = payload.strip_suffix('m').unwrap_or(payload);
                    let parts: Vec<&str> = params.split(';').collect();
                    let matched = match parts.as_slice() {
                        [p] if *p == marker => true,
                        [zero, p] if *zero == "0" && *p == marker => true,
                        _ => false,
                    };
                    if matched {
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
    )
}

fn decrqss_probe(
    name: &'static str,
    category: Category,
    sgr_set: &'static str,
    marker: &'static str,
) -> Probe {
    Probe::new(
        name,
        category,
        format!("{sgr_set}\x1bP$qm\x1b\\\x1b[0m").into_bytes(),
        Box::new(move |events| {
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
    )
}

// Probe multiple SGR forms for the same capability (e.g. semicolon vs colon
// subparameter variants for RGB colors). Sends each form as its own
// set+DECRQSS+reset sequence and reports supported if any response contains
// the marker.
fn decrqss_multi_probe(
    name: &'static str,
    category: Category,
    sgr_forms: &[&'static str],
    markers: &[&'static str],
) -> Probe {
    let mut query = Vec::new();
    for sgr in sgr_forms {
        query.extend_from_slice(format!("{sgr}\x1bP$qm\x1b\\\x1b[0m").as_bytes());
    }
    let markers: Vec<&'static str> = markers.to_vec();
    Probe::new(
        name,
        category,
        query,
        Box::new(move |events| {
            let mut saw_valid = false;
            let mut matched: Vec<String> = Vec::new();
            for event in events {
                if let Event::Decrqss { valid, payload } = event
                    && *valid
                {
                    if markers.iter().any(|m| payload.contains(m)) {
                        matched.push(payload.clone());
                    }
                    saw_valid = true;
                }
            }
            if !matched.is_empty() {
                matched.sort();
                matched.dedup();
                (ProbeStatus::Supported, Some(matched.join(", ")))
            } else if saw_valid {
                (ProbeStatus::Unsupported, None)
            } else {
                (ProbeStatus::Unknown, None)
            }
        }),
    )
}

fn osc_color_probe(name: &'static str, osc_index: u16) -> Probe {
    Probe::new(
        name,
        Category::Colors,
        format!("\x1b]{osc_index};?\x1b\\").into_bytes(),
        Box::new(move |events| {
            for event in events {
                if let Event::OscColor { index, value, .. } = event
                    && *index == osc_index
                {
                    return (ProbeStatus::Supported, Some(normalize_color(value)));
                }
            }
            (ProbeStatus::Unknown, None)
        }),
    )
}

pub fn probes() -> Vec<Probe> {
    vec![
        osc_color_probe("foreground-color", 10),
        osc_color_probe("background-color", 11),
        osc_color_probe("cursor-color", 12),
        osc_color_probe("selection-bg-color", 17),
        osc_color_probe("selection-fg-color", 19),
        // Query palette index 1 (red in standard ANSI palette)
        Probe::new(
            "palette-color",
            Category::Colors,
            b"\x1b]4;1;?\x1b\\".to_vec(),
            Box::new(|events| {
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
        ),
        Probe::new(
            "dark-light-theme",
            Category::Colors,
            b"\x1b]11;?\x1b\\".to_vec(),
            Box::new(|events| {
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
        ),
        Probe::new(
            "osc52-clipboard",
            Category::Features,
            b"\x1b]52;c;?\x1b\\".to_vec(),
            Box::new(|events| {
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
        ),
        decrqss_multi_probe(
            "true-color",
            Category::Colors,
            &[
                "\x1b[48;2;150;150;150m",  // semicolon (legacy, most compatible)
                "\x1b[48:2::150:150:150m", // 6-subparam colon (ITU T.416)
                "\x1b[48:2:150:150:150m",  // 5-subparam colon (xterm+direct2)
            ],
            &["150;150;150", "150:150:150"],
        ),
        decrqss_probe("styled-underline", Category::Styling, "\x1b[4:3m", "4:3"),
        decrqss_multi_probe(
            "underline-color",
            Category::Styling,
            &[
                "\x1b[58:2::170:170:170m", // 6-subparam colon (ITU T.416)
                "\x1b[58:2:170:170:170m",  // 5-subparam colon (xterm+direct2)
            ],
            &["170:170:170"],
        ),
        decrqss_probe("strikethrough", Category::Styling, "\x1b[9m", "9"),
        decrqss_probe("overline", Category::Styling, "\x1b[53m", "53"),
        decrqss_sgr_attr_probe("italic", Category::Styling, "\x1b[3m", "3"),
        decrqss_sgr_attr_probe("dim", Category::Styling, "\x1b[2m", "2"),
        decrqss_sgr_attr_probe("blink", Category::Styling, "\x1b[5m", "5"),
        decrqss_sgr_attr_probe("reverse", Category::Styling, "\x1b[7m", "7"),
        decrqss_sgr_attr_probe("invisible", Category::Styling, "\x1b[8m", "8"),
        // SGR 21 is "doubly underlined" per ECMA-48, but some terminals
        // (e.g. foot) normalize it to the modern 4:2 subparameter syntax
        // and report back "4:2m" instead of "21m".
        Probe::new(
            "double-underline",
            Category::Styling,
            b"\x1b[21m\x1bP$qm\x1b\\\x1b[0m".to_vec(),
            Box::new(|events| {
                let mut saw_valid = false;
                for event in events {
                    if let Event::Decrqss { valid, payload } = event
                        && *valid
                    {
                        let params = payload.strip_suffix('m').unwrap_or(payload);
                        let parts: Vec<&str> = params.split(';').collect();
                        let matched = match parts.as_slice() {
                            [p] if *p == "21" || *p == "4:2" => true,
                            [zero, p] if *zero == "0" && (*p == "21" || *p == "4:2") => true,
                            _ => false,
                        };
                        if matched {
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
        ),
        // DECSCUSR cursor style query via DECRQSS: DCS $ q SP q ST
        Probe::new(
            "cursor-style-report",
            Category::Styling,
            b"\x1bP$q q\x1b\\".to_vec(),
            Box::new(|events| {
                for event in events {
                    if let Event::Decrqss { valid, payload } = event
                        && *valid
                        && let Some(rest) = payload.strip_suffix(" q")
                    {
                        // Payload is "Ps SP q" or "SP q Ps SP q" depending
                        // on whether the terminal includes the selector prefix.
                        let style_str = rest.trim().trim_start_matches("q").trim();
                        let label = match style_str {
                            "0" => "default",
                            "1" => "blinking block",
                            "2" => "steady block",
                            "3" => "blinking underline",
                            "4" => "steady underline",
                            "5" => "blinking bar",
                            "6" => "steady bar",
                            _ => style_str.trim(),
                        };
                        return (
                            ProbeStatus::Supported,
                            Some(format!("{label} ({style_str})")),
                        );
                    }
                }
                // Check if we got any valid DECRQSS at all (terminal understands
                // DECRQSS but doesn't support DECSCUSR reporting)
                let saw_valid = events
                    .iter()
                    .any(|e| matches!(e, Event::Decrqss { valid: true, .. }));
                if saw_valid {
                    (ProbeStatus::Unsupported, None)
                } else {
                    (ProbeStatus::Unknown, None)
                }
            }),
        ),
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

    #[test]
    fn italic_supported() {
        let probes = probes();
        let probe = probes.iter().find(|p| p.name == "italic").unwrap();
        let events = vec![Event::Decrqss {
            valid: true,
            payload: "3m".to_string(),
        }];
        let (status, value) = (probe.interpret)(&events);
        assert!(matches!(status, ProbeStatus::Supported));
        assert_eq!(value.unwrap(), "3m");
    }

    #[test]
    fn italic_not_confused_by_styled_underline() {
        let probes = probes();
        let probe = probes.iter().find(|p| p.name == "italic").unwrap();
        // "4:3m" is the styled-underline response - should NOT match italic
        let events = vec![Event::Decrqss {
            valid: true,
            payload: "4:3m".to_string(),
        }];
        let (status, _) = (probe.interpret)(&events);
        assert!(matches!(status, ProbeStatus::Unsupported));
    }

    #[test]
    fn dim_not_confused_by_true_color() {
        let probes = probes();
        let probe = probes.iter().find(|p| p.name == "dim").unwrap();
        // "48;2;150;150;150m" has "2" as a parameter - should NOT match dim
        let events = vec![Event::Decrqss {
            valid: true,
            payload: "48;2;150;150;150m".to_string(),
        }];
        let (status, _) = (probe.interpret)(&events);
        assert!(matches!(status, ProbeStatus::Unsupported));
    }

    #[test]
    fn dim_supported_with_reset_prefix() {
        let probes = probes();
        let probe = probes.iter().find(|p| p.name == "dim").unwrap();
        let events = vec![Event::Decrqss {
            valid: true,
            payload: "0;2m".to_string(),
        }];
        let (status, _) = (probe.interpret)(&events);
        assert!(matches!(status, ProbeStatus::Supported));
    }

    #[test]
    fn double_underline_supported_sgr21() {
        let probes = probes();
        let probe = probes
            .iter()
            .find(|p| p.name == "double-underline")
            .unwrap();
        let events = vec![Event::Decrqss {
            valid: true,
            payload: "21m".to_string(),
        }];
        let (status, _) = (probe.interpret)(&events);
        assert!(matches!(status, ProbeStatus::Supported));
    }

    #[test]
    fn double_underline_supported_4_colon_2() {
        let probes = probes();
        let probe = probes
            .iter()
            .find(|p| p.name == "double-underline")
            .unwrap();
        // Terminals like foot normalize SGR 21 to the 4:2 subparameter syntax
        let events = vec![Event::Decrqss {
            valid: true,
            payload: "0;4:2m".to_string(),
        }];
        let (status, value) = (probe.interpret)(&events);
        assert!(matches!(status, ProbeStatus::Supported));
        assert_eq!(value.unwrap(), "0;4:2m");
    }

    #[test]
    fn sgr_attr_unknown_no_response() {
        let probes = probes();
        let probe = probes.iter().find(|p| p.name == "italic").unwrap();
        let (status, _) = (probe.interpret)(&[]);
        assert!(matches!(status, ProbeStatus::Unknown));
    }

    #[test]
    fn cursor_style_report_supported() {
        let probes = probes();
        let probe = probes
            .iter()
            .find(|p| p.name == "cursor-style-report")
            .unwrap();
        // Standard payload: just "Ps SP q"
        let events = vec![Event::Decrqss {
            valid: true,
            payload: "2 q".to_string(),
        }];
        let (status, value) = (probe.interpret)(&events);
        assert!(matches!(status, ProbeStatus::Supported));
        assert_eq!(value.unwrap(), "steady block (2)");
    }

    #[test]
    fn cursor_style_report_with_selector_prefix() {
        let probes = probes();
        let probe = probes
            .iter()
            .find(|p| p.name == "cursor-style-report")
            .unwrap();
        // Some terminals include the selector prefix: "SP q Ps SP q"
        let events = vec![Event::Decrqss {
            valid: true,
            payload: " q2 q".to_string(),
        }];
        let (status, value) = (probe.interpret)(&events);
        assert!(matches!(status, ProbeStatus::Supported));
        assert_eq!(value.unwrap(), "steady block (2)");
    }

    #[test]
    fn cursor_style_report_blinking_bar() {
        let probes = probes();
        let probe = probes
            .iter()
            .find(|p| p.name == "cursor-style-report")
            .unwrap();
        let events = vec![Event::Decrqss {
            valid: true,
            payload: "5 q".to_string(),
        }];
        let (status, value) = (probe.interpret)(&events);
        assert!(matches!(status, ProbeStatus::Supported));
        assert_eq!(value.unwrap(), "blinking bar (5)");
    }

    #[test]
    fn cursor_style_report_unknown_no_response() {
        let probes = probes();
        let probe = probes
            .iter()
            .find(|p| p.name == "cursor-style-report")
            .unwrap();
        let (status, _) = (probe.interpret)(&[]);
        assert!(matches!(status, ProbeStatus::Unknown));
    }

    #[test]
    fn multi_probe_both_forms_match() {
        let probes = probes();
        let probe = probes.iter().find(|p| p.name == "true-color").unwrap();
        let events = vec![
            Event::Decrqss {
                valid: true,
                payload: "48;2;150;150;150m".to_string(),
            },
            Event::Decrqss {
                valid: true,
                payload: "48:2::150:150:150m".to_string(),
            },
        ];
        let (status, value) = (probe.interpret)(&events);
        assert!(matches!(status, ProbeStatus::Supported));
        let v = value.unwrap();
        assert!(v.contains("48;2;150;150;150m"), "got: {v}");
        assert!(v.contains("48:2::150:150:150m"), "got: {v}");
    }

    #[test]
    fn multi_probe_one_form_matches() {
        let probes = probes();
        let probe = probes.iter().find(|p| p.name == "true-color").unwrap();
        // Only the 5-subparam colon form matches (like koshi)
        let events = vec![
            Event::Decrqss {
                valid: true,
                payload: "0m".to_string(),
            },
            Event::Decrqss {
                valid: true,
                payload: "0m".to_string(),
            },
            Event::Decrqss {
                valid: true,
                payload: "48:2:150:150:150m".to_string(),
            },
        ];
        let (status, value) = (probe.interpret)(&events);
        assert!(matches!(status, ProbeStatus::Supported));
        assert_eq!(value.unwrap(), "48:2:150:150:150m");
    }

    #[test]
    fn multi_probe_none_match() {
        let probes = probes();
        let probe = probes.iter().find(|p| p.name == "true-color").unwrap();
        let events = vec![
            Event::Decrqss {
                valid: true,
                payload: "0m".to_string(),
            },
            Event::Decrqss {
                valid: true,
                payload: "0m".to_string(),
            },
            Event::Decrqss {
                valid: true,
                payload: "0m".to_string(),
            },
        ];
        let (status, _) = (probe.interpret)(&events);
        assert!(matches!(status, ProbeStatus::Unsupported));
    }

    #[test]
    fn multi_probe_no_events() {
        let probes = probes();
        let probe = probes.iter().find(|p| p.name == "true-color").unwrap();
        let (status, _) = (probe.interpret)(&[]);
        assert!(matches!(status, ProbeStatus::Unknown));
    }

    #[test]
    fn underline_color_5_subparam_supported() {
        let probes = probes();
        let probe = probes.iter().find(|p| p.name == "underline-color").unwrap();
        // Terminal only understands 5-subparam form
        let events = vec![
            Event::Decrqss {
                valid: true,
                payload: "0m".to_string(),
            },
            Event::Decrqss {
                valid: true,
                payload: "58:2:170:170:170m".to_string(),
            },
        ];
        let (status, value) = (probe.interpret)(&events);
        assert!(matches!(status, ProbeStatus::Supported));
        assert_eq!(value.unwrap(), "58:2:170:170:170m");
    }
}
