# impeccable-rust

An agent skill for writing, reviewing, and verifying high-stakes Rust. It works
with Claude Code, Codex, Cursor, and any agent that loads `SKILL.md` skills.

The skill makes an agent treat "it compiles and the tests pass" as the start of
the job. The agent checks the change against the failures that can actually
happen, picks the right verifier for each one, and reports what was checked,
under which bounds, and what was left out.

## What it does

- **Checklist for every change.** Error-path tests, chaos and property tests,
  Miri and sanitizers on `unsafe`, Loom and Kani where they apply, trustworthy
  benchmarks, misuse-resistant APIs, semver hygiene, and dependency vetting.
- **Tools it knows.** Everything the talk names: Miri, sanitizers, turmoil,
  shuttle, quickcheck, proptest, cargo-mutants, Loom, Kani, gungraun (formerly
  iai-callgrind), tango-bench, Clippy, cargo-semver-checks, cargo-public-api,
  RUSTSEC, and cargo-vet. Added beyond it: Bolero, cargo-careful, nextest,
  cargo-llvm-cov, cargo-fuzz, cargo-deny, cargo-auditable, zizmor, TLA+,
  Stateright, Creusot, Verus, Lean, Aeneas, and hax.
- **Risk-to-owner routing.** Each failure class gets one owner: fuzzing for
  untrusted input, Miri and cargo-careful for `unsafe`, Loom for lock-free code,
  TLA+ or Stateright for multi-actor designs, Creusot, Verus, or Lean for proof
  kernels, cargo-auditable and zizmor for supply chain and CI.
- **Optimization loop with a benchmark contract.** Asked to make code faster,
  the agent freezes a baseline, tries one measured hypothesis at a time, keeps
  only changes that beat the baseline without breaking the oracle, and stops
  when gains converge. `impeccable bench-guard` fails the run if benchmarks,
  build settings, or compiler flags moved since the baseline.
- **Differential oracle for rewrites.** A rewrite, port, or optimization keeps
  the old implementation until the new one matches it on generated inputs.
- **Anti-drift rules.** One harness per property, and no second model of the
  same state machine without a conformance link to the production Rust.
- **Honest reports.** Each claim uses a precise term such as property-tested,
  Miri-checked, or bounded model-checked, with its bounds, assumptions, and
  blind spots. A bounded check is never called a proof.
- **Audit mode.** Asked to review verification, the agent reads the repo
  first, maps what already runs, and recommends only what fills a real gap.

## Install

Copy `skill/` into your agent's skills folder under the name `impeccable-rust`:

```sh
git clone https://github.com/hexuria/impeccable-rust
mkdir -p ~/.claude/skills
cp -r impeccable-rust/skill ~/.claude/skills/impeccable-rust
```

To get updates with `git pull`, link the folder instead of copying it:

```sh
ln -s "$PWD/impeccable-rust/skill" ~/.claude/skills/impeccable-rust
```

On Windows, in PowerShell:

```powershell
git clone https://github.com/hexuria/impeccable-rust
Copy-Item -Recurse impeccable-rust\skill "$HOME\.claude\skills\impeccable-rust"
```

The folder name must be `impeccable-rust`, matching the skill's `name`. Strict
loaders reject a folder named `skill`. For other agents, put the same folder
wherever they load skills from, for example `.claude/skills/` or
`.cursor/skills/` inside a project.

## Tooling

The skill comes with `impeccable`, a small Rust program that sets up and runs
the command-line tools the skill names. It has no dependencies and works on
Linux, macOS, and Windows. Put it on your `PATH` one of these ways:

```sh
cargo binstall impeccable           # downloads a prebuilt binary
cargo install --locked impeccable   # or compiles it, in a few seconds
```

