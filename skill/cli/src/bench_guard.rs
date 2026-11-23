//! `impeccable bench-guard`: check the benchmark contract against a baseline commit,
//! then run a benchmark command under the benchmark lock.

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::bench_lock;
use crate::failure::{Outcome, bail, say};
use crate::manifest;
use crate::proc;
use crate::project;

const CHECKS: [&str; 4] = ["benches", "build", "flags", "release"];
const USAGE: &str = "usage: impeccable bench-guard <baseline> [--allow <check>=<reason>]... [--benches <pathspec>]... [-- command...]";
/// Files a report line names before it says "and N more".
const LISTED_MAX: usize = 6;

/// A check the user accepted failing, with the reason that goes on the record.
#[derive(Debug, PartialEq, Eq)]
struct Waiver {
    check: String,
    reason: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Request {
    baseline: String,
    waivers: Vec<Waiver>,
    bench_specs: Vec<String>,
    command: Vec<String>,
}

/// `None` means the user asked for help.
fn parse(args: &[String]) -> Outcome<Option<Request>> {
    let mut request = Request::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--allow" => match args.next() {
                Some(value) => request.waivers.push(waiver(value)?),
                None => bail!("{USAGE}"),
            },
            "--benches" => match args.next() {
                Some(value) => request.bench_specs.push(value.clone()),
                None => bail!("{USAGE}"),
            },
            "--" => {
                request.command = args.cloned().collect();
                break;
            }
            "-h" | "--help" => return Ok(None),
            other => {
                if let Some(value) = other.strip_prefix("--allow=") {
                    request.waivers.push(waiver(value)?);
                } else if let Some(value) = other.strip_prefix("--benches=") {
                    request.bench_specs.push(value.to_owned());
                } else if other.starts_with('-') {
                    bail!("unknown option: {other}");
                } else if request.baseline.is_empty() {
                    request.baseline = other.to_owned();
                } else {
                    bail!("{USAGE}");
                }
            }
        }
    }
    if request.baseline.is_empty() {
        bail!("{USAGE}");
    }
    Ok(Some(request))
}

fn waiver(value: &str) -> Outcome<Waiver> {
    let (check, reason) = value.split_once('=').unwrap_or((value, ""));
    if !CHECKS.contains(&check) {
        bail!("--allow takes one of: {} (got '{check}')", CHECKS.join(" "));
    }
    if reason.is_empty() {
        bail!("--allow {check} needs a reason: --allow {check}=<why>");
    }
    Ok(Waiver {
        check: check.to_owned(),
        reason: reason.to_owned(),
    })
}

/// Prints verdicts and counts the failures nobody waived.
struct Report<'a> {
    waivers: &'a [Waiver],
    failed: usize,
}

fn row(status: &str, check: &str, detail: &str) {
    println!("  {status:<7} {check:<8} {detail}");
}

impl Report<'_> {
    fn pass(&self, check: &str, detail: &str) {
        row("PASS", check, detail);
    }

    fn fail(&mut self, check: &str, detail: &str) {
        match self.waivers.iter().find(|waiver| waiver.check == check) {
            Some(waiver) => row(
                "WAIVED",
                check,
                &format!("{detail}; reason: {}", waiver.reason),
            ),
            None => {
                row("FAIL", check, detail);
                self.failed += 1;
            }
        }
    }

    /// Reports a check: an empty `differing` list passes, anything else fails and is listed.
    fn expect_none(&mut self, check: &str, differing: &[String], pass: &str, fail: &str) {
        if differing.is_empty() {
            self.pass(check, pass);
        } else {
            self.fail(check, &format!("{fail}: {}", listed(differing)));
        }
    }
}

/// At most [`LISTED_MAX`] items on one line, then a count of the rest.
fn listed(items: &[String]) -> String {
    let shown = items
        .iter()
        .take(LISTED_MAX)
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    match items.len().checked_sub(LISTED_MAX) {
        Some(more) if more > 0 => format!("{shown} and {more} more"),
        _ => shown,
    }
}

/// A git work tree and the baseline commit to compare it with.
struct Repo {
    top: PathBuf,
    baseline: String,
}

impl Repo {
    fn git(&self, args: &[&str]) -> Option<String> {
        proc::stdout_if_ok(proc::command("git").arg("-C").arg(&self.top).args(args))
    }

    /// Paths matching the pathspecs that differ from the baseline, including untracked files.
    fn changed(&self, pathspecs: &[&str]) -> Vec<String> {
        let mut diff = vec!["diff", "--name-only", self.baseline.as_str(), "--"];
        diff.extend_from_slice(pathspecs);
        let mut untracked = vec!["ls-files", "--others", "--exclude-standard", "--"];
        untracked.extend_from_slice(pathspecs);
        let paths: BTreeSet<String> = [diff, untracked]
            .iter()
            .filter_map(|args| self.git(args))
            .flat_map(|output| output.lines().map(str::to_owned).collect::<Vec<_>>())
            .filter(|path| !path.is_empty())
            .collect();
        paths.into_iter().collect()
    }

