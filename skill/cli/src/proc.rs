//! Finding and running other programs the same way on every platform.

use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, ExitStatus, Stdio};

use crate::failure::{Failure, Outcome};

/// Extensions Windows tries when `PATHEXT` is unset.
const DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";

/// The value of an environment variable, when it is set and not empty.
pub fn env_nonempty(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.is_empty())
}

/// The full path of `name` on `PATH`, as a shell would resolve it.
pub fn which(name: &str) -> Option<PathBuf> {
    if Path::new(name).components().count() > 1 {
        return candidates(Path::new(name))
            .into_iter()
            .find(|path| is_executable(path));
    }
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .flat_map(|dir| candidates(&dir.join(name)))
        .find(|path| is_executable(path))
}

pub fn have(name: &str) -> bool {
    which(name).is_some()
}

/// The file names a program may have: itself, plus each `PATHEXT` extension on Windows.
fn candidates(base: &Path) -> Vec<PathBuf> {
    if !cfg!(windows) {
        return vec![base.to_path_buf()];
    }
    let mut found = Vec::new();
    if base.extension().is_some() {
        found.push(base.to_path_buf());
    }
    let extensions = env::var("PATHEXT").unwrap_or_else(|_| DEFAULT_PATHEXT.to_owned());
    for extension in extensions.split(';').filter(|ext| !ext.is_empty()) {
        let mut name = base.as_os_str().to_owned();
        name.push(extension);
        found.push(PathBuf::from(name));
    }
    found
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// A [`Command`] for `program`.
///
/// Windows starts only `.exe` files by bare name, so a `.cmd` or `.bat` launcher is
/// resolved to its full path first.
pub fn command(program: &str) -> Command {
    if cfg!(windows)
        && let Some(path) = which(program)
    {
        return Command::new(path);
    }
    Command::new(program)
}

/// What the command printed to stdout, whatever its exit status. `None` if it did not start.
pub fn stdout_of(command: &mut Command) -> Option<String> {
    let output = command
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// What the command printed to stdout, only when it exited successfully.
pub fn stdout_if_ok(command: &mut Command) -> Option<String> {
    let output = command
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// True when the command ran and exited successfully. Its output is discarded.
pub fn succeeds(command: &mut Command) -> bool {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Runs the command with this process's stdio and waits for it.
pub fn wait_for(mut command: Command) -> Outcome<ExitCode> {
    let status = command
        .status()
        .map_err(|error| cannot_run(command.get_program(), &error))?;
    Ok(exit_code(status))
}

/// Runs the command as the last act of this process.
///
/// On Unix the command replaces this process, so signals reach it directly. Elsewhere
/// this waits and passes the exit status on.
pub fn hand_over(command: Command) -> Outcome<ExitCode> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let mut command = command;
        let error = command.exec();
        Err(cannot_run(command.get_program(), &error))
    }
    #[cfg(not(unix))]
    {
        wait_for(command)
    }
}

fn cannot_run(program: &OsStr, error: &std::io::Error) -> Failure {
    Failure::new(format!("cannot run {}: {error}", program.to_string_lossy()))
}

/// Exit status of a process a signal ended, as shells report it.
const SIGNAL_BASE: i32 = 128;

fn exit_code(status: ExitStatus) -> ExitCode {
    let code = status.code().unwrap_or_else(|| signal_code(status));
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}

#[cfg(unix)]
fn signal_code(status: ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    status.signal().map_or(1, |signal| SIGNAL_BASE + signal)
}

#[cfg(not(unix))]
fn signal_code(_status: ExitStatus) -> i32 {
    SIGNAL_BASE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_program_every_rust_host_has() {
        assert!(have("cargo"), "cargo is on PATH wherever these tests run");
        assert!(!have("impeccable-no-such-program"));
    }

    #[test]
    fn captures_stdout_only_on_success() {
        let version = stdout_if_ok(command("cargo").arg("--version"));
        assert!(
            version.is_some_and(|text| text.starts_with("cargo ")),
            "cargo --version prints its name first"
        );
        assert_eq!(
            stdout_if_ok(command("cargo").arg("--no-such-flag")),
            None,
            "a failing command yields nothing"
        );
    }
}
