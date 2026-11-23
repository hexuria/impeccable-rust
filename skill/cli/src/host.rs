//! What this machine can run without the toolbox.

use std::path::Path;

use crate::pins::{self, Pin, Version};
use crate::proc;

/// The operating system this binary was built for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Linux,
    Mac,
    Windows,
    Other,
}

impl Os {
    pub fn current() -> Self {
        if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Other
        }
    }
}

/// Why `tool` cannot run on a host with this operating system, when it cannot.
///
/// Only limits the upstream projects document are listed. Sanitizers are probed through
/// rustc instead, because their support differs per target triple.
pub fn unsupported_reason(tool: &str, os: Os) -> Option<&'static str> {
    match (tool, os) {
        ("gungraun-runner" | "valgrind", Os::Mac) => Some("no Valgrind on macOS"),
        ("gungraun-runner" | "valgrind", Os::Windows) => Some("no Valgrind on Windows"),
        ("kani-verifier", Os::Windows) => Some("Kani has no Windows build"),
        _ => None,
    }
}

/// Which rustc to ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toolchain {
    Default,
    Nightly,
}

/// A rustup proxy command that never downloads a missing toolchain as a side effect.
fn rustup_tool(program: &str, toolchain: Toolchain) -> std::process::Command {
    let mut command = proc::command(program);
    command.env("RUSTUP_AUTO_INSTALL", "0");
    if toolchain == Toolchain::Nightly {
        command.arg("+nightly");
    }
    command
}

/// The host triple rustc reports, such as `aarch64-apple-darwin`.
pub fn rustc_host(toolchain: Toolchain) -> Option<String> {
    if toolchain == Toolchain::Nightly && !nightly_installed() {
        return None;
    }
    let verbose = proc::stdout_if_ok(rustup_tool("rustc", toolchain).arg("-vV"))?;
    verbose
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(|triple| triple.trim().to_owned())
}

pub fn nightly_installed() -> bool {
    proc::have("rustup")
        && proc::stdout_if_ok(proc::command("rustup").args(["toolchain", "list"]))
            .is_some_and(|list| list.lines().any(|line| line.starts_with("nightly-")))
}

/// The build date of the installed nightly, such as `2026-09-25`.
pub fn nightly_date() -> Option<String> {
    let version = proc::stdout_if_ok(rustup_tool("rustc", Toolchain::Nightly).arg("-V"))?;
    version
        .split(|c: char| !(c.is_ascii_digit() || c == '-'))
        .find(|word| is_date(word))
        .map(str::to_owned)
}

fn is_date(word: &str) -> bool {
    let bytes = word.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(at, byte)| match at {
            4 | 7 => *byte == b'-',
            _ => byte.is_ascii_digit(),
        })
}

/// True when the nightly toolchain has the rustup component `name`.
pub fn nightly_has(name: &str) -> bool {
    proc::stdout_if_ok(proc::command("rustup").args([
        "+nightly",
        "component",
        "list",
        "--installed",
    ]))
    .is_some_and(|list| list.lines().any(|line| line.starts_with(name)))
}

/// True when nightly rustc accepts `-Zsanitizer=<kind>` for `triple`.
pub fn sanitizer_supported(kind: &str, triple: &str) -> bool {
    proc::succeeds(
        rustup_tool("rustc", Toolchain::Nightly)
            .args(["--print", "cfg"])
            .arg(format!("-Zsanitizer={kind}"))
            .args(["--target", triple]),
    )
}

/// A tool whose location `impeccable run` decides: host or toolbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Pinned(Pin),
    Miri,
    Valgrind,
    Tlc,
}

/// What the host can do with one tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostState {
    /// Installed and usable; the text is its version, or `installed` when it prints none.
    Ready(String),
    /// Installed, but older than the toolbox's pin.
    Older { found: Version, pinned: Version },
    /// Not installed; the text is the command that installs it.
    Missing(String),
    /// Cannot run on this operating system; the text says why.
    Unsupported(&'static str),
}

impl Tool {
    /// The tool a command line invokes, when it is one this program tracks.
    pub fn of_command(argv: &[String]) -> Option<Self> {
        let program = program_name(argv.first()?);
        let crate_name = if program == "cargo" {
            let subcommand = argv[1..].iter().find(|arg| !arg.starts_with('+'))?;
            match subcommand.as_str() {
                "kani" => "kani-verifier".to_owned(),
                "miri" => return Some(Self::Miri),
                other => format!("cargo-{other}"),
            }
        } else {
            match program {
                "cargo-kani" | "kani" => "kani-verifier".to_owned(),
                "valgrind" => return Some(Self::Valgrind),
                "tlc" => return Some(Self::Tlc),
                other => other.to_owned(),
            }
        };
        pins::find(&crate_name).map(Self::Pinned)
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Pinned(pin) => pin.name,
            Self::Miri => "miri",
            Self::Valgrind => "valgrind",
            Self::Tlc => "tlc",
        }
    }

    /// Probes the host for this tool.
    pub fn state(&self) -> HostState {
        let os = Os::current();
        if let Some(reason) = unsupported_reason(self.label(), os) {
            return HostState::Unsupported(reason);
        }
        match self {
            Self::Pinned(pin) => pinned_state(*pin),
            Self::Miri => miri_state(),
            Self::Valgrind => match installed_version("valgrind", &["--version"]) {
                Some(found) => HostState::Ready(found.text()),
                None => HostState::Missing("your system package manager (valgrind)".to_owned()),
            },
            Self::Tlc => {
                if proc::have("tlc") {
                    HostState::Ready("installed".to_owned())
                } else {
                    HostState::Missing("a `tlc` launcher for tla2tools.jar on PATH".to_owned())
                }
            }
        }
    }
}

