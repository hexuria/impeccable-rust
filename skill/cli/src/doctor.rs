//! `impeccable doctor`: where each tool runs, and what is missing.

use std::process::ExitCode;

use crate::docker::{self, Daemon};
use crate::failure::Outcome;
use crate::host::{self, HostState, Tool, Toolchain};
use crate::pins;
use crate::run::{self, Backend, Place};

const SANITIZERS: [&str; 4] = ["address", "thread", "memory", "leak"];

fn row(tool: &str, host: &str, toolbox: &str, runs: &str) {
    let line = format!("  {tool:<22} {host:<37} {toolbox:<19} {runs}");
    println!("{}", line.trim_end());
}

fn host_text(state: &HostState) -> String {
    match state {
        HostState::Ready(version) => version.clone(),
        HostState::Older { found, .. } => format!("{found}, older than pinned"),
        HostState::Missing(_) => "missing".to_owned(),
        HostState::Unsupported(reason) => format!("unsupported: {reason}"),
    }
}

/// Docker and the backend setting, which together decide the RUNS column.
struct Routing {
    backend: Backend,
    daemon: Daemon,
    /// What RUNS calls this machine: `host`, or `here` inside the toolbox.
    here: &'static str,
}

impl Routing {
    fn runs(&self, tool: &Tool, state: &HostState) -> &'static str {
        match run::place(self.backend, Some((tool.label(), state)), || self.daemon) {
            Ok(Place::Host) => self.here,
            Ok(Place::Toolbox) => "toolbox",
            Err(_) => "nowhere",
        }
    }

    /// Where a check runs when the host either supports it or does not.
    fn runs_if(&self, host_supports: bool) -> &'static str {
        if host_supports && self.backend != Backend::Toolbox {
            self.here
        } else if self.backend != Backend::Host && self.daemon == Daemon::Ready {
            "toolbox"
        } else {
            "nowhere"
        }
    }
}

pub fn run() -> Outcome<ExitCode> {
    let inside = docker::in_toolbox();
    let routing = Routing {
        // Inside the toolbox the container is the host, and there is no Docker to fall back to.
        backend: if inside {
            Backend::Host
        } else {
            Backend::from_env()?
        },
        daemon: if inside {
            Daemon::NotInstalled
        } else {
            docker::daemon()
        },
        here: if inside { "here" } else { "host" },
    };

    let triple = host::rustc_host(Toolchain::Default);
    println!(
        "impeccable doctor on {}\n",
        triple.as_deref().unwrap_or("a host without rustc")
    );
    row("TOOL", "HOST", "TOOLBOX", "RUNS");

    let missing_components = rust_rows(&routing);
    for pin in pins::all() {
        let tool = Tool::Pinned(pin);
        let state = tool.state();
        row(
            pin.name,
            &host_text(&state),
            &pin.version.to_string(),
            routing.runs(&tool, &state),
        );
    }
    for (tool, name, toolbox) in [
        (Tool::Valgrind, "valgrind", "yes"),
        (
            Tool::Tlc,
            "tlc (TLA+)",
            pins::dockerfile_arg("TLA_VERSION").unwrap_or("yes"),
        ),
    ] {
        let state = tool.state();
        row(
            name,
            &host_text(&state),
            toolbox,
            routing.runs(&tool, &state),
        );
    }
    sanitizer_rows(&routing);

    println!();
    if !inside {
        println!(
            "  backend: {} (IMPECCABLE_BACKEND=auto|host|toolbox)",
            routing.backend.name()
        );
    }
    println!("  docker: {}", docker_line(inside, routing.daemon));
    if !inside && routing.daemon != Daemon::Ready {
        println!("  The TOOLBOX column applies once Docker is running.");
    }
    if !missing_components.is_empty() {
        println!(
            "  Add the missing nightly components with: rustup +nightly component add {}",
            missing_components.join(" ")
        );
    }
    println!("  RUNS is where `impeccable run <tool>` sends each tool with this backend.");
    println!("  A tool that runs nowhere is a gap to report, not a check to skip.");
    Ok(ExitCode::SUCCESS)
}

/// Prints the toolchain rows and returns the nightly components that are not installed.
fn rust_rows(routing: &Routing) -> Vec<&'static str> {
    let pinned_nightly = pins::dockerfile_arg("NIGHTLY").unwrap_or("pinned");
    if !host::nightly_installed() {
        row(
            "rust nightly",
            "missing: rustup toolchain install nightly",
            pinned_nightly,
            "",
        );
        return Vec::new();
    }
    let date = host::nightly_date().unwrap_or_else(|| "installed".to_owned());
    row(
        "rust nightly",
        &format!("nightly {date}"),
        pinned_nightly,
        "",
    );
    let mut missing = Vec::new();
    for component in ["miri", "rust-src"] {
        let installed = host::nightly_has(component);
        if !installed {
            missing.push(component);
        }
        let host = if installed { "installed" } else { "missing" };
        let runs = if component == "miri" {
            routing.runs_if(installed)
        } else {
            ""
        };
        row(component, host, "yes", runs);
    }
    missing
}

fn sanitizer_rows(routing: &Routing) {
    let triple = host::rustc_host(Toolchain::Nightly);
    for kind in SANITIZERS {
        let supported = triple
            .as_deref()
            .is_some_and(|triple| host::sanitizer_supported(kind, triple));
        let host = match &triple {
            None => "needs nightly".to_owned(),
            Some(_) if supported => "supported".to_owned(),
            Some(triple) => format!("unsupported on {triple}"),
        };
        row(
            &format!("sanitizer {kind}"),
            &host,
            "supported",
            routing.runs_if(supported),
        );
    }
}

fn docker_line(inside: bool, daemon: Daemon) -> String {
    if inside {
        return "inside the toolbox".to_owned();
    }
    match daemon {
        Daemon::NotInstalled => "not installed".to_owned(),
        Daemon::NotRunning => "installed, not running".to_owned(),
        Daemon::NotLinux => {
            "running Windows containers; the toolbox needs Linux containers".to_owned()
        }
        Daemon::Ready => {
            let image = docker::image();
            if docker::image_built(&image) {
                format!("ready, {image} built")
            } else {
                "ready, image not built; the first toolbox run builds it".to_owned()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn routing(backend: Backend, daemon: Daemon) -> Routing {
        Routing {
            backend,
            daemon,
            here: "host",
        }
    }

    #[test]
    fn a_host_capable_check_runs_on_the_host_unless_the_toolbox_is_forced() {
        assert_eq!(
            routing(Backend::Auto, Daemon::NotInstalled).runs_if(true),
            "host"
        );
        assert_eq!(routing(Backend::Host, Daemon::Ready).runs_if(true), "host");
        assert_eq!(
            routing(Backend::Toolbox, Daemon::Ready).runs_if(true),
            "toolbox"
        );
    }

    #[test]
    fn a_check_the_host_cannot_run_needs_docker() {
        assert_eq!(
            routing(Backend::Auto, Daemon::Ready).runs_if(false),
            "toolbox"
        );
        assert_eq!(
            routing(Backend::Auto, Daemon::NotRunning).runs_if(false),
            "nowhere"
        );
        assert_eq!(
            routing(Backend::Host, Daemon::Ready).runs_if(false),
            "nowhere"
        );
    }
}
