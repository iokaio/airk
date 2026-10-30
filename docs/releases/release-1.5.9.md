# AIRK 1.5.9 — MCP negotiation and indexing diagnostics

Local candidate work on `feat/manageindices`, 28 October 2026. This record supersedes the 1.5.8 installer for the protocol correction; the [1.5.8 record](release-1.5.8.md) retains its exact artifact and prior validation.

## Reproduced failure

The affected machine reported initialization success and server instructions followed by `tools/list` failure: `request _meta is missing or has malformed required fields: io.modelcontextprotocol/protocolVersion, io.modelcontextprotocol/clientCapabilities`. A direct legacy-only probe and an earlier Codex session succeeded. These are different protocol paths; successful initialization alone does not establish tool discovery.

The exact packaged 1.5.8 native runtime reproduces that error locally with `server/discover` followed by a buffered legacy `initialize`, then `tools/list` without inline metadata. No timing delay, live index or running editor is required. The pinned rmcp 3.2.0 startup path permanently sets its peer's inline-metadata requirement when discovery comes first; its later initialize handler accepts the handshake but leaves that requirement set. Legacy-only tests missed the transition. This reproduces the reported Claude failure sequence; the separate temporarily missing Codex tools were later reported available, with no transport cause established.

## Correction

An `airk-mcp` stdio lifecycle adapter owns this small connection boundary while retaining SDK framing, cancellation, metadata checks and request dispatch. Successful initialization records the actual negotiated legacy version and client capabilities. Earlier or later discovery cannot overwrite it. Legacy requests may omit protocol metadata; modern requests still require complete, valid supported metadata. A second initialize cannot replace an already negotiated session. Unknown requested handshake versions negotiate a supported legacy version rather than accidentally selecting a modern version that has no initialize handshake.

The state belongs to each stdio connection, not the project, process-global configuration or assistant name. Separate Codex, Claude Code, Copilot or other client connections share the same registered index and retain independent protocol/response state. Reconnection negotiates afresh. Modern per-request client identity now reaches usage attribution through the SDK context, rather than depending on a legacy handshake that never happened.

`suggest_view` advertises its alternatives inside the `draft` property, with a plain object at the tool schema root. This avoids the top-level composition form implicated in a [Claude Code tool omission report](https://github.com/anthropics/claude-code/issues/95504). The argument names and per-kind validation in the editor adapter are unchanged. The catalog test rejects root composition for all 26 tools. This is a compatibility precaution, separate from the locally reproduced handshake failure; it is not proof of that report's behavior on the affected machine.

The 26 tool names, saved bindings, config files, index namespace, indexing/recovery policy and source data are unchanged. The launch descriptor is optional for native tools and its stable filename is not a session identifier. Tool discovery remains independent of service/model/index readiness. This correction does not establish the cause of Bookbuilder's separate `project_operation_failed` freshness failure or repair it by resetting retry limits.

## Indexing diagnostics

The affected machine's complete retained ring contained process failures but no underlying error for its newer failed reconcile job. A healthy worker can finish a failed job without exiting; process-exit events did not cover that case. The supervisor now records `job_failed` with the exact project, job identity and durable failure code when it observes that job finish.

Index preparation and discovery errors retain fixed stage codes, including snapshot selection, current-index recovery, retained inventory, model preparation and source scanning. Safe native SQLite/OS codes are found through nested error sources; known inventory/integrity failures and configured source-byte, extracted-byte and file-count limits have fixed codes. Raw paths, SQL, source content and arbitrary exception text are not copied into logs. Unknown causes remain explicitly unspecified within their observed stage. Existing typed recovery decisions and retry budgets are preserved.

These diagnostics apply to new failures after activation. They cannot reconstruct the missing cause of an older failure, and discovery phase timing alone does not establish that a filesystem walk failed: index preparation occurs within that phase too. The Bookbuilder cause remains unconfirmed.

Management's recent service events now show readable timestamps, job IDs and failure codes; Service's copied details retain them too. Previously this view omitted the job and code even when the native record contained them. Configured ingestion-limit failures explain which limit was reached and direct users to review it before retrying, while retaining the precise native code.

## Validation

The final locked workspace/all-features run passed **1,419 tests across 67 targets**, with 17 ignored and zero failures. Formatting, workspace/all-targets/all-features Clippy with warnings denied, repository checks, version agreement and whitespace checks passed. The extension suite passed 356 tests with two platform-specific skips (358 total); the 18 Management tests also passed after the final wording adjustment. The Python tooling suite ran 177 tests successfully, including four skips.

Protocol regression coverage includes pipelined discovery/fallback, repeated discovery after initialization, concurrent legacy/fallback/modern connections, missing/malformed/unsupported metadata, and version/client identity preservation. Eight cold-start process probes exercised all four supported legacy versions with two client labels, no running service, no index and an unavailable Explorer descriptor; every connection returned 26 tools and answered `projects`. The native client campaign opened three real processes against the same saved project, retrieved the same evidence and reconnected all three without setup or reindexing. These protocol fixtures do not claim live qualification of proprietary assistant applications or other operating systems.

The final native campaign passed in 19.3 seconds with two isolated projects and 16 files per project. It verified lexical isolation, offline semantic publication/querying, same-job recovery after restoring a missing source directory, and automatic publication after an obsolete queued repair was settled. Deliberately damaged fixture metadata produced `index_current_scope_metadata_incomplete` in both the durable receipt and the retained `job_failed` record with the exact job ID. Restoring the original metadata allowed indexing again; original source bytes were preserved. Windows fixture ownership checks compare file identity as well as containment so extended native path spelling does not weaken isolation.

An intermediate full run found one capacity test expecting the original outer error message. The corrected test checks the full cause chain and specific safe limit code, retaining its assertions for the original size/path explanation, unchanged publication and unchanged source. The subsequent full run passed. The real-editor harness also removes only its own temporary Windows terminal PATH entry after each run, preserving other entries and the registry value type.

An explicit review pass covered the combined lifecycle/metadata dispatch, project isolation, error-chain and retry contracts, retained-log identity, Management display/copy behavior, installation-test ownership and package provenance. The implementing agent performed this review; it was not an independent review. Bookbuilder, maximum-capacity indexing, macOS/Linux and live Claude Code/Codex/Copilot applications remain outside this local qualification.

## Local installer workflow

The version is synchronized at **1.5.9**. The release-mode pair at `artifacts/local/management-1.5.9-20261028/runtime-release` was rebuilt after testing and matches the tested pair byte for byte. Its install-manifest SHA-256 is `4663782c603f46d4b0a24e0c9642cb79e1c964fd4692e6c33d4dbed20521c81b`; its native source receipt is `bd8c0a5a7ac2509e7ddcc5b7c22bc1a60d7775dbd76ab7d888c84cc0c052b893` over 677 files. License notices were generated for 411 packages; the existing declaration-only notices for adobe-cmap-parser, pdf-extract and type1-encoding-parser still require publication-time review.

The requested sequence is tests, build, local signed-off commit, then installer packaging without a push. Packaging uses that source commit and the verified runtime, with output at `artifacts/local/management-1.5.9-20261028/installer/publish/airk-1.5.9-win32-x64.vsix`. The adjacent package manifest and `SHA256SUMS` record its exact commit and digest. Installed-editor acceptance is a separate post-packaging check whose private receipts remain under `artifacts/editor-qualification`; source tests alone do not establish that result. This is an unsigned local test candidate, not a merged-main release or publication.
