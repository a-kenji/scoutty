use std::io;
use std::process::ExitCode;
use std::time::Duration;

use clap::{CommandFactory, Parser};
use clap_complete::Shell;

mod error;
mod hex;
mod output;
mod pager;
mod parser;
mod probe;
mod probes;
mod tty;

use error::Error;
use output::ColorMode;
use pager::PagerMode;

#[derive(Parser)]
#[command(
    name = "scoutty",
    version,
    about,
    long_about = "Probe your terminal's capabilities by sending escape sequences and \
        observing responses. Unlike static databases (terminfo/termcap), scoutty tests \
        what actually works in the current session, through multiplexers and SSH.",
    after_help = "EXAMPLES:\n  \
        scoutty                          Run all probes\n  \
        scoutty --category identity      Only identity probes\n  \
        scoutty --probe da1 --probe da2  Run specific probes\n  \
        scoutty --json                   Machine-readable output\n  \
        scoutty --json | jq              Filter with jq\n  \
        scoutty --list-probes            Show available probes"
)]
struct Cli {
    /// Output results as JSON
    #[arg(long)]
    json: bool,

    /// Filter by category
    #[arg(
        long,
        long_help = "Filter by category (comma-separated). Available categories:\n  \
        identity, modes, keyboard, graphics, colors, styling, features, geometry"
    )]
    category: Option<String>,

    /// Run specific probes by name (repeatable)
    #[arg(long, action = clap::ArgAction::Append,
        long_help = "Run specific probes by name. Can be repeated.\n\n  \
        scoutty --probe da1 --probe da2\n\n\
        Use --list-probes to see available probe names.")]
    probe: Vec<String>,

    /// Show raw query/response bytes in hex
    #[arg(
        long,
        long_help = "Show raw query/response bytes in hex. Displays the exact \
        escape sequences sent to the terminal and the raw bytes received back."
    )]
    raw: bool,

    /// Timeout in milliseconds for DA1 sentinel
    #[arg(
        long,
        default_value = "1000",
        long_help = "Timeout in milliseconds for the \
        DA1 sentinel response. scoutty sends DA1 as the last query — since every \
        terminal must respond to DA1, its arrival signals that all prior responses \
        have been received. This timeout is a fallback for terminals that don't \
        respond at all."
    )]
    timeout: u64,

    /// List available probes and exit
    #[arg(long)]
    list_probes: bool,

    /// Control pager behavior (auto, always, never)
    #[arg(
        long,
        default_value = "never",
        long_help = "Control pager behavior \
        (auto, always, never).\n\n\
        auto:   page through $PAGER when output exceeds terminal height\n\
        always: always use $PAGER (falls back to less -R, then more)\n\
        never:  print directly to stdout"
    )]
    pager: PagerMode,

    /// Control color output (auto, always, never)
    #[arg(long, default_value = "auto")]
    color: ColorMode,

    /// Generate shell completions
    #[arg(
        long,
        long_help = "Generate shell completions and print to stdout.\n\n  \
        scoutty --completions bash >> ~/.bashrc\n  \
        scoutty --completions fish > ~/.config/fish/completions/scoutty.fish\n  \
        scoutty --completions zsh > ~/.zfunc/_scoutty"
    )]
    completions: Option<Shell>,
}

fn run(probes: &[probe::Probe], cli: &Cli) -> Result<(), Error> {
    let output = {
        let tty = tty::Tty::open().map_err(Error::NoTty)?;
        let timeout = Duration::from_millis(cli.timeout);
        probe::run_probes(&tty, probes, timeout, cli.raw)?
    };

    let formatted = if cli.json {
        output::format_json(&output, cli.raw)?
    } else {
        output::format_human(&output, cli.raw)
    };

    let pager = pager::Pager::new(cli.pager);
    pager.show(&formatted).map_err(Error::Io)?;
    Ok(())
}

fn main() -> ExitCode {
    // Rust ignores SIGPIPE by default, causing println! to panic on broken
    // pipes (e.g. `scoutty | head`). Restore the default handler so we
    // silently exit instead.
    unsafe {
        nix::libc::signal(nix::libc::SIGPIPE, nix::libc::SIG_DFL);
    }

    let cli = Cli::parse();
    output::init_color(cli.color, cli.pager.may_page());

    if let Some(shell) = cli.completions {
        let mut cmd = Cli::command();
        clap_complete::generate(shell, &mut cmd, "scoutty", &mut io::stdout());
        return ExitCode::SUCCESS;
    }

    let all = probe::all_probes();

    if cli.list_probes {
        output::print_probe_list(&all);
        return ExitCode::SUCCESS;
    }

    let selected: Vec<_> = if !cli.probe.is_empty() {
        for name in &cli.probe {
            if !all.iter().any(|p| p.name == name.as_str()) {
                eprintln!("error: unknown probe '{name}'");
                return ExitCode::FAILURE;
            }
        }
        all.into_iter()
            .filter(|p| cli.probe.iter().any(|name| p.name == name.as_str()))
            .collect()
    } else if let Some(ref cats) = cli.category {
        let categories: Result<Vec<probe::Category>, String> =
            cats.split(',').map(|s| s.trim().parse()).collect();
        let categories = match categories {
            Ok(c) => c,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        };
        all.into_iter()
            .filter(|p| categories.contains(&p.category))
            .collect()
    } else {
        all
    };

    if selected.is_empty() {
        eprintln!("error: no probes selected");
        return ExitCode::FAILURE;
    }

    if let Err(e) = run(&selected, &cli) {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
