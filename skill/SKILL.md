---
name: impeccable-rust
description: >-
  Use when writing, reviewing, hardening, or designing Rust for high-stakes or
  long-lived crates. Checklist for exhaustive testing, trustworthy benchmarks,
  misuse-resistant APIs, decision records, semver hygiene, and deliberate
  dependency maintenance.
---

# Impeccable Rust

Faults may still happen outside the code (spec, hardware, ops). The
implementation should not be what is to blame. There is no single trick. Heavy
tools cost time and compute; spend a deliberate risk budget where failure
hurts.

## Operating rules

1. Prefer making incorrect use inexpressible over documenting "do not do that."
2. Treat "it works" as insufficient. Prove it is not broken under chaos and edge cases.
3. Prefer automation that catches human misses (Miri, Loom, Kani, semver checks, cargo-vet).
4. When you accept a downside or skip a corner case, write it down.
5. Stagnation is a choice with rising cost. Surface it; do not silently defer forever.
6. Apply expensive verification where failure actually hurts.

## Checklist (run what applies)

Work through each section that fits the change. Skip sections that clearly do
not apply (for example, no concurrency means skip Loom). Say what you ran and
what you deliberately skipped.

### 1. Testing

- Assert invariants. Panics on broken assumptions beat silent wrongness.
- It works is not the same as it is not broken.
- Run Miri on tests that touch `unsafe`, custom allocators, or subtle provenance (`cargo +nightly miri test`).
- Use sanitizers (ASan, TSan, and so on) when threading or memory access patterns matter beyond Miri.
- Test error paths, not only the happy path.
- Error litmus test: temporarily replace `return Err(...)` with `continue` (or otherwise skip the error return). If the suite still passes, error-path coverage is broken. Tests must trigger and verify the exact `Err`.

### 2. Chaos

Add at least one chaos layer that fits:

| Kind | Tools |
|------|--------|
| Async / sync scheduling | `turmoil`, `shuttle` |
| Value | `quickcheck`, `proptest` |
| Logic | `cargo-mutants` |

For reimplementations (custom map, codec, parser), property-test against a trusted oracle (for example, std) and assert broad invariants such as "never panics."

### 3. Exhaustive verification

- Loom for all distinguishable concurrent executions of lock-free, atomic, or custom sync code.
- Kani for symbolic / model-checked inputs around `unsafe` and high-risk logic.

Keep exhaustive tools on the smallest core that must be correct.

### 4. Benchmarks

Cover the full performance profile:

- Pathological cases
- Micro and end-to-end
- Under, at, and over capacity
- All relevant targets

Trustworthy measurements (CI should fail on regression):

- Prefer instruction-count / callgrind-style metrics over wall time alone (for example, `iai-callgrind`).
- Interleave old and new (for example, `tango`) to cut noise.
- Use a dedicated host and leave headroom (under 100% load).

Measure what matters, not only speed:

- Throughput and goodput (a flood of 500 responses can show high throughput with zero useful work)
- Memory (average and max)
- Latency distributions, not only the mean
- Outcomes: realistic inputs, measured outputs, compare to ground truth
- Prefer the real deployment target, not only a beefy CI box

Record how you load the system (open, closed, partly-open), which statistic you report (mean, median, histogram, CDF), and how you decide a regression. "y is greater than x" is not enough.

### 5. Documentation

Decisions:

- Alternatives discarded and why
- Downsides accepted and why
- Short ADRs, YADRs, or design notes

What is not there:

- Missing corner-case handling. Tell callers what the code cannot do. A silent `todo!()` is a landmine.
- Known future optimizations
- Deliberate absence of impls (for example, no `From` for a reason)

### 6. Misuse resistance

Make misuse inexpressible:

- Newtypes, not type aliases (`Meters(u64)` vs `Miles(u64)`)
- Typestates (`Rocket<Ground>` vs `Rocket<Air>`)
- Two-phase structs (raw `TomlConfig` vs validated `ResolvedConfig`)
- Enums over booleans, especially multi-bool parameter lists
- Enums for linked arguments (encode linked `bool` + `Option` pairs as one type so conflicting states cannot be constructed)

Idioms:

- Clippy in CI (deny or warn meaningfully)
- Rust API Guidelines
- If an API smells like OOP factories and inheritance trees, redesign toward builders, ownership-honest APIs, and few trait-supertrait pyramids

### 7. Compatibility

Public surface area is a liability. Prefer:

- `-> impl Trait` over naming concrete return types you may want to change
- Private fields with accessors or builders instead of `pub` fields
- No leaking public dependencies in args, returns, trait impls, or re-exports
- Non-pub inherent methods over blanket `impl From` / always-public trait impls when the coupling is accidental

Automate:

- `cargo-semver-checks`
- `cargo-public-api`

Keep a simple, stable core. Document semver expectations for callers.

### 8. Dependencies

1. Track the complete dependency closure across every deployment that matters.
2. Join against known issues (for example, RUSTSEC).
3. Vet for unknown issues (`cargo-vet`, public or internal).

Be able to answer operational questions such as which deployed units still run a vulnerable transitive crate.

### 9. Stagnation as a choice

Loud reminders when you are behind or dependencies are dead (Dependabot / Renovate). Reduce friction:

- Auto-merge dependency bump PRs that pass tests
- Budgeted maintenance time
- Prefer upstreaming over long-lived forks
- Wrap unstable dependencies behind a stable internal facade
- Treat rustc / edition lag the same as crate lag

Cost rises with every skipped upgrade cycle.

## CI shape

Adapt to the crate. Minimum credible set:

1. `cargo test` + Clippy + rustfmt
2. Miri for `unsafe` / allocator / concurrency-sensitive tests
3. At least one of: proptest/quickcheck, mutants, or fuzz on parsers / codecs
4. Loom and/or Kani gated to the modules that need them
5. Benchmark regression gate with non-noisy metrics
6. `cargo deny` / RUSTSEC audit + optional `cargo-vet`
7. `cargo-semver-checks` on published API crates

## Review report

When finishing work under this skill, report:

- Proven: verification run, or type-level misuse made impossible
- Documented: decisions and intentional gaps written down
- Deferred: what was skipped and why (follow-up if high stakes)
- Compat / deps: any new public surface or dependency hazard

## Anti-patterns

- Happy-path-only tests
- Wall-clock microbenchmarks on a shared machine as the sole perf signal
- `pub` everything, boolean soup, type aliases for distinct units
- Leaking hyper / serde / tokio types into a stable public API without intent
- Silent TODO debt and forever-pinned dependency versions with no reminder
- Claiming this quality bar without Miri, property tests, misuse-resistant types, or decision docs
