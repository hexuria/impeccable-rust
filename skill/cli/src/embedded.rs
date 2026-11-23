//! The skill's toolbox files and this program's own source, compiled into the binary.
//!
//! The binary therefore works from any location, and it can build the toolbox image,
//! which contains a copy of itself, without the skill folder at hand.

use std::borrow::Cow;
use std::fs;
use std::path::Path;

use crate::failure::say;
use crate::fingerprint::Fnv;

/// One embedded file and its path inside this crate, with forward slashes.
#[derive(Debug)]
pub struct File {
    pub path: &'static str,
    pub text: &'static str,
}

pub const TOOLS: &str = include_str!("../toolbox/tools.txt");
pub const DOCKERFILE: &str = include_str!("../toolbox/Dockerfile");

/// Everything the toolbox image is built from. A unit test fails when a source file is
/// missing from this list.
pub const FILES: &[File] = &[
    File {
        path: "toolbox/Dockerfile",
        text: DOCKERFILE,
    },
    File {
        path: "toolbox/entrypoint",
        text: include_str!("../toolbox/entrypoint"),
    },
    File {
        path: "toolbox/tools.txt",
        text: TOOLS,
    },
    File {
        path: "Cargo.toml",
        text: include_str!("../Cargo.toml"),
    },
    File {
        path: "Cargo.lock",
        text: include_str!("../Cargo.lock"),
    },
    File {
        path: "src/bench_guard.rs",
        text: include_str!("bench_guard.rs"),
    },
    File {
        path: "src/bench_lock.rs",
        text: include_str!("bench_lock.rs"),
    },
    File {
        path: "src/docker.rs",
        text: include_str!("docker.rs"),
    },
    File {
        path: "src/doctor.rs",
        text: include_str!("doctor.rs"),
    },
    File {
        path: "src/embedded.rs",
        text: include_str!("embedded.rs"),
    },
    File {
        path: "src/failure.rs",
        text: include_str!("failure.rs"),
    },
    File {
        path: "src/fingerprint.rs",
        text: include_str!("fingerprint.rs"),
    },
    File {
        path: "src/host.rs",
        text: include_str!("host.rs"),
    },
    File {
        path: "src/main.rs",
        text: include_str!("main.rs"),
    },
    File {
        path: "src/manifest.rs",
        text: include_str!("manifest.rs"),
    },
    File {
        path: "src/pins.rs",
        text: include_str!("pins.rs"),
    },
    File {
        path: "src/proc.rs",
        text: include_str!("proc.rs"),
    },
    File {
        path: "src/project.rs",
        text: include_str!("project.rs"),
    },
    File {
        path: "src/run.rs",
        text: include_str!("run.rs"),
    },
    File {
        path: "src/sanitize.rs",
        text: include_str!("sanitize.rs"),
    },
    File {
        path: "src/setup.rs",
        text: include_str!("setup.rs"),
    },
];

/// The text with Unix line endings.
///
/// A Windows checkout may hold CRLF files; the image must get the same bytes everywhere,
/// and a shell script with CRLF endings does not run.
pub fn unix_lines(text: &str) -> Cow<'_, str> {
    if text.contains("\r\n") {
        Cow::Owned(text.replace("\r\n", "\n"))
    } else {
        Cow::Borrowed(text)
    }
}

/// A fingerprint of every embedded file. It tags the toolbox image, so changing a pin or
/// this program yields a new image.
pub fn fingerprint() -> String {
    let mut hash = Fnv::new();
    for file in FILES {
        hash.update(file.path.as_bytes());
        hash.update(&[0]);
        hash.update(unix_lines(file.text).as_bytes());
        hash.update(&[0]);
    }
    hash.short_hex()
}

/// Warns when this binary sits in a skill folder whose files have changed since it was built.
///
/// Without this an edited pin, or an updated skill, would keep running the old rules.
pub fn warn_if_stale() {
    let Ok(exe) = std::env::current_exe().and_then(fs::canonicalize) else {
        return;
    };
    // The skill builds this binary into `<skill>/bin/`.
    let Some(skill) = exe.parent().and_then(Path::parent) else {
        return;
    };
    let crate_dir = skill.join("cli");
    if !crate_dir.join("Cargo.toml").is_file() {
        return;
    }
    if let Some(path) = first_changed(&crate_dir) {
        say(&format!(
            "this binary is older than the skill (cli/{path} changed); rebuild it from {}: \
             cargo install --locked --offline --path cli --root .",
            skill.display()
        ));
    }
}

/// The first embedded file whose copy under `crate_dir` differs, if any.
fn first_changed(crate_dir: &Path) -> Option<&'static str> {
    FILES.iter().find_map(|file| {
        let on_disk = fs::read_to_string(crate_dir.join(file.path)).ok()?;
        (unix_lines(&on_disk) != unix_lines(file.text)).then_some(file.path)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_source_file_is_embedded() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let on_disk: BTreeSet<String> = fs::read_dir(&src)
            .expect("the crate's src folder is readable in tests")
            .map(|entry| entry.expect("src entries are readable").file_name())
            .map(|name| format!("src/{}", name.to_string_lossy()))
            .collect();
        let embedded: BTreeSet<String> = FILES
            .iter()
            .map(|file| file.path.to_owned())
            .filter(|path| path.starts_with("src/"))
            .collect();
        assert_eq!(embedded, on_disk, "add new source files to embedded::FILES");
    }

    #[test]
    fn a_fresh_build_is_not_stale() {
        assert_eq!(first_changed(Path::new(env!("CARGO_MANIFEST_DIR"))), None);
    }

    #[test]
    fn line_endings_do_not_change_the_text() {
        assert_eq!(unix_lines("a\r\nb\r\n"), "a\nb\n");
        assert_eq!(unix_lines("a\nb\n"), "a\nb\n");
    }
}
