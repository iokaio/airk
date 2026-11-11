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
FROM toolchain AS static-inference
COPY scripts/provision-static-onnx-runtime.py scripts/static-onnx-runtime-lock.json ./scripts/
COPY scripts/provision-static-onnx-tools.py scripts/static-onnx-tools-lock.json ./scripts/
COPY scripts/static-onnx-link-closure.py ./scripts/
RUN python3 scripts/provision-static-onnx-runtime.py --destination /build/static-onnx-runtime --download --download-tools --build --parallel 2

FROM toolchain AS source
COPY --from=static-inference /build/static-onnx-runtime /build/static-onnx-runtime
COPY Dockerfile .dockerignore ./
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY .cargo/config.toml ./.cargo/config.toml
COPY crates ./crates
# Native and extension tests share this exact public authority-digest fixture.
COPY packages/vscode/src/test/fixtures/httpAuthorityDigest.json ./packages/vscode/src/test/fixtures/httpAuthorityDigest.json
COPY .config/nextest.toml ./.config/nextest.toml
COPY scripts/ci-metrics.py scripts/ci-prune-target.py ./scripts/
COPY scripts/license-notices.py scripts/provision-local-model.py ./scripts/
COPY scripts/provision-onnx-runtime.py scripts/int8-policy.py scripts/macho-identity.py ./scripts/
COPY scripts/provision-static-onnx-runtime.py scripts/static-onnx-runtime-lock.json ./scripts/
COPY scripts/provision-static-onnx-tools.py scripts/static-onnx-tools-lock.json ./scripts/
COPY scripts/static-onnx-link-closure.py ./scripts/
COPY scripts/build-service-runtime.py scripts/build-service-container.py ./scripts/
COPY scripts/container-build-user.sh ./scripts/
COPY LICENSE NOTICE THIRD_PARTY_NOTICES.md ./
COPY third_party/models ./third_party/models
RUN python3 scripts/provision-local-model.py --destination /build/model-assets --download
ENV AIRK_BUNDLED_MODEL_DIR=/build/model-assets
ENV LIBONNXRUNTIME_NO_PKG_CONFIG=1 ORT_SKIP_DOWNLOAD=1 ORT_PREFER_DYNAMIC_LINK=0 \
    ORT_LIB_LOCATION=/build/static-onnx-runtime/b ORT_LIB_PROFILE=Release
COPY third_party/licenses ./third_party/licenses

