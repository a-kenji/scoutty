use crate::parser::{Event, ModeSetting};
use crate::probe::{Category, Probe, ProbeStatus};

fn decrqm_probe(name: &'static str, mode: u16) -> Probe {
    Probe {
        name,
        label: Some(format!("{name} ({mode})")),
        category: Category::Modes,
        query: format!("\x1b[?{mode}$p").into_bytes(),
        is_sentinel: false,
        interpret: Box::new(move |events| {
            for event in events {
                if let Event::ModeReport { mode: m, setting } = event
                    && *m == mode
                {
                    return match setting {
                        ModeSetting::Set => (ProbeStatus::Supported, Some("enabled".to_string())),
                        ModeSetting::Reset => {
                            (ProbeStatus::Supported, Some("disabled".to_string()))
                        }
                        ModeSetting::PermanentlySet => (
                            ProbeStatus::Supported,
                            Some("enabled (permanent)".to_string()),
                        ),
                        ModeSetting::PermanentlyReset => (
                            ProbeStatus::Supported,
                            Some("disabled (permanent)".to_string()),
                        ),
                        ModeSetting::NotRecognized => (ProbeStatus::Unsupported, None),
                    };
                }
            }
            (ProbeStatus::Unknown, None)
        }),
    }
}

pub fn probes() -> Vec<Probe> {
    vec![
        decrqm_probe("auto-wrap", 7),
        decrqm_probe("cursor-visible", 25),
        decrqm_probe("mouse-x10", 9),
        decrqm_probe("bracketed-paste", 2004),
        decrqm_probe("focus-events", 1004),
        decrqm_probe("mouse-normal", 1000),
        decrqm_probe("mouse-button", 1002),
        decrqm_probe("mouse-any", 1003),
        decrqm_probe("mouse-sgr", 1006),
        decrqm_probe("mouse-sgr-pixels", 1016),
        decrqm_probe("alt-screen", 1049),
        decrqm_probe("synchronized-output", 2026),
        decrqm_probe("unicode-core", 2027),
        decrqm_probe("color-scheme-updates", 2031),
        decrqm_probe("in-band-resize", 2048),
    ]
}
