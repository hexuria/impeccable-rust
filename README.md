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
- **Risk-to-owner routing.** Each failure class gets one owner: fuzzing for
  untrusted input, Miri and cargo-careful for `unsafe`, Loom for lock-free code,
  TLA+ or Stateright for multi-actor designs, Creusot, Verus, or Lean for proof
  kernels, cargo-auditable and zizmor for supply chain and CI.
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

## Use it

The agent loads the skill on its own when you work on serious Rust. You can
also name it. Example prompts:

```text
Use impeccable-rust to review the unsafe code in src/arena.rs.
Harden this lock-free queue with impeccable-rust and add Loom tests.
I rewrote the parser for speed. Verify it against the old one.
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

## License

MIT
