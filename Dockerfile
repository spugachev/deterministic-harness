# Deterministic Harness image — the single `dhx:latest` that runs EVERYTHING.
#
# There is no `dhx` on the host and no per-project Dockerfile: you build this
# image once, then run every tier of every project through it, e.g.
#
#   docker build -t dhx:latest .
#   docker run --rm -v "$PWD":/work -w /work dhx:latest dhx init my-svc
#   docker run --rm -v "$PWD":/work -w /work dhx:latest dhx check
#   docker run --rm -v "$PWD":/work -w /work dhx:latest dhx verify --full
#
# The image bakes the `dhx` binary plus every external tool at the versions the
# scaffold pins (.harness/pins/* + rust-toolchain.toml), so the image and any
# project it scaffolds agree by construction. Multi-stage so the heavy toolchain
# layers cache across rebuilds of the (fast-changing) dhx source.

# trixie (Debian 13, glibc 2.41) — bookworm's glibc 2.36 is too old for the
# current Kani release binary (it needs GLIBC_2.39).
FROM debian:trixie-slim AS base
ENV DHX_IN_CONTAINER=1
RUN apt-get update && apt-get install -y --no-install-recommends \
        curl ca-certificates git build-essential pkg-config libssl-dev \
        default-jre-headless \
    && rm -rf /var/lib/apt/lists/*
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# The version authority is the scaffold's pin files + rust-toolchain.toml, so the
# image and every scaffolded project install identical tool versions.
FROM base AS toolchain
WORKDIR /work
COPY harness/assets/scaffold/rust-toolchain.toml ./rust-toolchain.toml
COPY harness/assets/scaffold/dot.harness/pins/ ./pins/
RUN rustup show \
    && rustup toolchain install "$(cat pins/nightly.txt)" \
         --component miri --component rust-src

FROM toolchain AS tools
RUN cargo install --locked cargo-llvm-cov cargo-deny cargo-machete cargo-nextest \
        cargo-mutants cargo-geiger cargo-outdated
RUN cargo install --locked kani-verifier && cargo kani setup
RUN cargo "+$(cat pins/nightly.txt)" install --locked cargo-fuzz
# gitleaks: pinned release binary onto PATH (the secrets-scan gate hard-fails if
# it is absent — a missing scanner is the silently-toothless mode we refuse).
ARG GITLEAKS_VERSION=8.21.2
RUN arch="$(uname -m)"; case "$arch" in aarch64) gl=arm64 ;; x86_64) gl=x64 ;; *) echo "unsupported $arch" >&2; exit 1 ;; esac; \
    curl -sSL -o /tmp/gitleaks.tgz \
      "https://github.com/gitleaks/gitleaks/releases/download/v${GITLEAKS_VERSION}/gitleaks_${GITLEAKS_VERSION}_linux_${gl}.tar.gz" \
    && tar -xzf /tmp/gitleaks.tgz -C /usr/local/bin gitleaks && rm /tmp/gitleaks.tgz \
    && gitleaks version
# TLA+ tools jar → where resolve_tlc_jar looks (/usr/local/lib/tla2tools.jar).
# tla2tools v1.7.4 ships TLC2 "Version 2.19" — the string .harness/pins/tla2tools.txt asserts.
ARG TLA2TOOLS_RELEASE=v1.7.4
RUN curl -sSL -o /usr/local/lib/tla2tools.jar \
      "https://github.com/tlaplus/tlaplus/releases/download/${TLA2TOOLS_RELEASE}/tla2tools.jar" \
    && java -cp /usr/local/lib/tla2tools.jar tlc2.TLC -h 2>&1 | grep -q "Version 2.19" \
    && echo "tla2tools installed (TLC Version 2.19)"

FROM tools AS dhx
# Build the dhx CLI from the in-repo crate (the build context is this repo root).
COPY harness/ /opt/dhx-src/
RUN cargo install --locked --path /opt/dhx-src

# Pre-warm the scaffold's dependency set (item 3). We materialize the embedded
# scaffold to a temp dir and compile its tests, which downloads + builds the
# common deps (axum/tokio/cucumber/proptest/serde/…) into /root/.cargo/registry.
# A FRESH `dhx-cargo-registry` named volume inherits the image's content at its
# mount path, so the very first `dhx check`/`verify` in a new project starts with
# those crates already downloaded — no cold registry fetch. (The per-project
# `dhx-target-*` volume still starts empty, but compiling against pre-downloaded
# deps is the bulk of the win.) The temp project is discarded; only the populated
# cargo registry persists in the layer.
RUN dhx init /tmp/prewarm --name prewarm >/dev/null 2>&1 \
    && cd /tmp/prewarm \
    && cargo fetch 2>/dev/null \
    && DHX_IN_CONTAINER=1 cargo test --workspace --no-run 2>/dev/null \
    ; rm -rf /tmp/prewarm; true
WORKDIR /work
