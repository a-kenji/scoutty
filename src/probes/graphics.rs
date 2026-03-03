use crate::parser::Event;
use crate::probe::{Category, Probe, ProbeStatus};

pub fn probes() -> Vec<Probe> {
    vec![
        Probe {
            label: None,
            name: "kitty-graphics",
            category: Category::Graphics,
            query: b"\x1b_Gi=1,a=q\x1b\\".to_vec(),
            is_sentinel: false,
            interpret: Box::new(|events| {
                for event in events {
                    if let Event::KittyGraphics { payload } = event {
                        return (ProbeStatus::Supported, Some(payload.clone()));
                    }
                }
                (ProbeStatus::Unknown, None)
            }),
        },
        Probe {
            label: None,
            name: "sixel",
            category: Category::Graphics,
            is_sentinel: false,
            // XTSMGRAPHICS: color registers + max geometry
            query: b"\x1b[?2;1;0S\x1b[?1;1;0S".to_vec(),
            interpret: Box::new(|events| {
                let mut da1_sixel = false;
                let mut color_regs = None;
                let mut max_geo = None;

                for event in events {
                    if let Event::Da1 { params } = event
                        && params.contains(&4)
                    {
                        da1_sixel = true;
                    }
                    if let Event::XtSmGraphics {
                        item,
                        status,
                        values,
                    } = event
                        && *status == 0
                    {
                        match item {
                            2 => {
                                if let Some(&count) = values.first() {
                                    color_regs = Some(count);
                                }
                            }
                            1 => {
                                if values.len() >= 2 {
                                    max_geo = Some((values[0], values[1]));
                                }
                            }
                            _ => {}
                        }
                    }
                }

                if da1_sixel || color_regs.is_some() || max_geo.is_some() {
                    let mut parts = Vec::new();
                    if da1_sixel {
                        parts.push("da1-ext-4".to_string());
                    }
                    if let Some(regs) = color_regs {
                        parts.push(format!("colors={regs}"));
                    }
                    if let Some((w, h)) = max_geo {
                        parts.push(format!("max={w}x{h}"));
                    }
                    (ProbeStatus::Supported, Some(parts.join(", ")))
                } else {
                    (ProbeStatus::Unknown, None)
                }
            }),
        },
    ]
}