/// The program a command line starts: its file name without a directory or `.exe`.
fn program_name(arg: &str) -> &str {
    let name = Path::new(arg)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(arg);
    name.strip_suffix(".exe").unwrap_or(name)
}

/// A tool found on the host.
enum Found {
    Version(Version),
    /// On `PATH`, but it prints no version to compare.
    Unversioned,
}

impl Found {
    fn text(&self) -> String {
        match self {
            Self::Version(version) => version.to_string(),
            Self::Unversioned => "installed".to_owned(),
        }
    }
}

/// Runs `program args...` when `program` is on `PATH` and reads a version from its output.
fn installed_version(program: &str, args: &[&str]) -> Option<Found> {
    if !proc::have(program) {
        return None;
    }
    let output = proc::stdout_of(proc::command(program).args(args)).unwrap_or_default();
    Some(Version::find_in(&output).map_or(Found::Unversioned, Found::Version))
}

fn find_pinned(name: &str) -> Option<Found> {
    match name {
        // Neither prints a version.
        "cargo-careful" | "cargo-auditable" => proc::have(name).then_some(Found::Unversioned),
        "kani-verifier" => proc::have("cargo-kani")
            .then(|| installed_version("cargo", &["kani", "--version"]))
            .flatten(),
        _ => match name.strip_prefix("cargo-") {
            Some(subcommand) => proc::have(name)
                .then(|| installed_version("cargo", &[subcommand, "--version"]))
                .flatten(),
            None => installed_version(name, &["--version"]),
        },
    }
}

fn pinned_state(pin: Pin) -> HostState {
    match find_pinned(pin.name) {
        None if pin.name == "kani-verifier" => {
            HostState::Missing("impeccable setup host --with-kani".to_owned())
        }
        None => HostState::Missing("impeccable setup host".to_owned()),
        Some(Found::Version(found)) if found < pin.version => HostState::Older {
            found,
            pinned: pin.version,
        },
        Some(found) => HostState::Ready(found.text()),
    }
}

fn miri_state() -> HostState {
    if !nightly_installed() {
        HostState::Missing("rustup toolchain install nightly --component miri".to_owned())
    } else if !nightly_has("miri") {
        HostState::Missing("rustup +nightly component add miri".to_owned())
    } else {
        HostState::Ready("installed".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(command: &[&str]) -> Option<&'static str> {
        let argv: Vec<String> = command.iter().map(|arg| (*arg).to_owned()).collect();
        Tool::of_command(&argv).map(|tool| tool.label())
    }

    #[test]
    fn cargo_subcommands_map_to_their_pins() {
        assert_eq!(
            tool(&["cargo", "kani", "--harness", "h"]),
            Some("kani-verifier")
        );
        assert_eq!(tool(&["cargo", "nextest", "run"]), Some("cargo-nextest"));
        assert_eq!(tool(&["cargo", "+nightly", "miri", "test"]), Some("miri"));
        assert_eq!(
            tool(&["cargo", "+nightly", "careful", "test"]),
            Some("cargo-careful")
        );
        assert_eq!(
            tool(&["/usr/local/cargo/bin/cargo", "deny", "check"]),
            Some("cargo-deny")
        );
        assert_eq!(tool(&["cargo.exe", "mutants"]), Some("cargo-mutants"));
    }

    #[test]
    fn standalone_programs_map_to_their_tools() {
        assert_eq!(tool(&["zizmor", "."]), Some("zizmor"));
        assert_eq!(tool(&["cargo-kani"]), Some("kani-verifier"));
        assert_eq!(tool(&["valgrind", "./target/debug/app"]), Some("valgrind"));
        assert_eq!(tool(&["tlc", "Spec.tla"]), Some("tlc"));
    }

    #[test]
    fn ordinary_commands_are_not_tracked() {
        assert_eq!(tool(&["cargo", "test"]), None);
        assert_eq!(tool(&["cargo", "bench"]), None);
        assert_eq!(tool(&["cargo"]), None);
        assert_eq!(tool(&["bash"]), None);
        assert_eq!(tool(&[]), None);
    }

    #[test]
    fn linux_only_tools_are_refused_elsewhere() {
        assert!(unsupported_reason("gungraun-runner", Os::Mac).is_some());
        assert!(unsupported_reason("valgrind", Os::Windows).is_some());
        assert!(unsupported_reason("kani-verifier", Os::Windows).is_some());
        assert_eq!(unsupported_reason("kani-verifier", Os::Mac), None);
        assert_eq!(unsupported_reason("gungraun-runner", Os::Linux), None);
        assert_eq!(unsupported_reason("cargo-nextest", Os::Windows), None);
    }

    #[test]
    fn recognises_a_nightly_build_date() {
        assert!(is_date("2026-09-25"));
        assert!(!is_date("2026-9-25"));
        assert!(!is_date("1.99.0-nightly"));
    }
}
