# seats — Deterministic Harness image. Built FROM the project's own pins
# (.harness/pins/*) so in-container tool versions match exactly (pins are the
# single version authority). `dhx verify --full` runs here.
#
# dhx is built INSIDE the image from its own source tree, supplied as a named
# build context `dhxsrc` (no second copy committed to this repo, no registry):
#
#   docker build -t seats-harness:latest \
#       --build-context dhxsrc=/path/to/deterministic-harness/dhx .
#   docker run --rm -v "$PWD":/work:ro -w /work seats-harness:latest dhx verify --full
#
# (Once dhx is published, the dhx stage can instead `cargo install dhx --version
# "$(cat .harness/pins/dhx.txt)"` and drop the build context.)
#
# Multi-stage so toolchain layers cache across rebuilds.

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

FROM base AS toolchain
WORKDIR /work
COPY rust-toolchain.toml ./
COPY .harness/pins/ ./.harness/pins/
RUN rustup show \
    && rustup toolchain install "$(cat .harness/pins/nightly.txt)" \
         --component miri --component rust-src
# (external tools — gitleaks, the TLA+ jar — are installed in the `tools` stage
# below, alongside the cargo-installed binaries.)

FROM toolchain AS tools
RUN cargo install --locked cargo-llvm-cov cargo-deny cargo-machete cargo-nextest \
        cargo-mutants cargo-geiger cargo-outdated
RUN cargo install --locked kani-verifier && cargo kani setup
RUN cargo "+$(cat .harness/pins/nightly.txt)" install --locked cargo-fuzz
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
# Build dhx from its own source tree, passed as the `dhxsrc` build context — one
# source of truth (the real dhx checkout), copied only into the image at build
# time, never duplicated into this project's repo.
COPY --from=dhxsrc . /dhx-src
RUN cargo install --locked --path /dhx-src
WORKDIR /work
