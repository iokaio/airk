# AIRK 1.6.1 installer fixes

Historical local candidate record. This candidate was not published; its changes are consolidated in the [1.6.4 public release](release-1.6.4.md). Validation below retains its original version and scope.

Native and extension versions are synchronized at **1.6.1**. This is an unpublished
candidate until the reviewed source passes CI and the publication workflows finish.
This candidate includes the subsequently requested registration recovery, MCP scope, diagnostics and Explorer polling fixes. Storage formats, extraction and model selection remain unchanged.

## Upgrade correction

The shipped 1.6.0 extension's automatic-upgrade policy admitted only the 1.4 and
1.5 release lines. An existing installation therefore skipped activation of the
new runtime and failed its connection check with
`explicit_runtime_upgrade_or_matching_extension_required`. Restarting VS Code or
using Install or Repair CLI repeated the same path.

1.6.1 admits forward upgrades within the supported 1.4, 1.5 and 1.6 lines before
connecting. It can supersede a verified unfinished predecessor upgrade at the
native pre-probe boundary without authorizing that older candidate to run under
the new extension. Downgrades, unknown release lines, external management and
exact payload/selection checks retain their protections.

The required extension suite reads the target version from `package.json` and
exercises startup upgrades from 1.4, 1.5 and 1.6, explicit repair/retry, retained
project registrations and quiet subsequent startup. A future release bump that
is absent from the automatic-upgrade policy fails the suite.

## Complete uninstall

