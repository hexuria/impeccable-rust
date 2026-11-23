# impeccable

The command-line companion of the
[impeccable-rust](https://github.com/hexuria/impeccable-rust) agent skill. It
sets up, checks, and runs the verification tools the skill names: Miri,
sanitizers, Kani, cargo-mutants, nextest, cargo-fuzz, gungraun, cargo-deny, and
the rest of the pinned list in [`toolbox/tools.txt`](toolbox/tools.txt).

It has no dependencies and works on Linux, macOS, and Windows.

## Install

```sh
cargo binstall impeccable        # downloads a prebuilt binary
cargo install --locked impeccable   # or compiles it, in a few seconds
```

Prebuilt archives are also attached to each
[release](https://github.com/hexuria/impeccable-rust/releases), with a SHA-256
file and a build provenance attestation:

```sh
gh attestation verify impeccable-*.tar.gz --repo hexuria/impeccable-rust
```

## Use

```sh
impeccable doctor              # where each tool runs: host, toolbox, or nowhere
impeccable run cargo kani      # on the host if Kani is installed, else in the toolbox
impeccable run --toolbox cargo test   # any command, in Linux, on the current project
impeccable sanitize memory     # cargo test under a sanitizer, with the right flags
impeccable bench-guard main -- cargo bench   # check the benchmark contract, then run one at a time
impeccable setup toolbox       # build the Linux image ahead of time (needs Docker)
impeccable setup host          # or install the same pinned tools on this machine
```

`impeccable run` sends a pinned tool to the host when the host has it, and to
the toolbox, a Docker image with every tool pinned, otherwise. Set
`IMPECCABLE_BACKEND=host` to work without Docker. `impeccable help` lists every
option.

The [project README](https://github.com/hexuria/impeccable-rust#tooling) covers
the backends, the toolbox image, and running untrusted code.

## License

MIT
