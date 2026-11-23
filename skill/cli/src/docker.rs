//! The toolbox: a Linux Docker image with every tool pinned.

use std::env;
use std::fs;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::embedded;
use crate::failure::{Failure, Outcome, bail, say};
use crate::fingerprint;
use crate::proc;
use crate::project;

const IMAGE_REPO: &str = "impeccable-rust-toolbox";

/// Environment variables passed from the host into the toolbox when they are set.
const PASS_ENV: &[&str] = &[
    "RUSTFLAGS",
    "RUSTDOCFLAGS",
    "MIRIFLAGS",
    "RUST_BACKTRACE",
    "RUST_LOG",
    "PROPTEST_CASES",
    "LOOM_MAX_PREEMPTIONS",
    "LOOM_MAX_BRANCHES",
    "BOLERO_RANDOM_SEED",
    "NEXTEST_PROFILE",
    "ASAN_OPTIONS",
    "TSAN_OPTIONS",
    "MSAN_OPTIONS",
    "LSAN_OPTIONS",
];

/// Where the toolbox keeps state between runs: a volume name suffix and its mount point.
///
/// Each project gets its own set. A build script can write to all of them, so sharing one
/// between projects would let one project's build change what another project runs.
const CACHES: &[(&str, &str)] = &[
    ("target", "/target"),
    ("registry", "/usr/local/cargo/registry"),
    ("git", "/usr/local/cargo/git"),
    ("cache", "/cache"),
];

/// True inside the toolbox container, where commands run directly.
pub fn in_toolbox() -> bool {
    proc::env_nonempty("IMPECCABLE_IN_TOOLBOX").is_some()
}

/// Whether Docker can run a container right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Daemon {
    NotInstalled,
    NotRunning,
    Ready,
}

impl Daemon {
    /// What stops Docker from being used, as a sentence start. Empty when it is ready.
    pub fn problem(self) -> &'static str {
        match self {
            Self::NotInstalled => "Docker is not installed",
            Self::NotRunning => "Docker is not running",
            Self::Ready => "",
        }
    }
}

pub fn daemon() -> Daemon {
    if !proc::have("docker") {
        Daemon::NotInstalled
    } else if proc::succeeds(proc::command("docker").arg("info")) {
        Daemon::Ready
    } else {
        Daemon::NotRunning
    }
}

fn require_daemon() -> Outcome<()> {
    match daemon() {
        Daemon::Ready => Ok(()),
        other => bail!(
            "{}. Start it, or use host tools: `impeccable setup host` installs them, \
             and IMPECCABLE_BACKEND=host keeps every run on this host.",
            other.problem()
        ),
    }
}

/// The image name for this build of the skill, such as `impeccable-rust-toolbox:1a2b3c4d5e6f`.
pub fn image() -> String {
    format!("{IMAGE_REPO}:{}", embedded::fingerprint())
}

pub fn image_built(image: &str) -> bool {
    proc::succeeds(proc::command("docker").args(["image", "inspect", image]))
}

/// A temporary folder holding the embedded files, laid out as the Dockerfile expects.
struct BuildContext {
    dir: PathBuf,
}

impl BuildContext {
    fn create() -> Outcome<Self> {
        let dir = env::temp_dir().join(format!(
            "impeccable-toolbox-{}-{}",
            embedded::fingerprint(),
            std::process::id()
        ));
        let context = Self { dir };
        context.write_files().map_err(|error| {
            Failure::new(format!(
                "cannot write the toolbox build files to {}: {error}",
                context.dir.display()
            ))
        })?;
        Ok(context)
    }

    fn write_files(&self) -> io::Result<()> {
        for file in embedded::FILES {
            let path = self.dir.join(file.path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, embedded::unix_lines(file.text).as_bytes())?;
        }
        Ok(())
    }
}