The extension now packages a standalone `vscode:uninstall` hook. VS Code invokes
this hook when extension removal completes after restart, and limits the hook to
five seconds. It hands off to a separate cleanup worker outside the extension
directory. The worker carries only the approved main executable and its own
bundled script; it verifies the executable digest before invoking lifecycle
commands. It needs neither the model weights nor the extension directory to
finish. The finite job log and `result.json` location are recorded in VS Code's
uninstall log. See the [VS Code uninstall hook contract](https://code.visualstudio.com/api/references/extension-manifest#extension-uninstall-hook).

Setup records bounded, per-installation/project cleanup receipts outside the
extension directory, including custom runtime locations and projects later
closed. Cleanup removes recognized AIRK entries from recorded project agent
files while preserving other servers, settings and comments. Custom, changed or
unsafe configurations remain for review and produce an explicit failure result.

Native uninstall records its intent before process shutdown so racing launchers
cannot restart work. On Windows and Linux it captures and stops verified owned
main, service, index, source, helper and launcher processes, then removes their
installed executable entry points. This includes the universal launcher named
by agent configurations, with durable receipt recovery for interrupted removal.
Unknown images and other installations are preserved. macOS retains authenticated
service/manager drain; forced process termination is not qualified there.

Project sources and index/service data are preserved. Missing or unrecorded agent
files cannot recreate a removed launcher; their stale configuration may require
manual removal. Organization-managed runtime payloads retain their existing
external ownership policy.

## Recovery, connections and responsive diagnostics

Trusted startup checks the native active and removed registrations instead of treating a saved editor receipt as proof that registration still exists. Missing open projects are registered and indexed automatically. An atomic only-if-missing guard preserves a concurrent explicit removal. Existing manually restored indexes and their settings are reused. Offline or unreadable registry metadata never authorizes a new registration.

New managed MCP entries contain the stable launcher and `--root <source> mcp`, without the storage directory or internal project binding. Each request resolves the current active registration for that source. Missing, deliberately removed and ambiguous indexes are distinct outcomes; `--index-collection` explicitly disambiguates multiple indexes. Root scopes stay fixed, while internal bindings may change. Revision tokens and evidence validators retain their exact publication/incarnation checks. MCP does not register projects or start indexing. Trusted startup migrates recognized older entries through guarded saves, preserving client policies and unrelated servers. Explorer's internal evidence session remains pinned to its reviewed registration.

Overview and Management status polls share a 15-second deadline and reconnect after expiration. An expired response cannot overwrite a newer observation. A missed poll marks current work unconfirmed instead of leaving a permanent spinner. Local log Open/Save actions do not wait behind status polling, and their outcomes remain in host state across tab navigation. Empty logs open an explanatory document; saving still opens the file picker and can save an empty file. Large captures stream through native temporary files rather than a bounded stdout IPC buffer. Known captures are cleaned on close/dispose and abandoned-session cleanup; unknown files are preserved.

The raw diagnostic stream now includes bounded setup, connection-check and polling/action error chains as well as job failures. These local details can contain paths; metadata reports remain separate and nothing is uploaded. Each logical diagnostic stream uses FIFO segments: an append that would exceed 200,000,000 bytes first removes oldest complete segments until the resulting stream is at most 100,000,000 bytes. Older entries are unavailable afterward.

## Lightweight status and detailed diagnostics

The unified catalog has **27 tools**. `status {}` reads published scalar metadata: counts, exact publication, source freshness, discovery and semantic readiness. Every response mode stays lightweight. It does not load imported fact payloads, hash executable images, request detailed job/worker observations or start an editor adapter. Its unified request has a two-second budget; expiration reports readiness as **unknown**, not failure or readiness of the index.

`diagnostics {}` provides the former detailed view: published coverage, process/runtime identities, jobs, queue, worker progress, watcher/wait observations and optional Explorer attachment. It can take longer and return more data. Overview and Management explicitly use this detailed path. Both tools are read-only; neither starts indexing or repair. Tool descriptions and bundled help tell assistants to use status for routine checks and diagnostics for troubleshooting.

## Validation

Local Windows validation:

- Full workspace tests with all features and doctests passed for the repair. The subsequent compact-status/cache refinements passed the focused MCP suite (45 tests), native direct-client/cache tests (6), CLI/MCP integration (9) and CLI tool contracts (3).
- Workspace Clippy with all targets/features and warnings denied, formatting, repository/whitespace checks and synchronized 1.6.1 versions passed.
- Extension tests passed: 392, with 14 native-environment cases skipped in the ordinary suite. Seven installed-runtime smoke scenarios then passed against the final pair, including real cached status, late registration, ambiguous-source refusal, editor/evidence isolation and reviewed drafts.
- Python contract suite passed: 186 tests, four environment-dependent skips. Native paired-build and package verification preserve both bundled INT8 models and the lightweight main executable.
- Native client qualification passed for two isolated projects with offline inference and six protocol connections across Codex-, Claude Code- and Copilot-style handshakes. Source bytes were preserved. Retained failure receipts and raw logs matched.
- Native uninstall tests exercised owned live clients/workers/launchers, foreign-process preservation and failed relaunch after executable removal. Detached-worker tests covered the five-second hook deadline and extension-directory removal. Upgrade policy tests reproduced the original 1.6.0 refusal and pass for the shipped target.

A small local installed-runtime fixture measured a 645-byte basic status JSON body (about 1.4 KB including the MCP text/structured envelope), 3.7–4.3 ms cache hits, a 29 ms uncached warm check and a 1.2-second first check. The detailed diagnostics body was about 7.6 KB. These are local fixture observations, not Bookbuilder or remote-machine latency qualification. First-use runtime verification remains intact.

The guidance plan remains documentation only. No hosted CI, publication, macOS/Linux execution or complete VS Code interactive uninstall/upgrade qualification is claimed for this candidate. The VSIX is for the requested manual Windows test.

Successful lightweight status snapshots are cached in memory for less than five seconds per native client/project. The result includes the responding AIRK version, basic health, collection/scope, index counts and exact publication token. Source and semantic states omit pipeline/selection digests and detailed coverage metadata. observed_unix_ms, cache_age_ms and cached identify the observation and cache age. Cache hits recheck registration and registry incarnation; unknown/error outcomes are never cached. Diagnostics and contradictory/newer evidence observations invalidate the cache. Cached status is not live-source verification.
