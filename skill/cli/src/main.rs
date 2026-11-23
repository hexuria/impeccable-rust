//! `impeccable` sets up, checks, and runs the command-line tools the impeccable-rust skill names.

mod bench_guard;
mod bench_lock;
mod docker;
mod doctor;
mod embedded;
mod failure;
mod fingerprint;
mod host;
mod manifest;
mod pins;
mod proc;
mod project;
mod run;
mod sanitize;
mod setup;

use std::process::ExitCode;

use failure::{Outcome, bail, say};

const USAGE: &str = "\
impeccable: set up, check, and run the command-line tools the impeccable-rust skill names.

  impeccable doctor                    where each tool runs: this host, the toolbox, or nowhere
  impeccable setup toolbox [--rebuild] build the Linux toolbox image (needs Docker)
  impeccable setup host [--with-kani] [--dry-run]
                                       install the pinned tools on this host
  impeccable run [--host|--toolbox] [command...]
                                       run a command on this host or in the toolbox
  impeccable sanitize <kind> [args...] cargo test under a sanitizer: address, thread, memory, leak
  impeccable bench-guard <baseline> [--allow <check>=<reason>]... [--benches <pathspec>]... [-- command...]
                                       check the benchmark contract against a baseline commit,
                                       then run the command under a machine-wide benchmark lock

Environment:
  IMPECCABLE_BACKEND=auto|host|toolbox
                             where `run` sends a command. auto (the default): a pinned tool
                             this host has runs here, everything else runs in the toolbox.
                             host: never use Docker. toolbox: always use it.
  IMPECCABLE_EPHEMERAL=1     run the toolbox with caches that are deleted afterwards, for
                             code you do not trust
  IMPECCABLE_UNCONFINED=1    run the toolbox without seccomp so gungraun can turn off ASLR
  IMPECCABLE_MOUNT=<dir>     mount this directory instead of the detected workspace root
  IMPECCABLE_BENCH_LOCK=<file>
                             benchmark lock file (default: a per-user file in /tmp)
";

fn main() -> ExitCode {
    match arguments().and_then(|args| dispatch(&args)) {
        Ok(code) => code,
        Err(failure) => {
            say(&failure.message);
            ExitCode::from(failure.code)
        }
    }
}

fn arguments() -> Outcome<Vec<String>> {
    std::env::args_os()
        .skip(1)
        .map(|arg| {
            arg.into_string().map_err(|arg| {
                failure::Failure::new(format!(
                    "argument is not valid Unicode: {}",
                    arg.to_string_lossy()
                ))
            })
        })
        .collect()
}

fn dispatch(args: &[String]) -> Outcome<ExitCode> {
    embedded::warn_if_stale();
    let Some((command, rest)) = args.split_first() else {
        print!("{USAGE}");
        return Ok(ExitCode::SUCCESS);
    };
    match command.as_str() {
        "doctor" => {
            if !rest.is_empty() {
                bail!("doctor takes no arguments");
            }
            doctor::run()
        }
        "setup" => setup::run(rest),
        "run" => run::run(rest),
        "sanitize" => sanitize::run(rest),
        "bench-guard" => bench_guard::run(rest),
        "help" | "-h" | "--help" => {
            print!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        "-V" | "--version" => {
            println!("impeccable {}", env!("CARGO_PKG_VERSION"));
            Ok(ExitCode::SUCCESS)
        }
        _ => {
            eprint!("{USAGE}");
            Ok(ExitCode::from(2))
        }
    }
}
