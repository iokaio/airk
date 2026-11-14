# SPDX-License-Identifier: Apache-2.0
FROM node:24.15.0-bookworm-slim AS node-toolchain
FROM rust:1.98.0-slim-bookworm AS toolchain
# Node/npm are build tools only; the final runtime stage copies native payloads.
COPY --from=node-toolchain /usr/local/bin/node /usr/local/bin/node
COPY --from=node-toolchain /usr/local/lib/node_modules/npm /usr/local/lib/node_modules/npm
RUN ln -s ../lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm
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
COPY scripts/provision-static-cmake-source.py scripts/static-cmake-source-lock.json ./scripts/
COPY scripts/static-onnx-link-closure.py ./scripts/
COPY scripts/static-onnx-aggregate.py ./scripts/
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
COPY scripts/license-notices.py scripts/provision-local-model.py scripts/model-build-inputs.py ./scripts/
COPY scripts/provision-onnx-runtime.py scripts/int8-policy.py scripts/macho-identity.py ./scripts/
COPY scripts/provision-static-onnx-runtime.py scripts/static-onnx-runtime-lock.json ./scripts/
COPY scripts/provision-static-onnx-tools.py scripts/static-onnx-tools-lock.json ./scripts/
COPY scripts/provision-static-cmake-source.py scripts/static-cmake-source-lock.json ./scripts/
COPY scripts/static-onnx-link-closure.py ./scripts/
COPY scripts/static-onnx-aggregate.py ./scripts/
COPY scripts/build-service-runtime.py scripts/build-service-container.py ./scripts/
COPY scripts/container-build-user.sh ./scripts/
COPY LICENSE NOTICE THIRD_PARTY_NOTICES.md ./
COPY third_party/models ./third_party/models
ARG AIRK_MODEL_INPUTS_ASSET_ID
RUN --mount=type=secret,id=github_token,required=true \
    python3 scripts/model-build-inputs.py acquire --asset-id "$AIRK_MODEL_INPUTS_ASSET_ID" --destination /build/model-assets --token-file /run/secrets/github_token \
    && python3 scripts/provision-local-model.py --destination /build/model-assets
ENV AIRK_BUNDLED_MODEL_DIR=/build/model-assets
ENV LIBONNXRUNTIME_NO_PKG_CONFIG=1 ORT_SKIP_DOWNLOAD=1 ORT_PREFER_DYNAMIC_LINK=0 \
    ORT_LIB_LOCATION=/build/static-onnx-runtime/b ORT_LIB_PROFILE=Release
COPY third_party/licenses ./third_party/licenses