Each [release](https://github.com/hexuria/impeccable-rust/releases) also has
an archive per platform, with a SHA-256 file and a build provenance
attestation:

```sh
gh attestation verify impeccable-*.tar.gz --repo hexuria/impeccable-rust
```

Without any of these, the agent builds the copy in the skill folder on first
use, which needs Rust 1.89 or newer and writes only inside that folder:

```sh
cd ~/.claude/skills/impeccable-rust
cargo install --locked --offline --path cli --root .
```

That copy is `bin/impeccable`. After a skill update or an edited pin it says
it is older than the skill; run the same command again.

```sh
impeccable doctor              # where each tool runs: host, toolbox, or nowhere
impeccable run cargo kani      # on the host if Kani is installed, else in the toolbox
impeccable run --toolbox cargo test   # any command, in Linux, on the current project
impeccable sanitize memory     # cargo test under a sanitizer, with the right flags
impeccable bench-guard main -- cargo bench   # check the benchmark contract, then run one at a time
impeccable setup toolbox       # build the Linux image ahead of time (needs Docker)
impeccable setup host          # or install the same pinned tools on this machine
```

### Host or toolbox

`impeccable run` picks a backend for each command:

- **Host.** A pinned tool you already have installed runs directly, and
  `impeccable` prints the version that ran. A tool older than its pin runs in
  the toolbox when Docker is available, and on the host with a warning when it
  is not.
- **Toolbox.** Everything else runs in a Docker image with its tools pinned in
  [`tools.txt`](skill/cli/toolbox/tools.txt) and the
  [`Dockerfile`](skill/cli/toolbox/Dockerfile): Miri, cargo-careful, sanitizers,
  nextest, cargo-mutants, cargo-llvm-cov, cargo-fuzz, Bolero, Kani, gungraun
  with Valgrind, TLC, cargo-semver-checks, cargo-public-api, cargo-deny,
  cargo-audit, cargo-auditable, cargo-vet, and zizmor.

To work without Docker, set `IMPECCABLE_BACKEND=host`. Every run stays on the
host, and a missing tool prints the command that installs it.
`IMPECCABLE_BACKEND=toolbox` does the opposite and sends everything to the
image.

Some tools have no host build on some systems. Valgrind, and so gungraun, runs
only on Linux. MemorySanitizer and standalone LeakSanitizer need Linux too.
Kani has no Windows build. Without Docker these run nowhere on that machine,
`doctor` says so, and the agent reports the check as a gap.

Library crates such as proptest and Loom come in through Cargo, and the provers
(Lean, Verus, Creusot, Aeneas, hax) are not included.

### The toolbox image

The image is several GB, and the first build takes a few minutes. It mounts
your workspace at `/work`. Build output, the Cargo registry, and tool caches
live in Docker volumes that belong to that one project, so host builds stay
untouched and one project's build scripts cannot change what another project
runs. The image tag is a fingerprint of the pins and the CLI, so a changed pin
builds a new image and removes the old one.

A host run has no isolation: build scripts and tests run with your own access.
For code you do not trust, such as a contributor's branch, use the toolbox
with `IMPECCABLE_EPHEMERAL=1`. Its caches are then deleted when the run ends,
so nothing that build wrote reaches a later run.

`GH_TOKEN` or `GITHUB_TOKEN`, when set, is passed to the image build so tool
downloads avoid GitHub's anonymous rate limit. It is mounted only for the step
that downloads prebuilt binaries. Steps that compile code run without it.

gungraun measures cache behavior more precisely with `IMPECCABLE_UNCONFINED=1`,
which lets it turn off address randomization.

### Upgrading from the bash script

Earlier versions shipped `scripts/impeccable`, a bash script. Its Cargo caches
were shared by every project. Remove them, and the old per-project build
volumes, once you have built the new CLI:

```sh
docker volume rm impeccable-cargo-registry impeccable-cargo-git impeccable-cache
docker volume ls -q --filter name=impeccable-target- | xargs docker volume rm
```

## Use it

The agent loads the skill on its own when you work on serious Rust. You can
also name it. Example prompts:

```text
Use impeccable-rust to review the unsafe code in src/arena.rs.
Harden this lock-free queue with impeccable-rust and add Loom tests.
I rewrote the parser for speed. Verify it against the old one.
Make the parser faster with impeccable-rust until the gains converge.
Audit how this workspace is verified and tell me what is missing.
Set up CI for this published crate following impeccable-rust.
```

Every change ends with a report like this:

```text
Evidence:     property-tested parse() vs old_parse(), 10k cases, no divergence
              Miri-checked arena tests; cargo careful on the FFI tests
Documented:   ADR for the arena layout; no From<&str> on purpose
Deferred:     fuzz campaign on parse() moved to nightly CI
Compat/deps:  no new public surface; cargo deny clean
Verification: unsafe / memory behavior; owner of each affected failure mode
```

## Releasing the CLI

Set the new version in `skill/cli/Cargo.toml`, run `cargo generate-lockfile`
there, and merge. Then tag that commit `v<version>` and push the tag. The
release workflow tests and builds every target and attaches the archives to a
GitHub release. It also publishes the crate to crates.io once the crate lists
this workflow as a trusted publisher and the repository variable
`CRATES_IO_TRUSTED_PUBLISHING` is `true`; until then, run `cargo publish` in
`skill/cli` from the tagged commit.

## Examples

- [`examples/claude-code`](examples/claude-code/README.md): Claude Code alone
  on your machine, with or without Docker, and a fresh subagent auditing the
  report.
- [`examples/opengrok`](examples/opengrok/README.md): OpenGrok or Grok Bot
  orchestrates, Claude Code writes with the skill, and a Cursor cloud agent
  reviews the PR against it.
- [`examples/devin`](examples/devin/README.md): Claude Code orchestrates,
  Devin sessions write with the skill on their own Linux machines, and a Devin
  automation gates every PR against it.

## License

MIT