    /// A file's text at the baseline commit; empty when it did not exist.
    fn at_baseline(&self, path: &str) -> String {
        self.git(&["show", &format!("{}:{path}", self.baseline)])
            .unwrap_or_default()
    }
}

pub fn run(args: &[String]) -> Outcome<ExitCode> {
    let Some(request) = parse(args)? else {
        say(USAGE);
        return Ok(ExitCode::SUCCESS);
    };

    let Some(top) = project::git_root().and_then(|top| project::canonical(&top).ok()) else {
        bail!("bench-guard needs a git repository");
    };
    let repo = Repo {
        top,
        baseline: request.baseline.clone(),
    };
    if repo
        .git(&[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{}^{{commit}}", repo.baseline),
        ])
        .is_none()
    {
        bail!("not a commit: {}", repo.baseline);
    }
    let summary = repo
        .git(&["log", "-1", "--format=%h %s", &repo.baseline])
        .unwrap_or_default();
    println!("impeccable bench-guard against {}\n", summary.trim());

    // Cargo files of the project, as paths git understands; `None` outside the work tree.
    let root = project::root()?;
    let in_repo = |file: &str| {
        root.strip_prefix(&repo.top)
            .ok()
            .map(|inside| project::slashed(&inside.join(file)))
    };

    let mut report = Report {
        waivers: &request.waivers,
        failed: 0,
    };

    // benches: the benchmark definitions are the contract.
    let specs: Vec<&str> = if request.bench_specs.is_empty() {
        vec![":(glob)**/benches/**"]
    } else {
        request.bench_specs.iter().map(String::as_str).collect()
    };
    report.expect_none(
        "benches",
        &repo.changed(&specs),
        "benchmark files match the baseline",
        "benchmark files differ from the baseline",
    );

    // build: toolchain, cargo config, and profile tables.
    let mut build = repo.changed(&[
        ":(glob)**/.cargo/config.toml",
        ":(glob)**/.cargo/config",
        ":(glob)**/rust-toolchain",
        ":(glob)**/rust-toolchain.toml",
    ]);
    if let Some(manifest_path) = in_repo("Cargo.toml")
        && let Ok(now) = fs::read_to_string(repo.top.join(&manifest_path))
        && manifest::profile_settings(&repo.at_baseline(&manifest_path))
            != manifest::profile_settings(&now)
    {
        build.push(format!("{manifest_path} [profile]"));
    }
    report.expect_none(
        "build",
        &build,
        "toolchain, cargo config, and profiles match the baseline",
        "build settings differ from the baseline",
    );

    // flags: compiler flags from the environment or user config do not ship.
    report.expect_none(
        "flags",
        &stray_flags(),
        "no compiler flags from the environment or user cargo config",
        "compiler flags that do not ship",
    );

    // release: a cargo command must build an optimized profile.
    if !request.command.is_empty() {
        match built_profile(&request.command) {
            Built::NotCargo => row(
                "NOTE",
                "release",
                "not a cargo command; confirm it builds with optimizations",
            ),
            Built::Unoptimized {
                subcommand,
                profile,
            } => report.fail(
                "release",
                &format!("cargo {subcommand} builds the {profile} profile; add --release"),
            ),
            Built::Optimized {
                subcommand,
                profile,
            } => {
                report.pass(
                    "release",
                    &format!("cargo {subcommand} builds the {profile} profile"),
                );
            }
        }
    }

    // Notes route new risk to its owner; they never fail the run.
    let added = added_unsafe(&repo);
    if !added.is_empty() {
        row(
            "NOTE",
            "unsafe",
            &format!("added lines with unsafe, for Miri: {}", listed(&added)),
        );
    }
    if let Some(lock_path) = in_repo("Cargo.lock")
        && let Ok(now) = fs::read_to_string(repo.top.join(&lock_path))
    {
        let before = manifest::lock_packages(&repo.at_baseline(&lock_path));
        let new: Vec<String> = manifest::lock_packages(&now)
            .into_iter()
            .filter(|package| !before.contains(package))
            .collect();
        if !new.is_empty() {
            row(
                "NOTE",
                "deps",
                &format!(
                    "crates added or changed, for the dependency checks: {}",
                    listed(&new)
                ),
            );
        }
    }

    if report.failed > 0 {
        println!();
        bail!(
            "{} check(s) failed. Restore the contract, or waive a check on the record with --allow <check>=<reason>.",
            report.failed
        );
    }
    let Some((program, arguments)) = request.command.split_first() else {
        return Ok(ExitCode::SUCCESS);
    };

    let _lock = bench_lock::acquire()?;
    println!();
    say(&format!(
        "running under the benchmark lock: {}",
        request.command.join(" ")
    ));
    let mut command = proc::command(program);
    command.args(arguments);
    proc::wait_for(command)
}

