# SPDX-License-Identifier: Apache-2.0
FROM rust:1.98.0-slim-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends build-essential python3 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY scripts/license-notices.py ./scripts/license-notices.py
COPY third_party/licenses ./third_party/licenses
RUN cargo build --release --locked -p airk-cli \
    && python3 scripts/license-notices.py --output /build/notices

FROM build AS test
RUN cargo test --workspace --locked --offline

FROM debian:bookworm-slim AS runtime
RUN groupadd --gid 10001 airk && useradd --uid 10001 --gid airk --no-create-home airk \
    && mkdir -p /data /workspaces /usr/share/licenses/airk \
    && chown airk:airk /data
COPY --from=build /build/target/release/airk /usr/local/bin/airk
COPY --from=build /build/notices /usr/share/licenses/airk/dependencies
COPY LICENSE NOTICE THIRD_PARTY_NOTICES.md /usr/share/licenses/airk/
USER 10001:10001
WORKDIR /workspaces
ENTRYPOINT ["airk", "--root", "/workspaces", "--data-dir", "/data"]
CMD ["host"]