# Exact external test inputs are reviewed in scripts/container-inputs.json.
COPY fixtures/split/arbiter/public_answer_cases.json ./fixtures/split/arbiter/public_answer_cases.json
COPY packages/admin/src/arbiter.tsx ./packages/admin/src/arbiter.tsx
COPY packages/admin/src/preparedActions.tsx ./packages/admin/src/preparedActions.tsx
COPY packages/admin/src/preparedMutations.ts ./packages/admin/src/preparedMutations.ts
COPY packages/ui/src/usagePresentation.ts ./packages/ui/src/usagePresentation.ts
COPY fixtures/arbiter/publication_catalog.json ./fixtures/arbiter/publication_catalog.json
COPY fixtures/arbiter/profile-downloads.json ./fixtures/arbiter/profile-downloads.json
COPY fixtures/arbiter/cpu-fit-16.json ./fixtures/arbiter/cpu-fit-16.json
COPY fixtures/arbiter/cpu-fit-24.json ./fixtures/arbiter/cpu-fit-24.json
COPY fixtures/split/i3/events_and_rates.json ./fixtures/split/i3/events_and_rates.json
COPY packages/vscode/src/adminUiContract.ts ./packages/vscode/src/adminUiContract.ts
COPY packages/ui/src/navigation.ts ./packages/ui/src/navigation.ts
COPY packages/ui/src/nativeSessionLifetime.ts ./packages/ui/src/nativeSessionLifetime.ts
COPY packages/ui/src/apiPort.ts ./packages/ui/src/apiPort.ts
COPY packages/admin/src/generatedActions.ts ./packages/admin/src/generatedActions.ts
COPY fixtures/split/i4/inno_handoff.json ./fixtures/split/i4/inno_handoff.json
COPY fixtures/arbiter/configuration-disabled-v2.json ./fixtures/arbiter/configuration-disabled-v2.json
COPY fixtures/models/model-policy.json ./fixtures/models/model-policy.json
COPY fixtures/split/native_admin/chunks.json ./fixtures/split/native_admin/chunks.json
COPY fixtures/split/catalog/split_grouping.json ./fixtures/split/catalog/split_grouping.json
COPY fixtures/split/i4/wrapper_utf8_operation.json ./fixtures/split/i4/wrapper_utf8_operation.json
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
COPY fixtures/split/i2/status.json ./fixtures/split/i2/status.json
COPY fixtures/split/i2/version.json ./fixtures/split/i2/version.json
COPY fixtures/split/i3/admin_operations.json ./fixtures/split/i3/admin_operations.json
COPY fixtures/split/i3/mutation_preparation.json ./fixtures/split/i3/mutation_preparation.json
COPY fixtures/split/i3/admission_cases.json ./fixtures/split/i3/admission_cases.json
COPY fixtures/split/i3/envelopes.json ./fixtures/split/i3/envelopes.json
COPY fixtures/split/i3/headers.json ./fixtures/split/i3/headers.json
COPY fixtures/split/i3/launch_exchange.json ./fixtures/split/i3/launch_exchange.json
COPY fixtures/split/i3/native_outcomes.json ./fixtures/split/i3/native_outcomes.json
COPY fixtures/split/i3/recovery_reviews.json ./fixtures/split/i3/recovery_reviews.json
COPY fixtures/split/i4/i4_cases.json ./fixtures/split/i4/i4_cases.json
COPY fixtures/split/i1/control_methods.json ./fixtures/split/i1/control_methods.json
COPY fixtures/split/i1/window_plane.json ./fixtures/split/i1/window_plane.json
COPY fixtures/split/i1/admin_launch_code.json ./fixtures/split/i1/admin_launch_code.json
COPY fixtures/split/agents/config_cases.json ./fixtures/split/agents/config_cases.json
COPY fixtures/split/agents/config_edits/cases.json ./fixtures/split/agents/config_edits/cases.json
COPY fixtures/split/agents/config_edits/linux-conflict.json ./fixtures/split/agents/config_edits/linux-conflict.json
COPY fixtures/split/agents/config_edits/linux-deletion-refused.json ./fixtures/split/agents/config_edits/linux-deletion-refused.json
COPY fixtures/split/agents/config_edits/linux-open-writer-oversized.json ./fixtures/split/agents/config_edits/linux-open-writer-oversized.json
COPY fixtures/split/agents/config_edits/linux-prepared.json ./fixtures/split/agents/config_edits/linux-prepared.json
COPY fixtures/split/agents/config_edits/linux-published.json ./fixtures/split/agents/config_edits/linux-published.json
COPY fixtures/split/agents/config_edits/linux-started.json ./fixtures/split/agents/config_edits/linux-started.json
COPY fixtures/split/agents/config_edits/linux-unknown.json ./fixtures/split/agents/config_edits/linux-unknown.json
COPY fixtures/split/agents/config_edits/macos-published.json ./fixtures/split/agents/config_edits/macos-published.json
COPY fixtures/split/agents/config_edits/marker-unix.json ./fixtures/split/agents/config_edits/marker-unix.json
COPY fixtures/split/agents/config_edits/marker-windows.json ./fixtures/split/agents/config_edits/marker-windows.json
COPY fixtures/split/agents/config_edits/windows-error-1177.json ./fixtures/split/agents/config_edits/windows-error-1177.json
COPY fixtures/split/agents/config_edits/windows-published.json ./fixtures/split/agents/config_edits/windows-published.json
COPY fixtures/split/agents/entry_remove_cases.json ./fixtures/split/agents/entry_remove_cases.json
COPY fixtures/split/agents/credentials_ledger_v1.json ./fixtures/split/agents/credentials_ledger_v1.json
COPY fixtures/split/agents/open_access.json ./fixtures/split/agents/open_access.json
COPY fixtures/split/agents/registry-policy3-descriptions.objects.json ./fixtures/split/agents/registry-policy3-descriptions.objects.json
COPY fixtures/split/agents/registry-policy3-descriptions.meta.json ./fixtures/split/agents/registry-policy3-descriptions.meta.json
COPY fixtures/split/parity ./fixtures/split/parity
COPY fixtures/split/i1/request_recovery.json ./fixtures/split/i1/request_recovery.json
COPY fixtures/split/i1/selection_authority.json ./fixtures/split/i1/selection_authority.json
COPY fixtures/split/i4/role_admission.json ./fixtures/split/i4/role_admission.json
COPY fixtures/split/i4/interruptions.json ./fixtures/split/i4/interruptions.json
COPY fixtures/split/i4/setup_recovery.json ./fixtures/split/i4/setup_recovery.json
COPY fixtures/split/i4/setup_active.json ./fixtures/split/i4/setup_active.json
COPY fixtures/split/i4/portable_cases.json ./fixtures/split/i4/portable_cases.json
COPY fixtures/arbiter/configuration-disabled-v1.json ./fixtures/arbiter/configuration-disabled-v1.json
COPY fixtures/split/ui_pilot ./fixtures/split/ui_pilot
COPY fixtures/split/receipts/runtime_metadata_cases.json ./fixtures/split/receipts/runtime_metadata_cases.json
COPY fixtures/split/receipts/pair_metadata_3.json ./fixtures/split/receipts/pair_metadata_3.json
COPY fixtures/wire/airkctl_session_frames.json ./fixtures/wire/airkctl_session_frames.json
COPY fixtures/wire/catalog_receipt.json ./fixtures/wire/catalog_receipt.json
COPY fixtures/wire/combined_budgets.json ./fixtures/wire/combined_budgets.json
COPY fixtures/wire/native_outcomes.json ./fixtures/wire/native_outcomes.json
COPY fixtures/wire/read_retry_decisions.json ./fixtures/wire/read_retry_decisions.json
COPY fixtures/wire/read_retry_envelopes.json ./fixtures/wire/read_retry_envelopes.json
COPY fixtures/wire/session_frames.json ./fixtures/wire/session_frames.json
COPY fixtures/wire/usage_page.json ./fixtures/wire/usage_page.json
COPY packages/admin/src/bundledHelp.tsx ./packages/admin/src/bundledHelp.tsx
COPY packages/admin/src/pilotPane.tsx ./packages/admin/src/pilotPane.tsx
COPY packages/ui/generated/help-manifest.json ./packages/ui/generated/help-manifest.json
COPY packages/ui/src/browserPilot.ts ./packages/ui/src/browserPilot.ts
COPY packages/ui/src/helpMarkdown.ts ./packages/ui/src/helpMarkdown.ts
COPY packages/ui/src/helpRoute.ts ./packages/ui/src/helpRoute.ts
COPY packages/ui/src/helpSchema.ts ./packages/ui/src/helpSchema.ts
COPY packages/ui/src/helpSearch.ts ./packages/ui/src/helpSearch.ts
COPY packages/ui/src/helpToolPresentation.ts ./packages/ui/src/helpToolPresentation.ts
COPY packages/ui/src/pilotContract.ts ./packages/ui/src/pilotContract.ts
COPY packages/ui/src/pilotPresentation.ts ./packages/ui/src/pilotPresentation.ts
COPY packages/ui/src/pilotProjection.ts ./packages/ui/src/pilotProjection.ts
COPY packages/vscode/src/generated/i2.ts ./packages/vscode/src/generated/i2.ts
COPY packages/vscode/src/uiPilotContract.ts ./packages/vscode/src/uiPilotContract.ts
COPY packages/contracts-gen/i3.schema.json ./packages/contracts-gen/i3.schema.json
COPY packages/contracts-gen/arbiter.schema.json ./packages/contracts-gen/arbiter.schema.json
COPY packages/contracts-gen/arbiter.ts ./packages/contracts-gen/arbiter.ts
COPY packages/vscode/src/generated/i2.schema.json ./packages/vscode/src/generated/i2.schema.json
COPY scripts/check-container.py ./scripts/check-container.py
COPY scripts/check-macos-direct-runtime.py ./scripts/check-macos-direct-runtime.py
COPY scripts/check-managed-runtime.py ./scripts/check-managed-runtime.py
COPY scripts/check-runtime-activation.py ./scripts/check-runtime-activation.py
COPY scripts/check-service-clients.py ./scripts/check-service-clients.py
COPY scripts/check-service-install.py ./scripts/check-service-install.py
COPY scripts/check-tokenizer-calibration.py ./scripts/check-tokenizer-calibration.py
COPY scripts/check-usage-inputs.py ./scripts/check-usage-inputs.py
COPY scripts/container-inputs.json ./scripts/container-inputs.json
COPY scripts/signing-identity.json ./scripts/signing-identity.json
COPY tools/parser-train/conversational-regression-v4.json ./tools/parser-train/conversational-regression-v4.json
COPY tools/parser-train/evaluation.json ./tools/parser-train/evaluation.json
COPY tools/parser-train/fixtures-v2.json ./tools/parser-train/fixtures-v2.json
COPY tools/parser-train/fixtures.json ./tools/parser-train/fixtures.json
COPY tools/parser-train/inference.json ./tools/parser-train/inference.json
COPY tools/parser-train/manifest.json ./tools/parser-train/manifest.json
COPY tools/parser-train/novel-role-queries.json ./tools/parser-train/novel-role-queries.json
COPY tools/parser-train/role-queries.json ./tools/parser-train/role-queries.json
COPY tools/parser-train/safety.json ./tools/parser-train/safety.json

