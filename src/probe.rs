use std::time::{Duration, Instant};

use serde::Serialize;

use crate::error::Error;
use crate::hex;
use crate::parser::{Event, ResponseParser};
use crate::tty::Tty;

pub type InterpretFn = Box<dyn Fn(&[Event]) -> (ProbeStatus, Option<String>)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    Identity,
    Modes,
    Keyboard,
    Graphics,
    Colors,
    Styling,
    Features,
    Geometry,
}

impl Category {
    pub const ALL: &[Category] = &[
        Category::Identity,
        Category::Modes,
        Category::Keyboard,
        Category::Graphics,
        Category::Colors,
        Category::Styling,
        Category::Features,
        Category::Geometry,
    ];
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Category::Identity => write!(f, "identity"),
            Category::Modes => write!(f, "modes"),
            Category::Keyboard => write!(f, "keyboard"),
            Category::Graphics => write!(f, "graphics"),
            Category::Colors => write!(f, "colors"),
            Category::Styling => write!(f, "styling"),
            Category::Features => write!(f, "features"),
            Category::Geometry => write!(f, "geometry"),
        }
    }
}

impl std::str::FromStr for Category {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "identity" => Ok(Category::Identity),
            "modes" => Ok(Category::Modes),
            "keyboard" => Ok(Category::Keyboard),
            "graphics" => Ok(Category::Graphics),
            "colors" => Ok(Category::Colors),
            "styling" => Ok(Category::Styling),
            "features" => Ok(Category::Features),
            "geometry" => Ok(Category::Geometry),
            _ => Err(format!("unknown category: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProbeStatus {
    Supported,
    Unsupported,
    Unknown,
}

impl std::fmt::Display for ProbeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProbeStatus::Supported => write!(f, "supported"),
            ProbeStatus::Unsupported => write!(f, "unsupported"),
            ProbeStatus::Unknown => write!(f, "unknown"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeResult {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub category: Category,
    pub status: ProbeStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_query: Option<String>,
}

pub struct ProbeOutput {
    pub results: Vec<ProbeResult>,
    pub raw_response: Option<String>,
}

pub struct Probe {
    pub name: &'static str,
    pub label: Option<String>,
    pub category: Category,
    pub query: Vec<u8>,
    pub interpret: InterpretFn,
    pub is_sentinel: bool,
}

impl Probe {
    pub fn new(
        name: &'static str,
        category: Category,
        query: Vec<u8>,
        interpret: InterpretFn,
    ) -> Self {
        Probe {
            name,
            label: None,
            category,
            query,
            interpret,
            is_sentinel: false,
        }
    }
}

pub fn all_probes() -> Vec<Probe> {
    let mut probes = Vec::new();
    probes.extend(crate::probes::identity::probes());
    probes.extend(crate::probes::modes::probes());
    probes.extend(crate::probes::keyboard::probes());
    probes.extend(crate::probes::graphics::probes());
    probes.extend(crate::probes::colors::probes());
    probes.extend(crate::probes::geometry::probes());
    probes
}

pub fn run_probes(
    tty: &Tty,
    probes: &[Probe],
    timeout: Duration,
    raw: bool,
) -> Result<ProbeOutput, Error> {
    // Build query buffer: non-sentinel probes first, then sentinel last
    let mut query_buf = Vec::new();
    for probe in probes {
        if !probe.is_sentinel {
            query_buf.extend_from_slice(&probe.query);
        }
    }
    // DA1 sentinel always goes last
    query_buf.extend_from_slice(b"\x1b[c");

    let _guard = tty.raw_mode().map_err(Error::Io)?;

    tty.write_all(&query_buf).map_err(Error::Io)?;

    let deadline = Instant::now() + timeout;
    let mut parser = ResponseParser::new();
    let mut read_buf = [0u8; 4096];

    loop {
        let n = tty.poll_read(&mut read_buf, deadline).map_err(Error::Io)?;
        if n == 0 {
            break;
        }
        parser.feed(&read_buf[..n]);
        if parser.has_da1() {
            break;
        }
    }

    // Drain slow responses (e.g. OSC 52 clipboard) to prevent leaking into the
    // shell. Instead of a fixed wait, use an idle timeout: keep reading as long
    // as data keeps arriving, stop after DRAIN_IDLE of silence. DRAIN_MAX caps
    // total drain time to prevent hanging on a noisy fd.
    const DRAIN_IDLE: Duration = Duration::from_millis(10);
    const DRAIN_MAX: Duration = Duration::from_millis(200);
    let drain_limit = Instant::now() + DRAIN_MAX;
    loop {
        let deadline = (Instant::now() + DRAIN_IDLE).min(drain_limit);
        let n = tty.poll_read(&mut read_buf, deadline).map_err(Error::Io)?;
        if n == 0 {
            break;
        }
        parser.feed(&read_buf[..n]);
    }

    if raw {
        let overflows = parser.apc_overflows();
        if overflows > 0 {
            eprintln!("warning: {overflows} APC payload(s) exceeded 64KB limit and were discarded");
        }
    }

    let events = parser.events();
    let raw_response = if raw {
        Some(hex::encode(parser.raw_bytes()))
    } else {
        None
    };

    let mut results = Vec::new();
    for probe in probes {
        let (status, value) = (probe.interpret)(events);
        results.push(ProbeResult {
            name: probe.name.to_string(),
            label: probe.label.clone(),
            category: probe.category,
            status,
            value,
            raw_query: if raw {
                Some(hex::encode(&probe.query))
            } else {
                None
            },
        });
    }

    Ok(ProbeOutput {
        results,
        raw_response,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_all_is_complete() {
        // Display/FromStr matches are exhaustive (compiler-enforced), so adding
        // a variant forces updating those. This count assertion catches ALL
        // falling out of sync.
        assert_eq!(Category::ALL.len(), 8);
        for &cat in Category::ALL {
            let s = cat.to_string();
            assert_eq!(s.parse::<Category>().unwrap(), cat);
        }
    }
}
