//! `impeccable setup`: build the toolbox image, or install the pinned tools on the host.

use std::process::ExitCode;

use crate::docker::{self, Rebuild};
use crate::failure::{Failure, Outcome, bail, say};
use crate::host::{self, Os};
use crate::pins;
use crate::proc;

const USAGE: &str =
    "usage: impeccable setup toolbox [--rebuild] | impeccable setup host [--with-kani] [--dry-run]";

pub fn run(args: &[String]) -> Outcome<ExitCode> {
    let Some((target, options)) = args.split_first() else {
        bail!("{USAGE}");
    };
    match target.as_str() {
        "toolbox" => {
            let rebuild = match options {
                [] => Rebuild::IfMissing,
                [flag] if flag == "--rebuild" => Rebuild::Always,
                _ => bail!("{USAGE}"),
            };
            docker::setup(rebuild)?;
        }
        "host" => setup_host(options)?,
        _ => bail!("{USAGE}"),
    }
    Ok(ExitCode::SUCCESS)
}

/// Whether `setup host` runs its commands or only prints them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Install,
    DryRun,
}

/// Runs one install command, or prints it in a dry run.
fn step(mode: Mode, argv: &[&str]) -> Outcome<()> {
    if mode == Mode::DryRun {
        println!("  {}", argv.join(" "));
        return Ok(());
    }
    let status = proc::command(argv[0])
        .args(&argv[1..])
        .status()
        .map_err(|error| Failure::new(format!("cannot run {}: {error}", argv[0])))?;
    if !status.success() {
        bail!("`{}` failed", argv.join(" "));
    }
    Ok(())
}

fn setup_host(options: &[String]) -> Outcome<()> {
    let mut mode = Mode::Install;
    let mut with_kani = false;
    for option in options {
        match option.as_str() {
            "--dry-run" => mode = Mode::DryRun,
            "--with-kani" => with_kani = true,
            other => bail!("unknown option: {other}"),
        }
    }
    for required in ["cargo", "rustup"] {
        if !proc::have(required) {
            bail!("{required} is required");
        }
    }

    if host::nightly_installed() {
        step(
            mode,
            &[
                "rustup",
                "+nightly",
                "component",
                "add",
                "miri",
                "rust-src",
                "llvm-tools-preview",
            ],
        )?;
    } else {
        step(
            mode,
            &[
                "rustup",
                "toolchain",
                "install",
                "nightly",
                "--profile",
                "minimal",
                "--component",
                "miri,rust-src,llvm-tools-preview",
            ],
        )?;
    }
    step(mode, &["rustup", "component", "add", "llvm-tools-preview"])?;

    let os = Os::current();
    let mut specs = Vec::new();
    for pin in pins::all() {
        // Kani compiles from source and downloads a solver bundle, so it is opt-in.
        if pin.name == "kani-verifier" {
            continue;
        }
        if let Some(reason) = host::unsupported_reason(pin.name, os) {
            say(&format!(
                "skipping {}: {reason}, so it runs in the toolbox",
                pin.name
            ));
            continue;
        }
        specs.push(format!("{}@{}", pin.name, pin.version));
    }
    if mode == Mode::Install && !proc::have("cargo-binstall") {
        // The newest cargo-binstall often needs a newer Rust than the one installed, so
        // name the version the toolbox pins.
        let version = pins::dockerfile_arg("BINSTALL_VERSION")
            .map(|version| format!(" --version {version}"))
            .unwrap_or_default();
        bail!("cargo-binstall is required: cargo install --locked cargo-binstall{version}");
    }
    let mut binstall = vec!["cargo", "binstall", "-y", "--disable-telemetry", "--locked"];
    binstall.extend(specs.iter().map(String::as_str));
    step(mode, &binstall)?;

    if with_kani {
        if let Some(reason) = host::unsupported_reason("kani-verifier", os) {
            say(&format!(
                "skipping kani-verifier: {reason}, so it runs in the toolbox"
            ));
            return Ok(());
        }
        let pin = pins::find("kani-verifier")
            .ok_or_else(|| Failure::new("tools.txt does not pin kani-verifier"))?;
        let version = pin.version.to_string();
        step(
            mode,
            &[
                "cargo",
                "install",
                "--locked",
                "kani-verifier",
                "--version",
                &version,
            ],
        )?;
        step(mode, &["cargo", "kani", "setup"])?;
    }
    Ok(())
}