impl Drop for BuildContext {
    fn drop(&mut self) {
        // Best effort: a leftover folder in the temp directory is harmless.
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// `impeccable setup toolbox`: builds the image unless it is already current.
pub fn setup(rebuild: Rebuild) -> Outcome<()> {
    if in_toolbox() {
        bail!("already inside the toolbox");
    }
    require_daemon()?;
    let image = image();
    if rebuild == Rebuild::IfMissing && image_built(&image) {
        say(&format!("toolbox image {image} is up to date"));
        return Ok(());
    }
    build(&image)
}

/// Whether `setup toolbox` rebuilds an image that already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rebuild {
    IfMissing,
    Always,
}

fn build(image: &str) -> Outcome<()> {
    let context = BuildContext::create()?;
    let mut command = proc::command("docker");
    command.arg("build").env("DOCKER_BUILDKIT", "1");
    // The Dockerfile mounts this secret only for the step that downloads prebuilt tools.
    match ["GH_TOKEN", "GITHUB_TOKEN"]
        .into_iter()
        .find(|name| proc::env_nonempty(name).is_some())
    {
        Some(name) => {
            command.args(["--secret", &format!("id=github_token,env={name}")]);
        }
        None => say(
            "tip: export GH_TOKEN or GITHUB_TOKEN so tool downloads avoid GitHub's anonymous rate limit",
        ),
    }
    command
        .args(["-t", image, "-t", &format!("{IMAGE_REPO}:latest"), "-f"])
        .arg(context.dir.join("toolbox").join("Dockerfile"))
        .arg(&context.dir)
        // Build progress is status, not the command's output.
        .stdout(io::stderr());

    say(&format!(
        "building {image}; the first build downloads several GB and takes a few minutes"
    ));
    let status = command
        .status()
        .map_err(|error| Failure::new(format!("cannot run docker: {error}")))?;
    if !status.success() {
        bail!("docker build failed");
    }
    say(&format!("built {image}"));
    remove_superseded(image);
    Ok(())
}

/// Removes toolbox images built from older versions of the skill.
fn remove_superseded(current: &str) {
    let Some(tags) = proc::stdout_if_ok(
        proc::command("docker").args(["image", "ls", IMAGE_REPO, "--format", "{{.Tag}}"]),
    ) else {
        return;
    };
    for tag in tags.lines().map(str::trim) {
        let old = format!("{IMAGE_REPO}:{tag}");
        if tag.is_empty() || tag == "latest" || old == current {
            continue;
        }
        say(&format!("removing superseded image {old}"));
        proc::succeeds(proc::command("docker").args(["image", "rm", &old]));
    }
}

/// Runs `command` in the toolbox on the current project. An empty command opens a shell.
pub fn run(command: &[String]) -> Outcome<ExitCode> {
    require_daemon()?;
    let image = image();
    if !image_built(&image) {
        build(&image)?;
    }

    let cwd = project::current_dir()?;
    let root = project::root()?;
    // A current directory outside the project is mounted on its own.
    let (root, inside) = match cwd.strip_prefix(&root) {
        Ok(inside) => (root.clone(), inside.to_path_buf()),
        Err(_) => (cwd.clone(), PathBuf::new()),
    };

    let mut docker = proc::command("docker");
    docker
        .args(["run", "--rm", "--init", "-v"])
        .arg(format!("{}:/work", root.display()))
        .args(["-w", &workdir(&inside)]);
    for volume in cache_volumes(&root, proc::env_nonempty("IMPECCABLE_EPHEMERAL").is_some()) {
        docker.args(["-v", &volume]);
    }
    if io::stdin().is_terminal() {
        docker.arg("-i");
    }
    if io::stdout().is_terminal() {
        docker.arg("-t");
    }
    if proc::env_nonempty("IMPECCABLE_UNCONFINED").is_some() {
        docker.args([
            "--security-opt",
            "seccomp=unconfined",
            "-e",
            "GUNGRAUN_ALLOW_ASLR=false",
        ]);
    }
    for name in PASS_ENV {
        if proc::env_nonempty(name).is_some() {
            docker.args(["-e", name]);
        }
    }
    docker.arg(&image);
    if command.is_empty() {
        docker.arg("bash");
    } else {
        docker.args(command);
    }
    say(&format!("toolbox: {image}"));
    proc::hand_over(docker)
}

/// The container directory matching a path inside the mounted project.
fn workdir(inside: &Path) -> String {
    let inside = project::slashed(inside);
    if inside.is_empty() {
        "/work".to_owned()
    } else {
        format!("/work/{inside}")
    }
}

/// The `-v` arguments for the toolbox's caches.
///
/// Ephemeral caches are anonymous volumes, which `docker run --rm` deletes with the
/// container: nothing a build writes outlives the run.
fn cache_volumes(root: &Path, ephemeral: bool) -> Vec<String> {
    let key = fingerprint::of(root.to_string_lossy().as_bytes());
    CACHES
        .iter()
        .map(|(kind, mount)| {
            if ephemeral {
                (*mount).to_owned()
            } else {
                format!("impeccable-{key}-{kind}:{mount}")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_cache_volume_is_shared_between_projects() {
        let first = cache_volumes(Path::new("/work/alpha"), false);
        let second = cache_volumes(Path::new("/work/beta"), false);
        assert_eq!(first.len(), CACHES.len());
        for volume in &first {
            assert!(
                !second.contains(volume),
                "{volume} would be writable from two projects"
            );
        }
    }

    #[test]
    fn a_project_keeps_its_volumes_between_runs() {
        assert_eq!(
            cache_volumes(Path::new("/work/alpha"), false),
            cache_volumes(Path::new("/work/alpha"), false)
        );
    }

    #[test]
    fn ephemeral_caches_are_anonymous_volumes() {
        let volumes = cache_volumes(Path::new("/work/alpha"), true);
        assert!(volumes.contains(&"/usr/local/cargo/registry".to_owned()));
        assert!(
            volumes.iter().all(|volume| !volume.contains(':')),
            "a named volume would outlive the run: {volumes:?}"
        );
    }

    #[test]
    fn the_working_directory_follows_the_host_one() {
        assert_eq!(workdir(Path::new("")), "/work");
        let nested: PathBuf = ["crates", "core"].iter().collect();
        assert_eq!(workdir(&nested), "/work/crates/core");
    }

    #[test]
    fn the_build_context_holds_a_cli_that_builds_on_its_own() {
        let context = BuildContext::create().expect("the temp directory is writable");
        for file in [
            "toolbox/Dockerfile",
            "toolbox/entrypoint",
            "toolbox/tools.txt",
        ] {
            assert!(
                context.dir.join(file).is_file(),
                "{file} is in the build context"
            );
        }
        let builds = proc::succeeds(
            proc::command("cargo")
                .args([
                    "check",
                    "--offline",
                    "--locked",
                    "--quiet",
                    "--manifest-path",
                ])
                .arg(context.dir.join("cli").join("Cargo.toml")),
        );
        assert!(
            builds,
            "the image compiles the CLI from these files alone, with no network"
        );
    }

    #[test]
    fn the_image_tag_is_a_fingerprint() {
        let image = image();
        let tag = image
            .strip_prefix("impeccable-rust-toolbox:")
            .expect("the image lives in the toolbox repository");
        assert_eq!(tag.len(), 12);
        assert!(tag.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }
}
