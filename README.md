# impeccable-rust

Agent skill for writing and reviewing Rust the way Jon Gjengset describes in
[*Towards Impeccable Rust*](https://youtu.be/qfknfCsICUM) (Rust Nation UK 2024).

**Impeccable** = free from fault or blame. The implementation should not be why
critical software fails. There is no single trick — layered testing, misuse-resistant
APIs, trustworthy benchmarks, documented decisions, and deliberate dependency hygiene.

The checklist follows the talk. The data-layout notes, everyday API idioms, and
the verification architecture (TLA+, Lean, conformance, anti-drift) go beyond it
and are not claims from the talk.

## Install

Copy [`skill/SKILL.md`](skill/SKILL.md) to `impeccable-rust/SKILL.md` in your
agent skills folder (Cursor / Claude Code / Codex / Grok Bot workflows), for
example `~/.claude/skills/impeccable-rust/SKILL.md`. To track this repo
instead, clone it and link `skill/` under that name:

```sh
ln -s "$PWD/skill" ~/.claude/skills/impeccable-rust
```

The folder name must match the skill's `name` (`impeccable-rust`). Strict
loaders such as `skills-ref validate` reject a folder named `skill`.

## Sources

- Video: https://youtu.be/qfknfCsICUM
- Slides: https://jon.thesquareplanet.com/slides/towards-impeccable-rust/
- PDF: https://jon.thesquareplanet.com/slides/towards-impeccable-rust/export.pdf

## License

MIT — the checklist is derived from a public talk; credit Jon Gjengset / Rust Nation UK.
