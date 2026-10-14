# SPDX-License-Identifier: Apache-2.0
FROM rust:1.98.0-slim-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends build-essential python3 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
ENV CARGO_BUILD_JOBS=2
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY scripts/license-notices.py ./scripts/license-notices.py
COPY third_party/licenses ./third_party/licenses
RUN --mount=type=cache,id=airk-cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=airk-linux-target,target=/build/target \
    cargo build --release --locked -p airk-cli \
    && python3 scripts/license-notices.py --output /build/notices \
    && mkdir -p /build/out && cp /build/target/release/airk /build/out/airk

FROM build AS test
RUN --mount=type=cache,id=airk-cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=airk-linux-target,target=/build/target \
    cargo test --workspace --locked --offline

FROM debian:bookworm-slim AS runtime
RUN groupadd --gid 10001 airk && useradd --uid 10001 --gid airk --no-create-home airk \
    && mkdir -p /data /workspaces /usr/share/licenses/airk \
    && chown airk:airk /data
COPY --from=build /build/out/airk /usr/local/bin/airk
COPY --from=build /build/notices /usr/share/licenses/airk/dependencies
COPY LICENSE NOTICE THIRD_PARTY_NOTICES.md /usr/share/licenses/airk/
USER 10001:10001
WORKDIR /workspaces
ENTRYPOINT ["airk", "--root", "/workspaces", "--data-dir", "/data"]
CMD ["host"]
