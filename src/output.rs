use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::sync::OnceLock;

use serde::Serialize;

use crate::probe::{Category, Probe, ProbeOutput, ProbeStatus};

#[derive(Clone, Copy)]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

impl std::str::FromStr for ColorMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "auto" => Ok(ColorMode::Auto),
            "always" => Ok(ColorMode::Always),
            "never" => Ok(ColorMode::Never),
            _ => Err(format!("unknown color mode: {s}")),
        }
    }
}

pub fn init_color(mode: ColorMode, pager_enabled: bool) {
    USE_COLOR.get_or_init(|| match mode {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => {
            std::env::var_os("NO_COLOR").is_none()
                && (pager_enabled || std::io::stdout().is_terminal())
        }
    });
}

static USE_COLOR: OnceLock<bool> = OnceLock::new();

fn use_color() -> bool {
    *USE_COLOR.get_or_init(|| {
        std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
    })
}

fn colored_status(status: &ProbeStatus) -> String {
    let (text, code) = match status {
        ProbeStatus::Supported => ("supported", "\x1b[32m"),
        ProbeStatus::Unsupported => ("unsupported", "\x1b[31m"),
        ProbeStatus::Unknown => ("unknown", "\x1b[2m"),
    };
    if use_color() {
        format!("{code}{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub fn format_human(output: &ProbeOutput, raw: bool) -> String {
    use std::fmt::Write;
    let mut buf = String::new();
    let results = &output.results;
    for category in Category::ALL {
        let cat_results: Vec<_> = results.iter().filter(|r| r.category == *category).collect();
        if cat_results.is_empty() {
            continue;
        }

        writeln!(buf, "\n{}", category.to_string().to_uppercase()).unwrap();
        for result in &cat_results {
            let display_name = result.label.as_deref().unwrap_or(&result.name);
            match (&result.status, &result.value) {
                (ProbeStatus::Supported, Some(value)) => {
                    writeln!(buf, "  {:<28} {}", display_name, value).unwrap();
                }
                (_, Some(value)) => {
                    writeln!(
                        buf,
                        "  {:<28} {} {}",
                        display_name,
                        colored_status(&result.status),
                        value
                    )
                    .unwrap();
                }
                (_, None) => {
                    writeln!(buf, "  {:<28} {}", display_name, colored_status(&result.status))
                        .unwrap();
                }
            }
            if raw && let Some(ref q) = result.raw_query {
                writeln!(buf, "    query:    {q}").unwrap();
            }
        }
    }

    if let Some(ref r) = output.raw_response {
        writeln!(buf, "\nRaw response: {r}").unwrap();
    }

    let supported = results
        .iter()
        .filter(|r| matches!(r.status, ProbeStatus::Supported))
        .count();
    let unsupported = results
        .iter()
        .filter(|r| matches!(r.status, ProbeStatus::Unsupported))
        .count();
    let unknown = results
        .iter()
        .filter(|r| matches!(r.status, ProbeStatus::Unknown))
        .count();
    let total = results.len();

    let terminal = results
        .iter()
        .find(|r| r.name == "xtversion" && matches!(r.status, ProbeStatus::Supported))
        .or_else(|| {
            results
                .iter()
                .find(|r| r.name == "da2" && matches!(r.status, ProbeStatus::Supported))
        })
        .and_then(|r| r.value.as_deref());

    writeln!(buf).unwrap();
    if let Some(term) = terminal {
        writeln!(buf, "{term}").unwrap();
    }
    writeln!(buf, "{supported}/{total} supported, {unsupported} unsupported, {unknown} unknown")
        .unwrap();
    buf
}

#[derive(Serialize)]
struct JsonProbeResult {
    name: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    raw_query: Option<String>,
}

#[derive(Serialize)]
struct JsonOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    raw_response: Option<String>,
    probes: BTreeMap<String, Vec<JsonProbeResult>>,
}

pub fn format_json(output: &ProbeOutput, raw: bool) -> serde_json::Result<String> {
    let mut grouped: BTreeMap<String, Vec<JsonProbeResult>> = BTreeMap::new();
    for result in &output.results {
        let json_result = JsonProbeResult {
            name: result.name.clone(),
            status: result.status.to_string(),
            value: result.value.clone(),
            raw_query: if raw { result.raw_query.clone() } else { None },
        };
        grouped
            .entry(result.category.to_string())
            .or_default()
            .push(json_result);
    }
    let json_output = JsonOutput {
        raw_response: output.raw_response.clone(),
        probes: grouped,
    };
    let mut json = serde_json::to_string_pretty(&json_output)?;
    json.push('\n');
    Ok(json)
}

pub fn print_probe_list(probes: &[Probe]) {
    for category in Category::ALL {
        let cat_probes: Vec<_> = probes.iter().filter(|p| p.category == *category).collect();
        if cat_probes.is_empty() {
            continue;
        }

        println!("\n{}", category.to_string().to_uppercase());
        for probe in &cat_probes {
            println!("  {}", probe.name);
        }
    }
    println!();
}
