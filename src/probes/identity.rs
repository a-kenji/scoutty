use crate::parser::Event;
use crate::probe::{Category, Probe, ProbeStatus};

fn decode_da1_conformance(param: u16) -> Option<&'static str> {
    match param {
        1 => Some("VT100"),
        6 => Some("VT102"),
        62 => Some("VT220"),
        63 => Some("VT320"),
        64 => Some("VT420"),
        65 => Some("VT525"),
        _ => None,
    }
}

fn decode_da1_feature(param: u16) -> Option<&'static str> {
    match param {
        1 => Some("132-columns"),
        2 => Some("printer"),
        3 => Some("ReGIS-graphics"),
        4 => Some("sixel"),
        6 => Some("selective-erase"),
        7 => Some("DRCS"),
        8 => Some("UDK"),
        9 => Some("national-replacement"),
        12 => Some("SCS"),
        15 => Some("technical-characters"),
        18 => Some("windowing"),
        21 => Some("horizontal-scrolling"),
        22 => Some("ANSI-color"),
        28 => Some("rectangular-editing"),
        29 => Some("text-locator"),
        _ => None,
    }
}

fn decode_da2_terminal(param: u16) -> Option<&'static str> {
    match param {
        0 => Some("VT100"),
        1 => Some("VT220"),
        2 => Some("VT240"),
        18 => Some("VT330"),
        19 => Some("VT340"),
        24 => Some("VT320"),
        32 => Some("VT382"),
        41 => Some("VT420"),
        61 => Some("VT510"),
        64 => Some("VT520"),
        65 => Some("VT525"),
        _ => None,
    }
}

fn xtgettcap_probe(name: &'static str, cap_name: &'static str, cap_hex: &'static str) -> Probe {
    Probe::new(
        name,
        Category::Identity,
        format!("\x1bP+q{cap_hex}\x1b\\").into_bytes(),
        Box::new(move |events| {
            for event in events {
                if let Event::XtGetTcap { name: n, value: v } = event
                    && n == cap_name
                {
                    return match v {
                        Some(val) => (ProbeStatus::Supported, Some(val.clone())),
                        None => (ProbeStatus::Unsupported, None),
                    };
                }
            }
            (ProbeStatus::Unknown, None)
        }),
    )
}

fn decode_hex_id(hex: &str) -> String {
    let Some(bytes) = crate::hex::decode(hex) else {
        return String::new();
    };
    if !bytes.is_empty() && bytes.iter().all(|b| b.is_ascii_graphic()) {
        String::from_utf8(bytes).unwrap_or_default()
    } else {
        String::new()
    }
}

fn decode_da2_params(params: &[u16]) -> String {
    let raw = params
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(";");

    if params.is_empty() {
        return raw;
    }

    let terminal = decode_da2_terminal(params[0])
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("type {}", params[0]));
    let version = params.get(1).copied().unwrap_or(0);

    format!("{terminal}, version {version} ({raw})")
}

fn decode_da1_params(params: &[u16]) -> String {
    let raw = params
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(";");

    let mut decoded = Vec::new();
    for (i, &p) in params.iter().enumerate() {
        if i == 0 {
            decoded.push(
                decode_da1_conformance(p)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| p.to_string()),
            );
        } else {
            decoded.push(
                decode_da1_feature(p)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| p.to_string()),
            );
        }
    }

    format!("{} ({})", decoded.join(", "), raw)
}

pub fn probes() -> Vec<Probe> {
    vec![
        Probe {
            label: None,
            name: "da1",
            category: Category::Identity,
            is_sentinel: true,
            query: b"\x1b[c".to_vec(),
            interpret: Box::new(|events| {
                for event in events {
                    if let Event::Da1 { params } = event {
                        return (ProbeStatus::Supported, Some(decode_da1_params(params)));
                    }
                }
                (ProbeStatus::Unknown, None)
            }),
        },
        Probe::new(
            "da2",
            Category::Identity,
            b"\x1b[>c".to_vec(),
            Box::new(|events| {
                for event in events {
                    if let Event::Da2 { params } = event {
                        return (ProbeStatus::Supported, Some(decode_da2_params(params)));
                    }
                }
                (ProbeStatus::Unknown, None)
            }),
        ),
        Probe::new(
            "da3",
            Category::Identity,
            b"\x1b[=c".to_vec(),
            Box::new(|events| {
                for event in events {
                    if let Event::Da3 { id } = event {
                        let decoded = decode_hex_id(id);
                        let display = if decoded.is_empty() {
                            id.clone()
                        } else {
                            format!("{decoded} ({id})")
                        };
                        return (ProbeStatus::Supported, Some(display));
                    }
                }
                (ProbeStatus::Unknown, None)
            }),
        ),
        xtgettcap_probe("xtgettcap-tn", "TN", "544e"),
        xtgettcap_probe("xtgettcap-rgb", "RGB", "524742"),
        xtgettcap_probe("xtgettcap-smulx", "Smulx", "536d756c78"),
        xtgettcap_probe("xtgettcap-hpa", "hpa", "687061"),
        xtgettcap_probe("xtgettcap-se", "Se", "5365"),
        xtgettcap_probe("xtgettcap-ss", "Ss", "5373"),
        xtgettcap_probe("xtgettcap-tc", "Tc", "5463"),
        xtgettcap_probe("xtgettcap-ms", "Ms", "4d73"),
        xtgettcap_probe("xtgettcap-sitm", "sitm", "7369746d"),
        xtgettcap_probe("xtgettcap-ritm", "ritm", "7269746d"),
        Probe::new(
            "xtversion",
            Category::Identity,
            b"\x1b[>0q".to_vec(),
            Box::new(|events| {
                for event in events {
                    if let Event::XtVersion { version } = event {
                        return (ProbeStatus::Supported, Some(version.clone()));
                    }
                }
                (ProbeStatus::Unknown, None)
            }),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_da1_vt220_sixel() {
        assert_eq!(
            decode_da1_params(&[62, 4, 6]),
            "VT220, sixel, selective-erase (62;4;6)"
        );
    }

    #[test]
    fn decode_da1_unknown_param() {
        assert_eq!(
            decode_da1_params(&[64, 4, 99]),
            "VT420, sixel, 99 (64;4;99)"
        );
    }

    #[test]
    fn decode_da1_single_conformance() {
        assert_eq!(decode_da1_params(&[1]), "VT100 (1)");
    }

    #[test]
    fn decode_da2_known_terminal() {
        assert_eq!(
            decode_da2_params(&[41, 382, 0]),
            "VT420, version 382 (41;382;0)"
        );
    }

    #[test]
    fn decode_da2_unknown_terminal() {
        assert_eq!(
            decode_da2_params(&[84, 0, 0]),
            "type 84, version 0 (84;0;0)"
        );
    }
}