/// `path (count)` for every Rust file with added lines containing `unsafe`.
fn added_unsafe(repo: &Repo) -> Vec<String> {
    let diff = repo
        .git(&["diff", "-U0", &repo.baseline, "--", "*.rs"])
        .unwrap_or_default();
    let mut counts = manifest::added_unsafe(&diff);
    let untracked = repo
        .git(&["ls-files", "--others", "--exclude-standard", "--", "*.rs"])
        .unwrap_or_default();
    for path in untracked.lines().filter(|path| !path.is_empty()) {
        let count = fs::read_to_string(repo.top.join(path))
            .map(|text| manifest::unsafe_lines(&text))
            .unwrap_or(0);
        if count > 0 {
            counts.insert(path.to_owned(), count);
        }
    }
    counts
        .into_iter()
        .map(|(path, count)| format!("{path} ({count})"))
        .collect()
}

/// Compiler flags that come from the environment or the user's own cargo config.
fn stray_flags() -> Vec<String> {
    let mut found: BTreeSet<String> = env::vars_os()
        .filter_map(|(name, _)| name.into_string().ok())
        .filter(|name| is_flag_variable(name))
        .collect();
    if let Some(cargo_home) = cargo_home() {
        for file in ["config.toml", "config"] {
            let path = cargo_home.join(file);
            if fs::read_to_string(&path).is_ok_and(|text| sets_rustflags(&text)) {
                found.insert(path.display().to_string());
            }
        }
    }
    found.into_iter().collect()
}

