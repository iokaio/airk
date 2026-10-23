# SPDX-License-Identifier: Apache-2.0
# Local Linux transport validation; no models, VSIX packaging, or image publication.
FROM rust:1.98.0-slim-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends build-essential ca-certificates git \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
RUN useradd --create-home --uid 10001 airktest \
    && chown -R airktest:airktest /build /usr/local/cargo
USER airktest
ENV CARGO_BUILD_JOBS=2
RUN --mount=type=cache,id=airksvc-light-target,target=/build/target,uid=10001,gid=10001 \
    --mount=type=cache,id=airksvc-light-registry,target=/usr/local/cargo/registry,uid=10001,gid=10001 \
    --mount=type=cache,id=airksvc-light-git,target=/usr/local/cargo/git,uid=10001,gid=10001 \
    cargo test -p airk-cli --no-default-features --lib --locked
RUN --mount=type=cache,id=airksvc-light-target,target=/build/target,uid=10001,gid=10001 \
    --mount=type=cache,id=airksvc-light-registry,target=/usr/local/cargo/registry,uid=10001,gid=10001 \
    --mount=type=cache,id=airksvc-light-git,target=/usr/local/cargo/git,uid=10001,gid=10001 \
    cargo build -p airk-cli --no-default-features --bin airk --locked \
    && ls -l target/debug/airk \
    && cargo test -p airk-cli --no-default-features --test service_runtime --locked \
    && cargo test -p airk-store --lib service:: --locked \
    && cargo test -p airk-store --lib publication_spike_tests --locked \
    && cargo test -p airk-store --lib wal_budget_spike --locked \
    && cargo test -p airk-store --lib service_publication --locked \
    && cargo test -p airk-store --lib service_namespace_cache --locked
RUN --mount=type=cache,id=airksvc-light-target,target=/build/target,uid=10001,gid=10001 \
    --mount=type=cache,id=airksvc-light-registry,target=/usr/local/cargo/registry,uid=10001,gid=10001 \
    --mount=type=cache,id=airksvc-light-git,target=/usr/local/cargo/git,uid=10001,gid=10001 \
    cargo test -p airk-kernel --no-default-features --lib service_publication --locked \
    && cargo test -p airk-kernel --no-default-features --lib service_resource --locked