COPY packages/admin/build.mjs ./packages/admin/build.mjs
COPY packages/admin/package-lock.json ./packages/admin/package-lock.json
COPY packages/admin/package.json ./packages/admin/package.json
COPY packages/admin/src/activity.tsx ./packages/admin/src/activity.tsx
COPY packages/admin/src/activityRequests.ts ./packages/admin/src/activityRequests.ts
COPY packages/admin/src/agents.tsx ./packages/admin/src/agents.tsx
COPY packages/admin/src/api.ts ./packages/admin/src/api.ts
COPY packages/admin/src/app.css ./packages/admin/src/app.css
COPY packages/admin/src/app.tsx ./packages/admin/src/app.tsx
COPY packages/admin/src/catalogRequests.ts ./packages/admin/src/catalogRequests.ts
COPY packages/admin/src/concurrencyNote.tsx ./packages/admin/src/concurrencyNote.tsx
COPY packages/admin/src/envelope.ts ./packages/admin/src/envelope.ts
COPY packages/admin/src/folderPicker.tsx ./packages/admin/src/folderPicker.tsx
COPY packages/admin/src/help.tsx ./packages/admin/src/help.tsx
COPY packages/admin/src/index.html ./packages/admin/src/index.html
COPY packages/admin/src/indexLibrary.tsx ./packages/admin/src/indexLibrary.tsx
COPY packages/admin/src/main.tsx ./packages/admin/src/main.tsx
COPY packages/admin/src/operations.ts ./packages/admin/src/operations.ts
COPY packages/admin/src/pageVisibility.ts ./packages/admin/src/pageVisibility.ts
COPY packages/admin/src/pages.tsx ./packages/admin/src/pages.tsx
COPY packages/admin/src/progress.tsx ./packages/admin/src/progress.tsx
COPY packages/admin/src/render.tsx ./packages/admin/src/render.tsx
COPY packages/admin/src/scopeConcurrency.ts ./packages/admin/src/scopeConcurrency.ts
COPY packages/admin/src/session.ts ./packages/admin/src/session.ts
COPY packages/admin/src/usage.tsx ./packages/admin/src/usage.tsx
COPY packages/admin/tsconfig.json ./packages/admin/tsconfig.json
COPY packages/contracts-gen/i3.ts ./packages/contracts-gen/i3.ts
COPY scripts/build-admin-assets.py ./scripts/build-admin-assets.py
COPY scripts/combined_evidence.py ./scripts/combined_evidence.py
COPY scripts/http_fixture.py ./scripts/http_fixture.py
COPY scripts/split_receipts.py ./scripts/split_receipts.py
COPY scripts/standalone-installer-contract.py ./scripts/standalone-installer-contract.py
RUN python3 scripts/build-admin-assets.py --output /build/artifacts/admin/admin-assets.json

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
RUN ln -s /usr/lib/airk/airk /usr/local/bin/airk \
    && ln -s /usr/lib/airk/airkctl /usr/local/bin/airkctl
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
