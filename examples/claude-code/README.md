# impeccable-rust with Claude Code alone

This example uses one agent on your own machine: no orchestrator, no cloud
agent, and no Docker unless you want it. Claude Code writes the change, runs
the verifiers, and has a fresh subagent audit the report before you see it.

## The loop

```text
You --task--> Claude Code
                 |  impeccable doctor        which checks can run here
                 |  writes the change        one failure class at a time
                 |  impeccable run ...       verifiers; long ones in the background
                 v
              fresh subagent                 audits the report against the diff
                 |
                 v
              report --> You                 merge, or decide the Deferred items
```

- **Start.** Claude runs `impeccable doctor` and tells you which checks run on
  this machine and which are gaps.
- **Work.** Claude follows the checklist and runs each verifier through
  `impeccable`. Long runs go to the background, and Claude keeps working.
- **Audit.** A subagent that has not seen Claude's reasoning checks the report.
  With one agent, this is what stands in for a second reviewer.
- **Finish.** Every change ends with the Evidence, Documented, Deferred,
  Compat/deps, and Verification report.

## Setup

### 1. Install the skill

Follow the [Install](../../README.md#install) section of the main README.
Claude Code loads the skill on its own when you work on Rust it applies to.

### 2. Install the CLI

Either command puts `impeccable` on your `PATH`:

```sh
cargo binstall impeccable           # downloads a prebuilt binary
cargo install --locked impeccable   # or compiles it, in a few seconds
```

Skip this and Claude builds the copy in the skill folder on first use.

### 3. Choose where the tools run

Run `impeccable doctor` and read the RUNS column.

- **With Docker.** Change nothing. A pinned tool you have installed runs on
  your machine. Everything else runs in the toolbox, a Linux image that the
  first toolbox run builds. It is several GB.
- **Without Docker.** Set `IMPECCABLE_BACKEND=host` in your shell profile.
  Every run stays on your machine, and a missing tool prints the command that
  installs it. `impeccable setup host` installs the whole pinned set at once;
  it needs cargo-binstall and adds a nightly toolchain.

On macOS without Docker, gungraun, MemorySanitizer, and LeakSanitizer run
nowhere. Claude reports them as gaps. AddressSanitizer and ThreadSanitizer
still run locally, and a Linux CI job can cover the rest.

### 4. Let Claude run the checks without asking

In Claude Code's default mode, each command asks for permission, and a
session left alone stops at the first prompt. Allow the ones you are
comfortable with in `.claude/settings.local.json` in your project, which is
personal and not committed:

```json
{
  "permissions": {
    "allow": [
      "Bash(impeccable doctor)",
      "Bash(impeccable sanitize:*)",
      "Bash(impeccable bench-guard:*)",
      "Bash(impeccable run cargo:*)"
    ]
  }
}
```

`impeccable run cargo:*` is as broad as allowing `cargo` itself, because cargo
runs the project's own code. Leave out a bare `impeccable run:*`: it would
allow any command at all.

## Standing prompt

Paste this at the start of a session:

```text
Use impeccable-rust for every Rust change in this session.

1. Start with `impeccable doctor`. Tell me which checks can run here and
   which are gaps, before you choose any.
2. Run long checks as background commands and keep working: mutation
   testing, fuzzing, Kani, and Miri over the full suite. Report each one when
   it finishes.
3. Before you call a change done, give a fresh subagent the diff and your
   report, without your reasoning. Ask it to find every Evidence claim that
   no check on this diff backs, and any bounded check described as a proof.
   Fix what it finds, or move the claim to Deferred.
4. End with the report. Ask me before you drop a Deferred item, install a
   tool on this machine, or do anything irreversible.
```

## Prompts for one task

You can also name the skill for a single task:

```text
Use impeccable-rust to review the unsafe code in src/arena.rs.
Harden this lock-free queue with impeccable-rust and add Loom tests.
I rewrote the parser for speed. Verify it against the old one.
Make the parser faster with impeccable-rust until the gains converge.
Audit how this crate is verified and tell me what is missing.
```

For the speed prompt, Claude freezes a baseline commit and runs every
benchmark through `impeccable bench-guard <baseline> -- cargo bench`. The run
fails if the benchmark files, build settings, or compiler flags moved, and a
lock keeps two benchmarks from running at once on your machine.

## What you get back

```text
Evidence:     property-tested parse() vs old_parse(), 10k cases, no divergence
              Miri-checked arena tests (miri on the host)
              cargo-mutants 27.1.0: 41 caught, 0 missed in src/arena.rs
Documented:   ADR for the arena layout; no From<&str> on purpose
Deferred:     MemorySanitizer: runs nowhere on this Mac without Docker
Compat/deps:  no new public surface; cargo deny clean
Verification: unsafe / memory behavior; owner of each affected failure mode
```

A check that could not run appears under Deferred with its reason. It is
never left out.

## Limits

- **The audit is the same model.** A fresh subagent has not seen the reasoning,
  but it shares the writer's blind spots. For high-stakes code, add a second
  agent or a person. The [OpenGrok](../opengrok/README.md) and
  [Devin](../devin/README.md) examples show two ways.
- **Host runs are not isolated.** Build scripts and tests run with your own
  access. For code you do not trust, use the toolbox with
  `IMPECCABLE_EPHEMERAL=1`.
- **Long runs use your machine.** Mutation testing and fuzzing can take hours
  and compete with your own builds. Run them overnight, or send them to CI.