fn cargo_home() -> Option<PathBuf> {
    env::var_os("CARGO_HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::home_dir().map(|home| home.join(".cargo")))
}

/// True for an environment variable that changes how cargo compiles.
fn is_flag_variable(name: &str) -> bool {
    let is_upper = |text: &str| {
        !text.is_empty()
            && text
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    };
    matches!(
        name,
        "RUSTFLAGS" | "CARGO_ENCODED_RUSTFLAGS" | "CARGO_BUILD_RUSTFLAGS"
    ) || name
        .strip_prefix("CARGO_TARGET_")
        .and_then(|rest| rest.strip_suffix("_RUSTFLAGS"))
        .is_some_and(is_upper)
        || name.strip_prefix("CARGO_PROFILE_").is_some_and(is_upper)
}

/// True when a cargo config file has a `rustflags = ...` line.
fn sets_rustflags(config: &str) -> bool {
    config.lines().any(|line| {
        line.trim_start()
            .strip_prefix("rustflags")
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    })
}

/// The profile a benchmark command builds.
#[derive(Debug, PartialEq, Eq)]
enum Built {
    NotCargo,
    Unoptimized { subcommand: String, profile: String },
    Optimized { subcommand: String, profile: String },
}

fn built_profile(command: &[String]) -> Built {
    let is_cargo = |arg: &String| {
        let name = Path::new(arg).file_name().and_then(|name| name.to_str());
        matches!(name, Some("cargo" | "cargo.exe"))
    };
    let Some(cargo_at) = command.iter().position(is_cargo) else {
        return Built::NotCargo;
    };

    let mut subcommand = "";
    let mut profile = "";
    let mut args = command[cargo_at + 1..].iter().map(String::as_str);
    while let Some(arg) = args.next() {
        match arg {
            "--release" | "-r" => profile = "release",
            "--profile" => profile = args.next().unwrap_or(""),
            // Everything after `--` belongs to the program cargo runs.
            "--" => break,
            _ => {
                if let Some(named) = arg.strip_prefix("--profile=") {
                    profile = named;
                } else if !arg.starts_with(['+', '-']) && subcommand.is_empty() {
                    subcommand = arg;
                }
            }
        }
    }
    if subcommand == "bench" && profile.is_empty() {
        profile = "bench";
    }
    match profile {
        "" | "dev" | "test" => Built::Unoptimized {
            subcommand: subcommand.to_owned(),
            profile: if profile.is_empty() { "dev" } else { profile }.to_owned(),
        },
        _ => Built::Optimized {
            subcommand: subcommand.to_owned(),
            profile: profile.to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_owned()).collect()
    }

    fn profile_of(command: &[&str]) -> Option<(bool, String)> {
        match built_profile(&args(command)) {
            Built::NotCargo => None,
            Built::Optimized { profile, .. } => Some((true, profile)),
            Built::Unoptimized { profile, .. } => Some((false, profile)),
        }
    }

    #[test]
    fn cargo_bench_is_optimized_by_default() {
        assert_eq!(
            profile_of(&["cargo", "bench"]),
            Some((true, "bench".into()))
        );
        assert_eq!(
            profile_of(&["cargo", "+nightly", "bench", "--bench", "x"]),
            Some((true, "bench".into()))
        );
    }

    #[test]
    fn cargo_run_and_test_need_an_optimized_profile() {
        assert_eq!(
            profile_of(&["cargo", "run", "--example", "perf"]),
            Some((false, "dev".into()))
        );
        assert_eq!(
            profile_of(&["cargo", "test", "--profile", "test"]),
            Some((false, "test".into()))
        );
        assert_eq!(
            profile_of(&["cargo", "run", "--release"]),
            Some((true, "release".into()))
        );
        assert_eq!(
            profile_of(&["cargo", "run", "-r", "--bin", "perf"]),
            Some((true, "release".into()))
        );
        assert_eq!(
            profile_of(&["cargo", "run", "--profile=profiling"]),
            Some((true, "profiling".into()))
        );
        assert_eq!(
            profile_of(&["cargo", "bench", "--profile", "dev"]),
            Some((false, "dev".into()))
        );
    }

    #[test]
    fn flags_after_the_separator_belong_to_the_program() {
        assert_eq!(
            profile_of(&["cargo", "run", "--", "--release"]),
            Some((false, "dev".into())),
            "--release after -- does not optimize the build"
        );
    }

    #[test]
    fn cargo_is_found_behind_a_wrapper_and_by_path() {
        assert_eq!(
            profile_of(&["taskset", "-c", "2", "cargo", "bench"]),
            Some((true, "bench".into()))
        );
        assert_eq!(
            profile_of(&["/usr/local/cargo/bin/cargo", "bench"]),
            Some((true, "bench".into()))
        );
        assert_eq!(
            profile_of(&["./target/release/perf", "--iters", "10"]),
            None
        );
    }

    #[test]
    fn the_subcommand_is_named_in_the_verdict() {
        assert_eq!(
            built_profile(&args(&["cargo", "--profile", "dev", "run"])),
            Built::Unoptimized {
                subcommand: "run".into(),
                profile: "dev".into()
            },
            "a profile name is not mistaken for the subcommand"
        );
    }

    #[test]
    fn only_compile_affecting_variables_are_flagged() {
        for name in [
            "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_BUILD_RUSTFLAGS",
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS",
            "CARGO_PROFILE_RELEASE_LTO",
        ] {
            assert!(is_flag_variable(name), "{name}");
        }
        for name in [
            "RUSTDOCFLAGS",
            "CARGO_HOME",
            "CARGO_TARGET_DIR",
            "CARGO_PROFILE_",
            "CARGO_TARGET__RUSTFLAGS_X",
        ] {
            assert!(!is_flag_variable(name), "{name}");
        }
    }

    #[test]
    fn rustflags_in_a_cargo_config_are_found() {
        assert!(sets_rustflags(
            "[build]\n  rustflags = [\"-C\", \"target-cpu=native\"]\n"
        ));
        assert!(sets_rustflags("rustflags=[]"));
        assert!(!sets_rustflags("# rustflags = []\n[build]\njobs = 4\n"));
        assert!(!sets_rustflags("rustflags_extra = 1\n"));
    }

    #[test]
    fn long_lists_are_cut_with_a_count() {
        let few = args(&["a", "b"]);
        assert_eq!(listed(&few), "a b");
        let many = args(&["1", "2", "3", "4", "5", "6", "7", "8"]);
        assert_eq!(listed(&many), "1 2 3 4 5 6 and 2 more");
    }

    #[test]
    fn options_and_the_command_are_separated() {
        let request = parse(&args(&[
            "main",
            "--allow",
            "benches=new case added",
            "--benches=perf/**",
            "--",
            "cargo",
            "bench",
            "--allow",
        ]))
        .expect("valid arguments")
        .expect("not a help request");
        assert_eq!(request.baseline, "main");
        assert_eq!(
            request.waivers,
            [Waiver {
                check: "benches".into(),
                reason: "new case added".into()
            }]
        );
        assert_eq!(request.bench_specs, ["perf/**"]);
        assert_eq!(request.command, ["cargo", "bench", "--allow"]);
    }

    #[test]
    fn a_waiver_needs_a_known_check_and_a_reason() {
        assert!(parse(&args(&["main", "--allow", "benches"])).is_err());
        assert!(parse(&args(&["main", "--allow", "speed=because"])).is_err());
        assert!(
            parse(&args(&["--allow", "flags=ok"])).is_err(),
            "a baseline is required"
        );
        assert!(parse(&args(&["main", "extra"])).is_err());
        assert!(parse(&args(&["main", "--bogus"])).is_err());
        assert_eq!(parse(&args(&["--help"])).expect("help parses"), None);
    }
}
