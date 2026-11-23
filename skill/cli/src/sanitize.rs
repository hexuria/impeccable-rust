//! `impeccable sanitize`: `cargo test` under a sanitizer, with the flags it needs.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use crate::docker;
use crate::failure::{Outcome, bail, say};
use crate::host::{self, Toolchain};
use crate::manifest;
use crate::proc;
use crate::run::Backend;

const KINDS: [&str; 4] = ["address", "thread", "memory", "leak"];

pub fn run(args: &[String]) -> Outcome<ExitCode> {
    let Some((kind, test_args)) = args.split_first() else {
        bail!(
            "usage: impeccable sanitize <{}> [cargo test args]",
            KINDS.join("|")
        );
    };
    let kind = kind.as_str();
    if !KINDS.contains(&kind) {
        bail!("unknown sanitizer: {kind}");
    }
    if !host::nightly_installed() {
        bail!("a nightly toolchain is required: rustup toolchain install nightly");
    }
    let Some(triple) = host::rustc_host(Toolchain::Nightly) else {
        bail!("cannot read the host triple from `rustc +nightly -vV`");
    };

    if !host::sanitizer_supported(kind, &triple) {
        if docker::in_toolbox() {
            bail!("the {kind} sanitizer is not supported on {triple}");
        }
        if Backend::from_env()? == Backend::Host {
            bail!(
                "the {kind} sanitizer is not supported on {triple}. It needs the toolbox or a \
                 Linux machine; until it runs, report this check as a gap."
            );
        }
        say(&format!(
            "the {kind} sanitizer is not supported on {triple}; running it in the toolbox"
        ));
        let mut inside = vec![
            "impeccable".to_owned(),
            "sanitize".to_owned(),
            kind.to_owned(),
        ];
        inside.extend_from_slice(test_args);
        return docker::run(&inside);
    }

    // Without an instrumented std, rustc refuses to link (ABI mismatch) or reports noise.
    let build_std = matches!(kind, "thread" | "memory");
    if build_std && !host::nightly_has("rust-src") {
        bail!("the {kind} sanitizer rebuilds std: rustup +nightly component add rust-src");
    }

    let mut flags = format!("-Zsanitizer={kind}");
    if kind == "memory" {
        flags.push_str(" -Zsanitizer-memory-track-origins");
    }
    let rustflags = appended("RUSTFLAGS", &flags);

    let mut cargo = proc::command("cargo");
    cargo.args(["+nightly", "test"]);
    if build_std {
        cargo.arg("-Zbuild-std");
    }
    cargo
        .args(["--target", &triple])
        .args(test_args)
        .env("RUSTFLAGS", &rustflags)
        .env("RUSTDOCFLAGS", appended("RUSTDOCFLAGS", &flags))
        // A sanitized build must not share artifacts with a normal one.
        .env(
            "CARGO_TARGET_DIR",
            target_dir().join(format!("sanitize-{kind}")),
        );
    // macOS turns leak detection off by default and Linux has it on. Windows has no
    // leak detection, and AddressSanitizer refuses to start when asked for it there.
    if kind == "address" && !cfg!(windows) && proc::env_nonempty("ASAN_OPTIONS").is_none() {
        cargo.env("ASAN_OPTIONS", "detect_leaks=1");
    }

    say(&format!(
        "RUSTFLAGS='{rustflags}' cargo +nightly test {}--target {triple} {}",
        if build_std { "-Zbuild-std " } else { "" },
        test_args.join(" ")
    ));
    proc::hand_over(cargo)
}

/// The value of an environment variable with `flags` added at the end.
fn appended(variable: &str, flags: &str) -> String {
    match proc::env_nonempty(variable) {
        Some(existing) => format!("{existing} {flags}"),
        None => flags.to_owned(),
    }
}

/// The project's Cargo target directory.
fn target_dir() -> PathBuf {
    if let Some(dir) = env::var_os("CARGO_TARGET_DIR").filter(|dir| !dir.is_empty()) {
        return PathBuf::from(dir);
    }
    proc::stdout_if_ok(proc::command("cargo").args([
        "metadata",
        "--format-version",
        "1",
        "--no-deps",
    ]))
    .and_then(|metadata| manifest::json_string_field(&metadata, "target_directory"))
    .map_or_else(|| PathBuf::from("target"), PathBuf::from)
}
