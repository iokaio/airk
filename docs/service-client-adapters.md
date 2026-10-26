# Service client adapters and request recovery

The current branch is **1.4.5** and uses redb-backed native service metadata. Fresh registration/full indexing replace old SQLite state; see [cutover qualification](release-1.4.5.md). The October 24 artifact identities and 1.3.2/1.4.0 development records below remain historical evidence.


This records current adapter behavior for the [1.4 plan](airkservice-architecture.md). The package activates `serviceExtension.ts`; extension and public CLI defaults use native service ownership at 1.4.5. The dated evidence below predates that cutover. Final artifact and installed-platform qualification remain separate gates.

## Direct native clients

The stable launcher's opaque project context supports `mcp`, `tools call`, `tools shell`, `status` and ordinary `index`. Read-only tools use the same direct project client as MCP, including finite admission, connection renewal, per-client status receipts and cancellation handling. Tool schemas/help remain available without opening a collection. CLI tools honor the Terse default, the response-mode environment setting and an explicit per-call override.

Frontend resolution first verifies the approved selected runtime and elected service. Root/data/collection lookup requests one existing registration through the instance-fenced native metadata adapter. Short-lived clients do not open or decode the shared registry. The reply must match the requested opaque binding; paused projects retain their registration, and unknown bindings fail without creating state. It never creates a local Kernel or adopts a different root merely because collection names match. An explicit saved opaque binding can resolve retained evidence without canonicalizing unavailable sources. Shell labels use the native saved registration, independently of display arguments supplied to the launcher.

Ordinary service indexing uses the registered source and durable native queue. Legacy window-scheduler options, path overrides and unreviewed full rebuilds are refused on this path. A full rebuild instead creates a native guarded maintenance preview; the console still requires separate approval before execution.

## Durable CLI requests

The default 1.4 adapter also supports [direct usage/settings pages and guarded configuration](service-native-management.md). Configuration intents use the same bounded journal and exact-key recovery as enqueue intents. Their inspection and input selectors remain native; report bodies do not pass through the broker.

The native CLI saves an immutable request before submitting it. Each record binds its control location, registry namespace, caller, nonce, request epoch, project and job kind. The same-version/engine metadata adapter checks health and sends to the observed service instance. A changed instance or uncertain response preserves the record. There is no automatic fresh-key replay.

The private control directory contains a `client-requests` journal with at most 64 records, each at most 64 KiB: an additional bounded 4 MiB of client metadata, separate from the registry database. A short file lock serializes admission and file changes; it is released before network waits. Writes sync the prepared file and, on Unix, the containing directory before submission. Existing control/state source isolation protects this journal. Unknown/unfinished files, changed record bytes or scope, and exhausted capacity are explicit errors; they are preserved for inspection. No sample or source content is copied into these enqueue records.

```console
airk service requests --runtime-dir <control> list
airk service requests --runtime-dir <control> prepare --project <binding> --kind index
airk service requests --runtime-dir <control> retry <saved-nonce>
airk service requests --runtime-dir <control> forget <saved-nonce>
```

`prepare` writes a durable request and reports `submitted: false`. `retry` submits those exact saved bytes/key. A complete matching native job receipt is printed before its local record is retired. Lost acknowledgment therefore leaves an explicit recovery path; native acceptance and job completion remain separate. Forgetting a local key never cancels native work or asserts its outcome. After a key expires natively, the CLI preserves that explicit failure instead of manufacturing another request.

## Native extension activation

The service-backed activation registers no editor source watcher or indexing scheduler. It advertises only reviewed native workspace bindings. Resolution honors explicit Stop and selects the installed stable launcher, with the launcher directory as the working directory so unavailable sources can retain read-only MCP access. The panel's response-mode setting controls generated definitions and the tools terminal.

Trusted activation installs the verified runtime, registers workspace roots, prepares the offline model and queues indexing with native watching. Disabling automatic installation preserves the manual setup path. The 1.4.5 storage boundary requires fresh registration/full indexing at the saved selected data location; old database-backed registrations, jobs and usage are not imported. Native preparation reserves storage before creation, while separately stored settings and sources remain protected. Initial requests use the same durable extension journal as console work. Recognized existing Codex, Claude Code and Copilot connections are refreshed to the stable launcher; foreign/custom entries require review. Automatic watching can continue after windows close and is independently configurable.

Each opened root has one separately saved active project selection. Reviewing new storage selects the new binding while retaining the preceding registration and native state. Closed native registrations remain in the service console. Open-workspace highlighting uses those opaque bindings. Status/tools calls remain direct; repair commands capture a bounded native preview without approving it. PATH setup points to the verified stable launcher directory and preserves existing entries instead of copying a versioned extension helper over service executables.

## Historical development evidence before default cutover

Windows and Linux pass 113 native library tests and three CLI tests. Extension checks pass 197 tests with six skips, including actual activation against a VS Code API fixture that refuses any editor-owned source watcher, verifies no indexing during definition advertisement, and checks explicit Stop. Python passes 129 tests with four skips. Workspace Clippy, separately run documentation tests, formatting, repository and whitespace checks pass. The Linux image is `7a58dc20eaa2c5579eee02abd8935f7b5af5b5f4190b631c95c800a17590d90b`.

The installed pair-45 workload passes direct CLI tools and interactive shell calls. It prepares a saved request, accepts that exact request natively while retaining the client's record (the state after losing an acknowledgment), then retries through a separate CLI process and verifies the original job ID. Stable-launcher MCP reads pass with sources present and unavailable. The fixture exits successfully after uninstalling its owned runtime and removing its private files. Its frozen source receipt is `e3eff8b6e01ce602e20fd6819b90046a54e60aed7f9a0cad0c82d899309a9fe3` (634 inputs), main `014a47c58daef154ddc6ac0175182ba27f1ec29b4af5941dfc1e4e0c90af89ca`, helper `f9dcfbcc9c6250e4e352a383ee1a3e5c8e3b9eeba659b22253a6b79e2fa26f4e`.

The complete default CLI/extension/container cutover, all supported management surfaces, explicit legacy configuration review/removal, schema-2 default packaging, synchronized 1.4.0 versions, full frozen qualification and final VSIX remain open. Native Mac and installed-editor qualification are not claimed. No VSIX has been built.
