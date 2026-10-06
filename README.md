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

The folder name must be `impeccable-rust`, matching the skill's `name`. Strict
loaders reject a folder named `skill`. For other agents, put the same folder
wherever they load skills from, for example `.claude/skills/` or
`.cursor/skills/` inside a project.

## Toolbox

The skill ships `scripts/impeccable`, which sets up and runs the command-line
tools the skill names. The agent runs `impeccable doctor` when it starts
verification work, and runs anything the host lacks in a pinned Linux toolbox.

```sh
alias impeccable=~/.claude/skills/impeccable-rust/scripts/impeccable

impeccable doctor              # what runs here, in the toolbox, or nowhere
impeccable setup toolbox       # build the Linux image once (needs Docker)
impeccable run cargo kani      # any command, in Linux, on the current project
impeccable sanitize memory     # cargo test under a sanitizer, with the right flags
impeccable bench-guard main -- cargo bench   # check the benchmark contract, then run one at a time
impeccable setup host          # or install the same pinned tools on this machine
```

The toolbox is a Docker image with its tools pinned in
[`skill/toolbox/tools.txt`](skill/toolbox/tools.txt) and the
[`Dockerfile`](skill/toolbox/Dockerfile): Miri, cargo-careful,
sanitizers, nextest, cargo-mutants, cargo-llvm-cov, cargo-fuzz, Bolero, Kani,
gungraun with Valgrind, TLC, cargo-semver-checks, cargo-public-api, cargo-deny,
cargo-audit, cargo-auditable, cargo-vet, and zizmor. The image is several GB,
and the first build takes a few minutes. It mounts your workspace at `/work` and
keeps build output in a Docker volume, so host builds stay untouched. Editing a
pin rebuilds the image on the next run and removes the superseded one. Library
crates such as proptest and Loom come in through Cargo, and the provers (Lean,
Verus, Creusot, Aeneas, hax) are not included.

On macOS the toolbox runs what the host cannot: gungraun, MemorySanitizer, and
standalone LeakSanitizer. gungraun measures cache behavior more precisely with
`IMPECCABLE_UNCONFINED=1`, which lets it turn off address randomization.

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

## Examples

- [`examples/opengrok`](examples/opengrok/README.md): OpenGrok or Grok Bot
  orchestrates, Claude Code writes with the skill, and a Cursor cloud agent
  reviews the PR against it.

## License

MIT
