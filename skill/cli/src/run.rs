//! `impeccable run`: send a command to the host or to the toolbox.

use std::process::ExitCode;

use crate::docker::{self, Daemon};
use crate::failure::{Outcome, bail, say};
use crate::host::{HostState, Tool};
use crate::proc;

/// Who decides where a command runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// A pinned tool the host has runs there; everything else runs in the toolbox.
    Auto,
    /// Everything runs on the host. Docker is never used.
    Host,
    /// Everything runs in the toolbox.
    Toolbox,
}

impl Backend {
    /// Reads `IMPECCABLE_BACKEND`, defaulting to [`Backend::Auto`].
    pub fn from_env() -> Outcome<Self> {
        match proc::env_nonempty("IMPECCABLE_BACKEND").as_deref() {
            None | Some("auto") => Ok(Self::Auto),
            Some("host") => Ok(Self::Host),
            Some("toolbox") => Ok(Self::Toolbox),
            Some(other) => {
                bail!("IMPECCABLE_BACKEND must be auto, host, or toolbox (got '{other}')")
            }
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Host => "host",
            Self::Toolbox => "toolbox",
        }
    }
}

/// Where a command runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Host,
    Toolbox,
}

/// Decides where a command runs, or explains why it can run nowhere.
///
/// `tool` is the tracked tool the command invokes and what the host can do with it;
/// `None` means an ordinary command such as `cargo test`. `daemon` is called only when
/// the answer depends on Docker, because asking Docker is slow.
pub fn place(
    backend: Backend,
    tool: Option<(&str, &HostState)>,
    daemon: impl FnOnce() -> Daemon,
) -> Result<Place, String> {
    let toolbox = |daemon: Daemon, otherwise: String| match daemon {
        Daemon::Ready => Ok(Place::Toolbox),
        Daemon::NotInstalled | Daemon::NotRunning => Err(otherwise),
    };
    match (backend, tool) {
        (Backend::Toolbox, _) => {
            let daemon = daemon();
            toolbox(
                daemon,
                format!("{}, and the toolbox backend needs it.", daemon.problem()),
            )
        }
        (Backend::Host, None) => Ok(Place::Host),
        (Backend::Auto, None) => {
            let daemon = daemon();
            toolbox(
                daemon,
                format!(
                    "{}. A command that is not a pinned tool runs in the toolbox; \
                     to run it on this host instead, use `impeccable run --host`.",
                    daemon.problem()
                ),
            )
        }
        (_, Some((_, HostState::Ready(_)))) => Ok(Place::Host),
        (Backend::Host, Some((_, HostState::Older { .. }))) => Ok(Place::Host),
        (Backend::Auto, Some((_, HostState::Older { .. }))) => Ok(match daemon() {
            Daemon::Ready => Place::Toolbox,
            Daemon::NotInstalled | Daemon::NotRunning => Place::Host,
        }),
        (Backend::Host, Some((label, HostState::Missing(install)))) => Err(format!(
            "{label} is not installed on this host. Install it with: {install}"
        )),
        (Backend::Auto, Some((label, HostState::Missing(install)))) => {
            let daemon = daemon();
            toolbox(
                daemon,
                format!(
                    "{label} is not installed on this host, and {}. Install it with: {install}",
                    daemon.problem()
                ),
            )
        }
        (Backend::Host, Some((label, HostState::Unsupported(reason)))) => Err(format!(
            "{label} cannot run on this host ({reason}). It needs the toolbox or a Linux \
             machine; until it runs, report this check as a gap."
        )),
        (Backend::Auto, Some((label, HostState::Unsupported(reason)))) => {
            let daemon = daemon();
            toolbox(
                daemon,
                format!(
                    "{label} cannot run on this host ({reason}), and {}. It needs the toolbox \
                     or a Linux machine; until it runs, report this check as a gap.",
                    daemon.problem()
                ),
            )
        }
    }
}

const USAGE: &str = "usage: impeccable run [--host|--toolbox] [command...]";

pub fn run(args: &[String]) -> Outcome<ExitCode> {
    let (backend, command) = parse(args, Backend::from_env()?)?;
    if docker::in_toolbox() {
        return proc::hand_over(host_command(command.unwrap_or(&["bash".to_owned()])));
    }

    let tool = command.and_then(Tool::of_command);
    let state = tool.as_ref().map(Tool::state);
    let tracked = tool.as_ref().map(Tool::label).zip(state.as_ref());
    match place(backend, tracked, docker::daemon).map_err(crate::failure::Failure::new)? {
        Place::Host => {
            let Some(command) = command else {
                bail!("give a command to run on the host. {USAGE}");
            };
            announce_host(tracked);
            proc::hand_over(host_command(command))
        }
        Place::Toolbox => docker::run(command.unwrap_or(&[])),
    }
}

/// Splits the leading backend flags from the command. `None` means no command was given.
fn parse(args: &[String], from_env: Backend) -> Outcome<(Backend, Option<&[String]>)> {
    let mut backend = from_env;
    let mut rest = args;
    while let Some((first, tail)) = rest.split_first() {
        match first.as_str() {
            "--host" => backend = Backend::Host,
            "--toolbox" => backend = Backend::Toolbox,
            "--" => {
                rest = tail;
                break;
            }
            "-h" | "--help" => bail!("{USAGE}"),
            flag if flag.starts_with('-') => bail!("unknown option: {flag}. {USAGE}"),
            _ => break,
        }
        rest = tail;
    }
    Ok((backend, (!rest.is_empty()).then_some(rest)))
}

