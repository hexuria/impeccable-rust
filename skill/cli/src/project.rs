//! Locating the project a command applies to.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::failure::{Failure, Outcome};
use crate::proc;

/// The absolute, symlink-free form of `path`, in a spelling other programs accept.
///
/// On Windows `canonicalize` returns a `\\?\` path, which git and Docker reject.
pub fn canonical(path: &Path) -> io::Result<PathBuf> {
    let resolved = fs::canonicalize(path)?;
    Ok(match resolved.to_str() {
        Some(text) => PathBuf::from(strip_verbatim(text)),
        None => resolved,
    })
}

fn strip_verbatim(path: &str) -> String {
    if let Some(share) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{share}")
    } else if let Some(plain) = path.strip_prefix(r"\\?\") {
        plain.to_owned()
    } else {
        path.to_owned()
    }
}

pub fn current_dir() -> Outcome<PathBuf> {
    env::current_dir()
        .and_then(|dir| canonical(&dir))
        .map_err(|error| Failure::new(format!("cannot read the current directory: {error}")))
}

/// The project root: `IMPECCABLE_MOUNT`, else the Cargo workspace root, else the git
/// root, else the current directory.
pub fn root() -> Outcome<PathBuf> {
    if let Some(mount) = proc::env_nonempty("IMPECCABLE_MOUNT") {
        return canonical(Path::new(&mount))
            .map_err(|error| Failure::new(format!("IMPECCABLE_MOUNT={mount}: {error}")));
    }
    let found = cargo_workspace().or_else(git_root);
    match found.and_then(|dir| canonical(&dir).ok()) {
        Some(dir) => Ok(dir),
        None => current_dir(),
    }
}

fn cargo_workspace() -> Option<PathBuf> {
    let manifest = proc::stdout_if_ok(proc::command("cargo").args([
        "locate-project",
        "--workspace",
        "--message-format",
        "plain",
    ]))?;
    Path::new(manifest.trim()).parent().map(Path::to_path_buf)
}

/// The top of the git work tree the current directory is in.
pub fn git_root() -> Option<PathBuf> {
    let top = proc::stdout_if_ok(proc::command("git").args(["rev-parse", "--show-toplevel"]))?;
    let top = top.trim();
    (!top.is_empty()).then(|| PathBuf::from(top))
}

/// `path` with forward slashes, as git and container paths are written.
pub fn slashed(path: &Path) -> String {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbatim_prefixes_are_removed() {
        assert_eq!(strip_verbatim(r"\\?\C:\work\crate"), r"C:\work\crate");
        assert_eq!(
            strip_verbatim(r"\\?\UNC\server\share\crate"),
            r"\\server\share\crate"
        );
        assert_eq!(strip_verbatim("/home/me/crate"), "/home/me/crate");
    }

    #[test]
    fn relative_paths_join_with_forward_slashes() {
        let nested: PathBuf = ["crates", "core", "benches"].iter().collect();
        assert_eq!(slashed(&nested), "crates/core/benches");
        assert_eq!(slashed(Path::new("")), "");
    }
}