# Exact external test inputs are reviewed in scripts/container-inputs.json.
COPY docs/cli-reference.md ./docs/cli-reference.md
COPY docs/research/contracts-v190/fixtures.json ./docs/research/contracts-v190/fixtures.json
COPY fixtures/schema/airk-state-1/catalog-schema11-backup.meta.json ./fixtures/schema/airk-state-1/catalog-schema11-backup.meta.json
COPY fixtures/schema/airk-state-1/catalog-schema11-backup.objects.json ./fixtures/schema/airk-state-1/catalog-schema11-backup.objects.json
COPY fixtures/schema/airk-state-1/current.service.meta.json ./fixtures/schema/airk-state-1/current.service.meta.json
COPY fixtures/schema/airk-state-1/current.service.objects.json ./fixtures/schema/airk-state-1/current.service.objects.json
COPY fixtures/schema/airk-state-1/current.standalone.meta.json ./fixtures/schema/airk-state-1/current.standalone.meta.json
COPY fixtures/schema/airk-state-1/current.standalone.objects.json ./fixtures/schema/airk-state-1/current.standalone.objects.json
COPY fixtures/schema/airk-state-1/scheduler-1.2.meta.json ./fixtures/schema/airk-state-1/scheduler-1.2.meta.json
COPY fixtures/schema/airk-state-1/scheduler-1.2.objects.json ./fixtures/schema/airk-state-1/scheduler-1.2.objects.json
COPY fixtures/schema/airk-state-1/service-1.4.adopted-policy0.meta.json ./fixtures/schema/airk-state-1/service-1.4.adopted-policy0.meta.json
COPY fixtures/schema/airk-state-1/service-1.4.adopted-policy0.objects.json ./fixtures/schema/airk-state-1/service-1.4.adopted-policy0.objects.json
COPY fixtures/schema/airk-state-1/service-1.4.adopted-policy1.meta.json ./fixtures/schema/airk-state-1/service-1.4.adopted-policy1.meta.json
COPY fixtures/schema/airk-state-1/service-1.4.adopted-policy1.objects.json ./fixtures/schema/airk-state-1/service-1.4.adopted-policy1.objects.json
COPY fixtures/schema/airk-state-1/service-1.4.adopted-policy2.meta.json ./fixtures/schema/airk-state-1/service-1.4.adopted-policy2.meta.json
COPY fixtures/schema/airk-state-1/service-1.4.adopted-policy2.objects.json ./fixtures/schema/airk-state-1/service-1.4.adopted-policy2.objects.json
COPY fixtures/schema/airk-state-1/service-1.4.adopted-policy3.meta.json ./fixtures/schema/airk-state-1/service-1.4.adopted-policy3.meta.json
COPY fixtures/schema/airk-state-1/service-1.4.adopted-policy3.objects.json ./fixtures/schema/airk-state-1/service-1.4.adopted-policy3.objects.json
COPY fixtures/schema/airk-state-1/service-1.4.authority0.meta.json ./fixtures/schema/airk-state-1/service-1.4.authority0.meta.json
COPY fixtures/schema/airk-state-1/service-1.4.authority0.objects.json ./fixtures/schema/airk-state-1/service-1.4.authority0.objects.json
COPY fixtures/schema/airk-state-1/service-1.4.authority1-armed.meta.json ./fixtures/schema/airk-state-1/service-1.4.authority1-armed.meta.json
COPY fixtures/schema/airk-state-1/service-1.4.authority1-armed.objects.json ./fixtures/schema/airk-state-1/service-1.4.authority1-armed.objects.json
COPY fixtures/schema/airk-state-1/service-1.4.authority1.meta.json ./fixtures/schema/airk-state-1/service-1.4.authority1.meta.json
COPY fixtures/schema/airk-state-1/service-1.4.authority1.objects.json ./fixtures/schema/airk-state-1/service-1.4.authority1.objects.json
COPY fixtures/schema/airk-state-1/service-1.4.policy1-armed.meta.json ./fixtures/schema/airk-state-1/service-1.4.policy1-armed.meta.json
COPY fixtures/schema/airk-state-1/service-1.4.policy1-armed.objects.json ./fixtures/schema/airk-state-1/service-1.4.policy1-armed.objects.json
COPY fixtures/schema/airk-state-1/service-1.4.policy1.meta.json ./fixtures/schema/airk-state-1/service-1.4.policy1.meta.json
COPY fixtures/schema/airk-state-1/service-1.4.policy1.objects.json ./fixtures/schema/airk-state-1/service-1.4.policy1.objects.json
COPY fixtures/schema/airk-state-1/service-1.4.policy2-armed.meta.json ./fixtures/schema/airk-state-1/service-1.4.policy2-armed.meta.json
COPY fixtures/schema/airk-state-1/service-1.4.policy2-armed.objects.json ./fixtures/schema/airk-state-1/service-1.4.policy2-armed.objects.json
COPY fixtures/schema/airk-state-1/service-1.4.policy2.meta.json ./fixtures/schema/airk-state-1/service-1.4.policy2.meta.json
COPY fixtures/schema/airk-state-1/service-1.4.policy2.objects.json ./fixtures/schema/airk-state-1/service-1.4.policy2.objects.json
COPY fixtures/schema/airk-state-1/service-1.4.policy3-armed.meta.json ./fixtures/schema/airk-state-1/service-1.4.policy3-armed.meta.json
COPY fixtures/schema/airk-state-1/service-1.4.policy3-armed.objects.json ./fixtures/schema/airk-state-1/service-1.4.policy3-armed.objects.json
COPY fixtures/schema/airk-state-1/service-1.4.policy3.meta.json ./fixtures/schema/airk-state-1/service-1.4.policy3.meta.json
COPY fixtures/schema/airk-state-1/service-1.4.policy3.objects.json ./fixtures/schema/airk-state-1/service-1.4.policy3.objects.json
COPY fixtures/schema/airk-state-1/service-1.4.schema11.meta.json ./fixtures/schema/airk-state-1/service-1.4.schema11.meta.json
COPY fixtures/schema/airk-state-1/service-1.4.schema11.objects.json ./fixtures/schema/airk-state-1/service-1.4.schema11.objects.json
COPY fixtures/schema/airk-state-1/usage.base.meta.json ./fixtures/schema/airk-state-1/usage.base.meta.json
COPY fixtures/schema/airk-state-1/usage.base.objects.json ./fixtures/schema/airk-state-1/usage.base.objects.json
COPY fixtures/schema/airk-state-1/usage.supplemental.meta.json ./fixtures/schema/airk-state-1/usage.supplemental.meta.json
COPY fixtures/schema/airk-state-1/usage.supplemental.objects.json ./fixtures/schema/airk-state-1/usage.supplemental.objects.json
COPY fixtures/split/i2/error_lines.json ./fixtures/split/i2/error_lines.json
COPY fixtures/split/i2/exit_codes.json ./fixtures/split/i2/exit_codes.json
COPY fixtures/split/i2/version.json ./fixtures/split/i2/version.json
COPY fixtures/split/i3/admin_operations.json ./fixtures/split/i3/admin_operations.json
COPY fixtures/split/i3/admission_cases.json ./fixtures/split/i3/admission_cases.json
COPY fixtures/split/i3/envelopes.json ./fixtures/split/i3/envelopes.json
COPY fixtures/split/i3/headers.json ./fixtures/split/i3/headers.json
COPY fixtures/split/i3/launch_exchange.json ./fixtures/split/i3/launch_exchange.json
COPY fixtures/split/i4/i4_cases.json ./fixtures/split/i4/i4_cases.json
COPY fixtures/split/i4/role_admission.json ./fixtures/split/i4/role_admission.json
COPY fixtures/wire/airkctl_session_frames.json ./fixtures/wire/airkctl_session_frames.json
COPY fixtures/wire/catalog_receipt.json ./fixtures/wire/catalog_receipt.json
COPY fixtures/wire/native_outcomes.json ./fixtures/wire/native_outcomes.json
COPY fixtures/wire/read_retry_decisions.json ./fixtures/wire/read_retry_decisions.json
COPY fixtures/wire/read_retry_envelopes.json ./fixtures/wire/read_retry_envelopes.json
COPY fixtures/wire/session_frames.json ./fixtures/wire/session_frames.json
COPY fixtures/wire/usage_page.json ./fixtures/wire/usage_page.json
COPY packages/contracts-gen/i3.schema.json ./packages/contracts-gen/i3.schema.json
COPY packages/vscode/src/generated/i2.schema.json ./packages/vscode/src/generated/i2.schema.json
COPY packages/vscode/src/test/diagnosticLogs.test.ts ./packages/vscode/src/test/diagnosticLogs.test.ts
COPY packages/vscode/src/test/extensionUninstall.test.ts ./packages/vscode/src/test/extensionUninstall.test.ts
COPY packages/vscode/src/test/serviceBootstrap.test.ts ./packages/vscode/src/test/serviceBootstrap.test.ts
COPY scripts/check-container.py ./scripts/check-container.py
COPY scripts/check-macos-direct-runtime.py ./scripts/check-macos-direct-runtime.py
COPY scripts/check-managed-runtime.py ./scripts/check-managed-runtime.py
COPY scripts/check-runtime-activation.py ./scripts/check-runtime-activation.py
COPY scripts/check-service-clients.py ./scripts/check-service-clients.py
COPY scripts/check-service-install.py ./scripts/check-service-install.py
COPY scripts/check-tokenizer-calibration.py ./scripts/check-tokenizer-calibration.py
COPY scripts/check-usage-inputs.py ./scripts/check-usage-inputs.py
COPY scripts/container-inputs.json ./scripts/container-inputs.json
COPY tools/parser-train/conversational-regression-v4.json ./tools/parser-train/conversational-regression-v4.json
COPY tools/parser-train/evaluation.json ./tools/parser-train/evaluation.json
COPY tools/parser-train/fixtures-v2.json ./tools/parser-train/fixtures-v2.json
COPY tools/parser-train/fixtures.json ./tools/parser-train/fixtures.json
COPY tools/parser-train/inference.json ./tools/parser-train/inference.json
COPY tools/parser-train/manifest.json ./tools/parser-train/manifest.json
COPY tools/parser-train/novel-role-queries.json ./tools/parser-train/novel-role-queries.json
COPY tools/parser-train/role-queries.json ./tools/parser-train/role-queries.json
COPY tools/parser-train/safety.json ./tools/parser-train/safety.json

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
    python3 scripts/ci-metrics.py run --name release-runtime -- python3 scripts/build-service-container.py --destination /build/out \
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

# An actual HTTP test client runs inside the loopback namespace. Python belongs
# only to this explicitly selected test image, never the distributed runtime.
FROM runtime AS smoke
USER 0:0
RUN apt-get update && apt-get install -y --no-install-recommends python3 \
    && rm -rf /var/lib/apt/lists/*
USER 10001:10001

FROM runtime AS final