fn host_command(argv: &[String]) -> std::process::Command {
    let mut command = proc::command(&argv[0]);
    command.args(&argv[1..]);
    command
}

/// Says which host tool is about to run, so a result names the version that produced it.
fn announce_host(tracked: Option<(&str, &HostState)>) {
    match tracked {
        Some((label, HostState::Ready(version))) => say(&format!("host: {label} {version}")),
        Some((label, HostState::Older { found, pinned })) => say(&format!(
            "host: {label} {found} is older than the pinned {pinned}; results may differ from the toolbox"
        )),
        Some((_, HostState::Missing(_) | HostState::Unsupported(_))) | None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pins::Version;

    fn ready() -> HostState {
        HostState::Ready("1.2.3".to_owned())
    }

    fn older() -> HostState {
        let version = |text| Version::find_in(text).expect("test input holds a version");
        HostState::Older {
            found: version("1.0.0"),
            pinned: version("1.2.3"),
        }
    }

    fn missing() -> HostState {
        HostState::Missing("impeccable setup host".to_owned())
    }

    fn unsupported() -> HostState {
        HostState::Unsupported("no Valgrind on macOS")
    }

    fn placed(backend: Backend, state: Option<&HostState>, daemon: Daemon) -> Option<Place> {
        place(backend, state.map(|state| ("tool", state)), || daemon).ok()
    }

    #[test]
    fn auto_prefers_a_ready_host_tool_and_never_asks_docker() {
        let asked = place(Backend::Auto, Some(("tool", &ready())), || {
            panic!("a ready host tool must not query Docker")
        });
        assert_eq!(asked, Ok(Place::Host));
    }

    #[test]
    fn auto_falls_back_to_the_toolbox() {
        for state in [missing(), unsupported(), older()] {
            assert_eq!(
                placed(Backend::Auto, Some(&state), Daemon::Ready),
                Some(Place::Toolbox),
                "{state:?}"
            );
        }
        assert_eq!(
            placed(Backend::Auto, None, Daemon::Ready),
            Some(Place::Toolbox)
        );
    }

    #[test]
    fn auto_without_docker_uses_what_the_host_has() {
        assert_eq!(
            placed(Backend::Auto, Some(&older()), Daemon::NotRunning),
            Some(Place::Host),
            "an older host tool beats no tool"
        );
        assert_eq!(
            placed(Backend::Auto, Some(&missing()), Daemon::NotRunning),
            None
        );
        assert_eq!(
            placed(Backend::Auto, Some(&unsupported()), Daemon::NotInstalled),
            None
        );
        assert_eq!(placed(Backend::Auto, None, Daemon::NotInstalled), None);
    }

    #[test]
    fn host_backend_never_uses_docker() {
        let no_docker = || -> Daemon { panic!("the host backend must not query Docker") };
        assert_eq!(place(Backend::Host, None, no_docker), Ok(Place::Host));
        assert_eq!(
            place(Backend::Host, Some(("tool", &ready())), no_docker),
            Ok(Place::Host)
        );
        assert_eq!(
            place(Backend::Host, Some(("tool", &older())), no_docker),
            Ok(Place::Host)
        );
        assert!(place(Backend::Host, Some(("tool", &missing())), no_docker).is_err());
        assert!(place(Backend::Host, Some(("tool", &unsupported())), no_docker).is_err());
    }

    #[test]
    fn toolbox_backend_needs_docker() {
        assert_eq!(
            placed(Backend::Toolbox, Some(&ready()), Daemon::Ready),
            Some(Place::Toolbox)
        );
        assert_eq!(
            placed(Backend::Toolbox, None, Daemon::Ready),
            Some(Place::Toolbox)
        );
        assert_eq!(
            placed(Backend::Toolbox, Some(&ready()), Daemon::NotRunning),
            None
        );
    }

    #[test]
    fn a_tool_that_runs_nowhere_says_how_to_fix_it() {
        let message = place(Backend::Auto, Some(("cargo-kani", &missing())), || {
            Daemon::NotRunning
        })
        .expect_err("a missing tool with no Docker runs nowhere");
        assert!(message.contains("cargo-kani"), "{message}");
        assert!(message.contains("impeccable setup host"), "{message}");

        let message = place(Backend::Host, Some(("valgrind", &unsupported())), || {
            Daemon::Ready
        })
        .expect_err("an unsupported tool cannot run on the host");
        assert!(message.contains("report this check as a gap"), "{message}");
    }

    #[test]
    fn flags_come_before_the_command() {
        let args =
            |list: &[&str]| -> Vec<String> { list.iter().map(|arg| (*arg).to_owned()).collect() };

        let input = args(&["--host", "cargo", "test", "--toolbox"]);
        let (backend, command) = parse(&input, Backend::Auto).expect("valid flags");
        assert_eq!(backend, Backend::Host);
        assert_eq!(
            command,
            Some(&input[1..]),
            "flags after the command belong to it"
        );

        let input = args(&["--toolbox"]);
        let (backend, command) = parse(&input, Backend::Auto).expect("valid flags");
        assert_eq!((backend, command), (Backend::Toolbox, None));

        let input = args(&["--", "--odd-program"]);
        let (_, command) = parse(&input, Backend::Auto).expect("valid flags");
        assert_eq!(command, Some(&input[1..]));

        assert!(parse(&args(&["--nope", "cargo"]), Backend::Auto).is_err());
    }
}
