# syntax=docker/dockerfile:1
# Linux toolbox for the impeccable-rust skill: its command-line tools, pinned.
# Build and use it through the impeccable CLI, the crate this folder belongs to
# (`setup toolbox`, then `run <command>`). The CLI embeds this folder and its own source,
# and tags the image with their fingerprint, so editing a pin and rebuilding the CLI
# yields a new image. The build context is the crate root.
ARG RUST_IMAGE=rust:1.98.1-trixie
FROM ${RUST_IMAGE}

ARG NIGHTLY=nightly-2026-09-25
ARG BINSTALL_VERSION=1.24.0
ARG TLA_VERSION=1.7.4
ARG TLA_SHA256=936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88

# Valgrind runs gungraun. The JRE runs TLC.
RUN apt-get update \
 && apt-get install -y --no-install-recommends valgrind default-jre-headless python3 \
 && rm -rf /var/lib/apt/lists/*

# A pinned nightly for Miri, cargo-careful, sanitizers, and branch coverage, installed
# under the name `nightly` so `cargo +nightly ...` works unchanged. rustup refuses to link
# a custom toolchain as `nightly`, so the dated toolchain is renamed. Auto-install stays
# off, so a missing toolchain fails instead of silently downloading the latest one.
ENV RUSTUP_AUTO_INSTALL=0
RUN triple="$(rustc -vV | sed -n 's/^host: //p')" \
 && rustup component add llvm-tools-preview \
 && rustup toolchain install "${NIGHTLY}" --profile minimal --component miri,rust-src,llvm-tools-preview \
 && mv "${RUSTUP_HOME}/toolchains/${NIGHTLY}-${triple}" "${RUSTUP_HOME}/toolchains/nightly-${triple}" \
 && if [ -f "${RUSTUP_HOME}/update-hashes/${NIGHTLY}-${triple}" ]; then \
      mv "${RUSTUP_HOME}/update-hashes/${NIGHTLY}-${triple}" "${RUSTUP_HOME}/update-hashes/nightly-${triple}"; fi

RUN curl -fsSL "https://github.com/cargo-bins/cargo-binstall/releases/download/v${BINSTALL_VERSION}/cargo-binstall-$(uname -m)-unknown-linux-musl.tgz" \
  | tar -xz -C "${CARGO_HOME}/bin"

# Anonymous GitHub API calls are rate limited, so `impeccable setup toolbox` passes GH_TOKEN
# or GITHUB_TOKEN as a build secret when one is set. The token exists only in this step, and
# this step only downloads prebuilt binaries: with the compile strategy off, no build script
# runs while the token is readable. A tool with no prebuilt binary is left for the next step.
COPY toolbox/tools.txt /usr/local/share/impeccable/tools.txt
RUN --mount=type=secret,id=github_token,env=GITHUB_TOKEN,required=false \
    grep -v -E '^[[:space:]]*(#|$)' /usr/local/share/impeccable/tools.txt \
  | awk '$1 != "kani-verifier" { print $1 "@" $2 }' \
  | while read -r spec; do \
      cargo binstall -y --disable-telemetry --locked --disable-strategies compile \
        --maximum-resolution-timeout 120 "${spec}" \
      || echo "impeccable: no prebuilt binary for ${spec}; the next step builds it" >&2; \
    done

# No secret in this step. binstall skips what the step above installed and compiles the
# rest; Kani always compiles. Their build scripts run with no token in reach.
RUN grep -v -E '^[[:space:]]*(#|$)' /usr/local/share/impeccable/tools.txt \
  | awk '$1 != "kani-verifier" { print $1 "@" $2 }' \
  | xargs cargo binstall -y --disable-telemetry --locked --maximum-resolution-timeout 120 \
 && kani="$(awk '$1 == "kani-verifier" { print $2 }' /usr/local/share/impeccable/tools.txt)" \
 && cargo install --locked kani-verifier --version "${kani}" \
 && cargo kani setup \
 && rm -rf "${CARGO_HOME}/registry" "${CARGO_HOME}/git"

RUN curl -fsSL -o /opt/tla2tools.jar "https://github.com/tlaplus/tlaplus/releases/download/v${TLA_VERSION}/tla2tools.jar" \
 && echo "${TLA_SHA256}  /opt/tla2tools.jar" | sha256sum -c - \
 && printf '#!/bin/sh\nexec java -XX:+UseParallelGC -cp /opt/tla2tools.jar tlc2.TLC "$@"\n' > /usr/local/bin/tlc \
 && chmod +x /usr/local/bin/tlc

# The CLI, built from the same source as the one on the host. It has no dependencies, so it
# builds offline.
COPY --chmod=755 toolbox/entrypoint /usr/local/bin/impeccable-entrypoint
COPY . /usr/local/src/impeccable
RUN cargo install --locked --offline --path /usr/local/src/impeccable \
 && rm -rf /usr/local/src/impeccable/target

# Docker's default seccomp profile blocks the call gungraun uses to turn off ASLR, so the
# toolbox allows ASLR. `IMPECCABLE_UNCONFINED=1 impeccable run ...` restores gungraun's default.
# Miri and cargo-careful build their sysroots on first use into the /cache volume, which
# persists across runs; a sysroot baked into an image layer cannot be replaced in place.
ENV IMPECCABLE_IN_TOOLBOX=1 \
    CARGO_TARGET_DIR=/target \
    XDG_CACHE_HOME=/cache/xdg \
    GUNGRAUN_ALLOW_ASLR=true
WORKDIR /work
ENTRYPOINT ["impeccable-entrypoint"]
CMD ["bash"]
