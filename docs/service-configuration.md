# Guarded project configuration

This is the implemented development contract for the [1.4 service plan](airkservice-architecture.md). Native and extension versions remain 1.3.2 until default adapter cutover. These controls do not qualify an installed editor or release a VSIX.

The trusted Service Console's **Project settings** action supports vectors, extraction detail, semantic selection rules, additional source roots, explicit offline custom-model selection, explicit return to the bundled model, usage recording policy and local tokenizer calibration. Inspect the project first. Host-owned pickers and dialogs collect the choices; webview messages carry only a fixed action and a visible project identity. Trust is checked again after each asynchronous preparation step.

`airk service prepare-configuration --runtime-dir <control> --project <binding>` reads one typed draft from stdin, bounded to 16 KiB. It reads the cached inspection and returns a request guarded by the exact index incarnation and desired-settings digest. File preparation canonicalizes explicit absolute paths and hashes bounded inputs. Model manifests are limited to 16 KiB, tokenizer definitions to 1 MiB, and each calibration sample to 256 KiB. Preparation does not enqueue work, start a project or change its sources/settings.

## Acceptance and execution

The `configure_project` control method carries the complete guarded request, caller and request key. The client persists that exact prepared intent before sending. Changing any accepted request field conflicts with its key. Equivalent selection-rule order has one canonical representation. A repeated key after an engine upgrade still returns the original receipt; a new request joins pending work only when both the request and accepting engine match. Explicit retry retains the original request and engine.

The service stores metadata and schedules a fenced `configure` job. The project worker checks the accepting engine, current incarnation and settings digest before effects. A stale request fails with `configuration_settings_or_engine_changed`; inspect again before deliberately preparing a replacement. Adding/removing roots reserves the proposed source boundaries in the registry before changing configuration, using the same serialized protection as registration. Protected control, data and installation paths remain excluded.

Configuration that affects indexed evidence requests a later durable source pass. Its configuration receipt does not claim that pass has published or that semantic coverage is complete. Model/calibration operations use the existing direct helper and bounded shared model/input stores; the service broker never carries model bytes or calibration text. Source-only settings do not depend on a responsive inference helper. Selecting the bundled model through Project settings explicitly replaces a custom selection; the separate **Install bundled model** action continues to preserve custom selection.

## Bounded receipts and uncertain outcomes

Each accepted configuration reserves 16,392 bytes for a result: an eight-byte format/length header and up to 16 KiB of JSON metadata. SQLite incremental BLOB writes fill that reservation in place, in the same transaction as the source-continuation flag. The background row is also reserved before execution. A full database therefore cannot require a fresh overflow allocation just to save the bounded result. This uses the existing SQLite dependency's BLOB feature, with no additional dependency.

The ledger remains capped at 32 MiB, 1,024 jobs and 4,096 aliases. Admission also accounts for physical page pressure, reserving one eighth of the configured page ceiling, capped at 1,024 pages, as headroom. Up to 64 oldest terminal receipts can retire under pressure; pending/running work and a protected retry origin remain retained. Receipt retirement advances the request epoch atomically. Known retained keys still replay, while retired keys expire instead of repeating an operation. New work can fail capacity admission before acceptance.

Development registry schema 4 adds the fixed result reservation. Versions 1/2 upgrade additively; version 3 migrates bounded requests and JSON results while preserving the registry namespace, aliases, jobs and fences. A missing required table in a completed registry remains a refusal. Read-only saved snapshots never migrate a registry.

Recovery marks an older running configuration successful only if its saved result decodes correctly. Without a valid result, the job fails with `configuration_outcome_unknown_reinspect` and is never automatically replayed: the settings effect may already have happened. Damaged result bytes are preserved. Reinspect the actual settings before explicitly choosing another change. A result transaction and the configuration effect are separate boundaries; the plan does not claim atomicity between them.

## Privacy and presentation

Durable requests retain selected file paths and SHA-256 hashes until receipt retirement; they do not retain sample plaintext. The worker verifies the exact sample/tokenizer bytes before counting or recording. Usage aggregates remain per collection. Calculate-only calibration does not create an aggregate record. Results contain bounded metadata and counts, with original selected files unchanged.

Live console pages include the latest configuration job independently of newer source/vector progress. The host retrieves its bounded result and offers exact JSON Copy. The view labels cached results and truncates only the display at 4,096 characters. Down-service snapshots retain a separate 64 KiB result budget for at most 32 visible projects, under the overall 256 KiB snapshot ceiling. Omitted results are explicitly labeled; project/lifecycle metadata remains available. Default diagnostic export includes configuration job/result status, not the result payload or prepared file selectors.

## Development evidence

Windows registry tests cover exact-key persistence, canonical policies, engine identity, interrupted effects, valid-result recovery, damaged-result preservation, versions 2/3 migration, missing completed tables, pressure retirement and a result write with SQLite's page limit pinned to its existing page count. Kernel tests verify stale guards and exact calibration-input hashes while preserving sources and excluding sample text from results. Native preparation tests verify bounds and selector-only output. Extension tests cover persistence before send, exact uncertain retry, trust loss, cached result Copy, omission labels and diagnostic redaction.

Windows passed 70 registry tests, 85 kernel unit tests, 104 CLI unit tests and 21 native service tests at this configuration checkpoint. The extension passed 184 tests with six skips; Python passed 129 tests with four skips. All-feature/all-target workspace Clippy, formatting, repository and whitespace checks passed. The broader workspace run remains in progress; these counts do not claim its completion.

Non-root Debian 12 x64 passed 70 registry tests, two focused kernel configuration tests, 104 CLI unit tests, 20 native service tests and lightweight all-target Clippy. Its review image is `6c963e51f9e25acb54ffdf0cae61344b75838f10849d367be8c8f6f9177763b0`.

The complete real native/controller fixture passed detail/vector settings, protected-root refusal, additional roots, shared custom-model selection, usage policy, recorded local calibration and exact result Copy. It also passed persisted work, guarded repair approval, diagnostic preview, trust refusal, durable Stop/Start, missing-helper observation and approved payload repair. It removed only its owned fixture and preserved original source bytes throughout. Debug pair `target/airksvc-runtime-stage39` has source fingerprint `4336537c44530e45aebc217f6f84cc66b3ea30db24db7f15117a8e484aa11fb0` (631 inputs), main digest `d5417be359b080275ebe71e065ead88cb0aad53531a2b48bc1596805f50508e5` and helper digest `ba6613961241b58d3215f2597e19e55eede5ac9b87dfb7fcc426bc213eec42f5`. No installed-editor or native macOS acceptance is inferred. No VSIX was built.
