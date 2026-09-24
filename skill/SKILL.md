---
name: impeccable-rust
description: >-
  Use when writing, reviewing, hardening, or designing Rust for high-stakes or
  long-lived crates — libraries, services, or systems where faults must not be
  the software's fault. Applies Gjengset's "Towards Impeccable Rust" practices:
  exhaustive testing, trustworthy benchmarks, misuse-resistant APIs, documented
  decisions, semver hygiene, and deliberate dependency maintenance.
---

# Impeccable Rust

Grounded in Jon Gjengset's Rust Nation UK 2024 talk *Towards Impeccable Rust*
([video](https://youtu.be/qfknfCsICUM), [slides](https://jon.thesquareplanet.com/slides/towards-impeccable-rust/)).

**Impeccable** means *free from fault or blame*. The software can still fail
(bad spec, bad hardware), but the *implementation* should not be to blame.
Perfection is impossible; strive to get close. There is **no single trick**.

Use this skill when the user asks for impeccable / production-grade / critical
Rust quality, or whenever you are designing or reviewing Rust that must stay
dependable under misuse, load, and time.

## Operating rules

1. Prefer making incorrect use **inexpressible** over documenting "don't do that."
2. Treat "it works" as insufficient — prove it is not broken under chaos and edge cases.
3. Prefer **automation that catches human misses** (Miri, Loom, Kani, semver checks, cargo-vet) over hope.
4. When you accept a downside or skip a corner case, **write it down** so knowledge is not lost.
5. Stagnation is a choice with rising cost — surface it; do not silently defer forever.
6. These practices are expensive (time, money, compute). Spend a deliberate **risk budget** — apply the heavy tools where failure actually hurts.

## Checklist (run what applies)

Work through each section that fits the change. Skip sections that clearly do
not apply (e.g. no concurrency → skip Loom). Say what you ran and what you
deliberately skipped.

### 1. Testing — be paranoid

- Assert invariants liberally; panics on broken assumptions beat silent wrongness.
- Remember: **it works is not the same as it's not broken**.
- Run **Miri** on tests that touch `unsafe`, custom allocators, or subtle provenance (`cargo +nightly miri test`). Miri catches UB and leaks even when nothing panics.
- Use **sanitizers** (ASan / TSan / etc.) when threading or memory access patterns matter beyond what Miri covers.
- Add tests for **error paths**, not only the happy path.
- **Error litmus test:** temporarily replace `return Err(...)` with `continue` (or otherwise skip the error return). If the suite still passes, error-path coverage is broken. Tests must trigger and verify the exact `Err`.

### 2. Embrace chaos

Anything that can go wrong will, at the worst time. Add at least one chaos layer that fits:

| Kind | Tools |
|------|--------|
| Async / sync scheduling chaos | `turmoil`, `shuttle` |
| Value chaos | `quickcheck`, `proptest` |
| Logic chaos | `cargo-mutants` |

For reimplementations (custom map, codec, parser), property-test against a trusted oracle (e.g. std) and/or assert broad invariants such as "never panics."

### 3. Be exhaustive (where you can)

- **Loom** — all distinguishable concurrent executions for lock-free / atomic / custom sync code.
- **Kani** — all distinguishable inputs (symbolic / model checking), especially around `unsafe` and high-risk logic.

Prefer exhaustive tools on the *smallest core* that must be correct; keep the surface area model-checkable.

### 4. Benchmarks — know thyself

Benchmarks must capture the **entire** performance profile, not a flattering loop:

- Pathological cases
- Micro **and** macro / end-to-end
- Under, at, and **over** capacity
- All relevant targets

**Trustworthy measurements** (let CI fail on regression):

- Prefer instruction-count / callgrind-style metrics over wall time alone (e.g. `iai-callgrind`).
- Interleave old and new (e.g. `tango`) to reduce noise.
- Minimize noise: dedicated host, leave headroom (<100% load).

**Measure all that matters**, not just speed:

- Throughput and *goodput* (a flood of 500 responses can look like high throughput but zero useful work)
- Memory (average and max)
- Latency distributions (not only mean)
- Outcomes: simulate realistic inputs → measure outputs → compare to ground truth
- Prefer the **real deployment target** (e.g. a Pi or fleet SKU), not only a beefy CI box

**Simple benchmarks lie.** Record how you load the system (open / closed / partly-open), what statistic you report (mean, median, histogram, CDF), and how you decide a regression (y > x is not enough).

### 5. Documentation — preserve knowledge

**Document decisions taken** (bus-factor insurance)

- Alternatives discarded and why (so the next engineer does not retry them)
- Downsides explicitly accepted and why
- Prefer short ADRs / YADRs / design notes — any durable record beats none

**Document what's not there**

- Missing corner-case handling — tell callers what the code cannot do; a silent `todo!()` is a landmine
- Known future optimizations
- Deliberate absence of impls (e.g. no `From` for a reason)

### 6. Misuse resistance — "you're holding it wrong" is unacceptable

Make misuse inexpressible:

- **Newtypes**, not type aliases (`Meters(u64)` vs `Miles(u64)`)
- **Typestates** (`Rocket<Ground>` vs `Rocket<Air>`)
- **Two-phase structs** (e.g. raw `TomlConfig` vs validated `ResolvedConfig`)
- **Enums over booleans** (especially multi-bool parameter lists)
- **Enums for linked arguments** (encode linked `bool` + `Option` pairs as one type so conflicting states are impossible)

Follow idioms so surprise does not become misuse:

- Clippy (deny or warn meaningfully in CI)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- If an API smells like OOP / Java / Python / C factories and inheritance trees, redesign toward Rust idioms (builders, ownership-honest APIs, few trait-supertrait pyramids)

### 7. Compatibility — minimize hazards

Public surface area is a liability. Prefer:

- `-> impl Trait` over naming concrete return types you may want to change
- Private fields; expose accessors or builders instead of `pub` fields
- Avoid leaking **public dependencies** in args, returns, trait impls, and re-exports (upgrading them later becomes a breaking change)
- Prefer **non-pub inherent methods** over blanket `impl From` / other always-public trait impls when the coupling is accidental

Automate what humans miss:

- `cargo-semver-checks`
- `cargo-public-api` (high-level API diff awareness)

Educate callers on semver expectations; keep a **simple, stable core**.

### 8. Dependencies — knowledge is everything

Healthy skepticism, then:

1. **Track** the complete dependency closure across every deployment / device that matters
2. **Join** against known issues (e.g. RUSTSEC)
3. **Vet** for unknown issues (`cargo-vet`, public or internal)

Ask operational questions you can actually answer, e.g. which deployed units still run a vulnerable transitive crate.

### 9. Make stagnation a recurrent choice

Have loud reminders when you are behind or dependencies are dead (Dependabot / Renovate). Reduce friction to catch up:

- Auto-merging dependency bump PRs (with tests)
- Budgeted maintenance time
- Prefer upstreaming over long-lived forks
- Wrap unstable dependencies behind a stable internal façade
- Treat **rustc / edition** lag the same way as crate lag

Cost rises with every skipped upgrade cycle.

## Suggested CI shape (adapt to the crate)

Minimum credible set for a serious crate:

1. `cargo test` + Clippy + rustfmt
2. Miri job for `unsafe` / allocator / concurrency-sensitive tests
3. At least one of: proptest/quickcheck, mutants, or fuzz on parsers / codecs
4. Loom and/or Kani jobs gated to the modules that need them
5. Benchmark regression gate with non-noisy metrics
6. `cargo deny` / RUSTSEC audit + optional `cargo-vet`
7. `cargo-semver-checks` on published API crates

## Review / PR comment style

When reviewing or finishing work under this skill, report briefly:

- **Proven:** what verification ran or what type-level misuse was made impossible
- **Documented:** decisions and intentional gaps written down
- **Deferred:** what was skipped and why (with a follow-up if high stakes)
- **Compat / deps:** any new public surface or dependency hazard introduced

## Anti-patterns

- Happy-path-only tests
- "It passed on my machine" timing microbenchmarks as the sole perf signal
- `pub` everything / boolean soup / type aliases for distinct units
- Leaking hyper/serde_/tokio types (etc.) into a stable public API without intent
- Silent TODO debt and forever-pinned dependency versions with no reminder
- Claiming impeccable quality without any of: Miri, property tests, misuse-resistant types, or decision docs

## Sources

- Talk: https://youtu.be/qfknfCsICUM
- Slides: https://jon.thesquareplanet.com/slides/towards-impeccable-rust/
- PDF: https://jon.thesquareplanet.com/slides/towards-impeccable-rust/export.pdf
