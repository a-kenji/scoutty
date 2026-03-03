use crate::parser::Event;
use crate::probe::{Category, Probe, ProbeStatus};

pub fn probes() -> Vec<Probe> {
    vec![Probe {
        label: None,
        name: "kitty-keyboard",
        category: Category::Keyboard,
        query: b"\x1b[?u".to_vec(),
        is_sentinel: false,
        interpret: Box::new(|events| {
            for event in events {
                if let Event::KittyKeyboard { flags } = event {
                    return (ProbeStatus::Supported, Some(format!("flags={flags}")));
                }
            }
            (ProbeStatus::Unknown, None)
        }),
    }]
}
