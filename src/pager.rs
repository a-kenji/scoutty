use std::io::{self, IsTerminal, Write};
use std::process::{Command, Stdio};
use std::{env, mem};

use nix::libc;

#[derive(Clone, Copy)]
pub enum PagerMode {
    Auto,
    Always,
    Never,
}

impl std::str::FromStr for PagerMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "auto" => Ok(PagerMode::Auto),
            "always" => Ok(PagerMode::Always),
            "never" => Ok(PagerMode::Never),
            _ => Err(format!("unknown pager mode: {s}")),
        }
    }
}

impl PagerMode {
    pub fn may_page(self) -> bool {
        !matches!(self, PagerMode::Never)
    }
}

pub struct Pager {
    mode: PagerMode,
}

impl Pager {
    pub fn new(mode: PagerMode) -> Self {
        Self { mode }
    }

    pub fn show(&self, content: &str) -> io::Result<()> {
        let should_page = match self.mode {
            PagerMode::Never => false,
            PagerMode::Always => io::stdout().is_terminal(),
            PagerMode::Auto => io::stdout().is_terminal() && content_exceeds_terminal(content),
        };
        if !should_page {
            print!("{content}");
            return Ok(());
        }

        let pager_cmd = env::var("PAGER").ok().filter(|s| !s.is_empty());
        let candidates: Vec<&str> = match pager_cmd.as_deref() {
            Some(cmd) => vec![cmd],
            None => vec!["less -R", "more"],
        };

        for candidate in candidates {
            let mut parts = candidate.split_whitespace();
            let Some(program) = parts.next() else {
                continue;
            };
            let args: Vec<&str> = parts.collect();
            if let Ok(mut child) = Command::new(program)
                .args(&args)
                .stdin(Stdio::piped())
                .spawn()
            {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(content.as_bytes());
                }
                let _ = child.wait();
                return Ok(());
            }
        }

        print!("{content}");
        Ok(())
    }
}

fn content_exceeds_terminal(content: &str) -> bool {
    let mut ws: libc::winsize = unsafe { mem::zeroed() };
    let ret = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) };
    if ret != 0 || ws.ws_row == 0 {
        return false;
    }
    let terminal_rows = ws.ws_row as usize;
    content.lines().count() > terminal_rows.saturating_sub(3)
}
