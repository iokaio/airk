# SPDX-License-Identifier: Apache-2.0
FROM rust:1.98.0-slim-bookworm AS toolchain
RUN apt-get update && apt-get install -y --no-install-recommends build-essential python3 curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
ENV CARGO_BUILD_JOBS=2
ENV CARGO_INCREMENTAL=0
# Verify the pinned test tool for the build host; CI qualifies Linux x64.
RUN case "$(uname -m)" in \
      x86_64) nextest_arch=x86_64; nextest_sha=223ba936714cc861a9cdedad5a8eb664dfdd69d6fbdf6cea6aa8184f523290dc ;; \
      aarch64) nextest_arch=aarch64; nextest_sha=81469eb963ef883fd590e60189e77d6aba3d91b418591d3391f50fe3e9480d2b ;; \
      *) exit 1 ;; esac \
    && curl --fail --location --retry 3 -o /tmp/nextest.tar.gz \
      "https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-0.9.148/cargo-nextest-0.9.148-${nextest_arch}-unknown-linux-gnu.tar.gz" \
    && echo "${nextest_sha}  /tmp/nextest.tar.gz" | sha256sum -c - \
    && tar -xzf /tmp/nextest.tar.gz -C /usr/local/cargo/bin cargo-nextest \
    && rm /tmp/nextest.tar.gz
FROM toolchain AS source
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY .config/nextest.toml ./.config/nextest.toml
COPY scripts/ci-metrics.py scripts/ci-prune-target.py ./scripts/
COPY scripts/license-notices.py scripts/provision-local-model.py ./scripts/
COPY scripts/provision-onnx-runtime.py scripts/int8-policy.py scripts/macho-identity.py ./scripts/
COPY scripts/build-service-runtime.py scripts/build-service-container.py ./scripts/
COPY scripts/container-build-user.sh ./scripts/
COPY LICENSE NOTICE THIRD_PARTY_NOTICES.md ./
COPY third_party/models ./third_party/models
RUN python3 scripts/provision-local-model.py --destination /build/model-assets --download
ENV AIRK_BUNDLED_MODEL_DIR=/build/model-assets
ENV AIRK_BUNDLED_RUNTIME_DIR=/build/runtime-assets
RUN python3 scripts/provision-onnx-runtime.py --download
COPY third_party/licenses ./third_party/licenses

FROM source AS test
ARG AIRK_QUALIFICATION=false
RUN rustup show active-toolchain \
    && useradd --create-home --uid 10001 airkbuild \
    && chown -R airkbuild:airkbuild /build /usr/local/cargo /usr/local/rustup
RUN --mount=type=cache,id=airk-cargo-registry,target=/usr/local/cargo/registry,uid=10001,gid=10001,sharing=locked \
    --mount=type=cache,id=airk-cargo-git,target=/usr/local/cargo/git,uid=10001,gid=10001,sharing=locked \
    --mount=type=cache,id=airk-linux-target,target=/build/target,uid=10001,gid=10001,sharing=locked \
    sh scripts/container-build-user.sh sh -ec '\
    cargo fetch --locked \
    && mkdir -p /build/ci-report \
    && case "$AIRK_QUALIFICATION" in \
      true) python3 scripts/ci-metrics.py run --name test-compile -- cargo nextest run --profile ci --workspace --all-features --locked --offline --no-run \
        && python3 scripts/ci-metrics.py run --name test-execution -- cargo nextest run --profile ci --workspace --all-features --locked --offline --ignore-default-filter \
        && python3 scripts/ci-metrics.py run --name doctests -- cargo test --doc --workspace --locked --offline \
        && python3 scripts/ci-metrics.py summary \
        && cp target/nextest/ci/junit.xml artifacts/ci/timings.jsonl /build/ci-report/ ;; \
      false) printf "%s\n" "Container workspace qualification not requested; image smoke runs separately." > /build/ci-report/scope.txt ;; \
      *) exit 1 ;; \
    esac'
USER 10001:10001

FROM test AS build
USER 0:0
RUN --mount=type=cache,id=airk-cargo-registry,target=/usr/local/cargo/registry,uid=10001,gid=10001,sharing=locked \
    --mount=type=cache,id=airk-cargo-git,target=/usr/local/cargo/git,uid=10001,gid=10001,sharing=locked \
    --mount=type=cache,id=airk-linux-target,target=/build/target,uid=10001,gid=10001,sharing=locked \
    sh scripts/container-build-user.sh sh -ec '\
    python3 scripts/ci-metrics.py run --name release-pair -- python3 scripts/build-service-container.py --destination /build/out \
    && cp artifacts/ci/timings.jsonl /build/ci-report/ \
    && python3 scripts/ci-prune-target.py'
USER 10001:10001

FROM scratch AS report
COPY --from=build /build/ci-report/ /

FROM debian:bookworm-slim AS runtime
RUN groupadd --gid 10001 airk && useradd --uid 10001 --gid airk --home-dir /data --no-create-home airk \
    && mkdir -p /data /workspaces /usr/share/licenses/airk \
    && chown airk:airk /data
COPY --from=build /build/out/runtime /usr/lib/airk
COPY --from=build /build/out/airk-container /usr/local/bin/airk-container
RUN ln -s /usr/lib/airk/airk /usr/local/bin/airk
COPY LICENSE NOTICE THIRD_PARTY_NOTICES.md /usr/share/licenses/airk/
USER 10001:10001
ENV XDG_DATA_HOME=/data/share XDG_STATE_HOME=/data/state XDG_CONFIG_HOME=/data/config XDG_CACHE_HOME=/data/cache
WORKDIR /workspaces
ENTRYPOINT ["airk-container"]
CMD []
