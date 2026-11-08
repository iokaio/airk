# Index setup and search selection

The **1.8.14 working implementation** retains **Search scope**, **Index library**
and **Index sets**. The local developer owns these indexes; selecting them does
not require creating or inspecting access grants. The extension handles native
scope and access records internally when **Apply selection** is chosen. Native
CLI/API `Project` terminology remains for compatibility and corresponds to the
UI's index sets. See [current agent integration](agent-integration.md); the [1.8.11 record](releases/release-1.8.11.md) documents the earlier UI cutover.

## Create an index

In **Indexing → Index library**, expand **Set up a new index**. Review the primary
source, additional source locations, storage parent and new index leaf,
collection and watching choice. Creation uses the existing native
prepare/reserve/storage/register owner. It does not silently adopt occupied or
unowned storage. Missing parents, aliases and source/private-state overlap remain
explicit failures. Uncertain outcomes retain the original request for recovery.

Opening a trusted folder does not create or start an index. Compatible native
runtime upgrades and capability discovery happen independently and show actual
stages or failures. **Management → Service → Start service** prepares the approved
runtime; **Connect / reconnect** observes an existing installation. Neither action
chooses source folders or applies a search selection. Existing Stop, pause and
watching preferences remain native.

## Choose what to search

Open **Search scope**. Check individual **Indexes**, reusable **Index sets**, or
both, then choose **Apply selection**. Use **Refresh choices** after catalog
changes or a failed read. Checkbox edits stay a draft until applied. The native
host validates the selected current indexes and set memberships and installs the
resulting window search scope. Access records are an internal implementation
detail; users do not copy IDs or manage grants in this flow. The selection does
not silently rewrite an external coding agent's configured scope.

**Index sets** creates and manages named reusable selections of existing indexes
or source scopes. A set does not duplicate stored evidence. **Index library** is
the single place for index creation, source/settings changes, splitting and
**Delete index**. Library and set pages load bounded metadata automatically;
continuations and complete responses remain available. Reading metadata never
creates an index or starts indexing.

## Delete or replace an index

Choose **Delete index** in the library, inspect **Delete this index?**, then confirm
**Delete index and stop maintenance**. Native retirement stops maintenance and
removes current memberships/default references; derived-data cleanup can finish
later. The result distinguishes logical removal from cleanup completion, which
is not observed in that view. Original
source files, shared models and independent indexes remain intact. See
[deletion and cleanup](service-project-retirement.md).

To build a differently shaped independent index over some of the same source
files, use **Set up a new index** with a new storage leaf, collection and backing
identity. A split is optional. The exact retired backing cannot be reactivated or
used to erase its permanent exclusion policy. Source/private-storage isolation
and all new-index setup checks still apply.

Restricted Mode blocks native work. Stale or revoked native scope requires a
fresh explicit selection; imports of legacy snapshots retain their original
connection boundary. Complete read results and exports preserve scope, revision,
coverage and failures. See the [CLI catalog](cli-reference.md),
[storage boundary](storage-formats.md#development-catalog-boundary) and the
[historical P6f ledger](research/implementation-v190.md#p6f--catalog-management-and-scoped-native-activation).

## Connect MCP over the shared service

After applying the native search selection, **Connect Coding Agent** prepares an
HTTP connection with the exact grant/namespace and protected credential
references. Use the actual native `https://127.0.0.1:<bound-port>/mcp` URL; an
occupied preferred port can cause a different persisted endpoint. VS Code uses
`McpHttpServerDefinition`. Connections do not launch an MCP command or helper.

Imported scopes retain their genuine prior configuration/provider receipt and
native grant. Multi-root predecessors retain all original members, without an
implicit default project. Missing lineage, revoked authority or unsupported
client trust/auth formats remain review-required. Saving one client's setup
does not rewrite another running agent's authority.

Provision the exact public issuer through a qualified additive CA bundle or
**AIRK: Trust Local HTTPS Issuer**. Certificate verification remains enabled;
credentials are protected host references, not copied tokens. New-terminal
provisioning does not update running apps or remote/container hosts. Verify the
actual client's initialization, tool list, status and cited read. See
[agent integration](agent-integration.md) and [templates](../examples/mcp/README.md).

## Historical registration checkpoints

Historical onboarding follow-up (25 October 2026; superseded by P6f): trusted VSIX activation automatically prepares and submits registrations for new workspace folders using native defaults or User storage selection, then queues model setup and indexing with automatic watching. The native canonical-path, reservation, isolation and trust checks below still apply. Existing saved 1.4 bindings are retained without changing their storage, watching or index work. Manual storage changes still show an exact review. The dated checkpoint and artifact results below describe the earlier manual-only setup.

**Development checkpoint before the 1.4 default cutover (2026-10-24).** The artifact identities and 1.3.2 development versions below remain historical evidence. The current source/lockfiles are 1.4.0 and the extension/public CLI now use service defaults; see the [current completion checklist](service-implementation-checklist.md).


This is the implemented registration adapter for the [1.4 service plan](airkservice-architecture.md). Native and extension source/lockfiles are 1.4.0 and default to service ownership. The dated validation below retains its earlier artifact versions. Final platform/editor qualification and the VSIX gate remain open.

`airk --root <source> --data-dir <storage> --collection <name> service prepare-registration` returns the exact canonical project registration without creating directories. The source must already exist. Storage may be missing: the native adapter resolves its existing ancestor and rejects traversal, unavailable ancestors and overlap with the proposed source. The review identifies source, data, collection, opaque binding and installed runtime scope.

The trusted host freezes that review and retains its object identity. A copied review, changed runtime binding or lost workspace authorization is refused. After user review, the pinned metadata client submits `reserve_project`. The service validates the planned canonical identity and atomically reserves it using the same bounded registry and source/private-path isolation checks as an ordinary registration. A missing data directory is still a protected reservation: another project cannot subsequently register a source that contains it. Ordinary `register` continues to require existing canonical directories.

Only after that acknowledgment does `service prepare-storage --runtime-dir <control> --project <binding>` read the completed, namespace-checked registry and create the reserved storage directories. It checks canonical identity before and after creation. It never initializes/migrates a registry or copies source bytes. Creation failure preserves the reservation for explicit exact retry; a lost acknowledgment does not authorize selecting another path. The adapter does not delete partial directories or start indexing as compensation.

The extension's saved workspace bindings are separate bounded records (at most 1,024), scoped by installation and project. They contain reviewed metadata, not executable approval or index contents. A workspace root is offered only after explicit review. A redirected root does not silently acquire the old binding. Missing or inaccessible sources may retain their existing opaque binding for retained read-only evidence. Workspace authorization is rechecked around asynchronous native calls and trusted confirmation.

Generated service MCP arguments use the stable launcher:

```console
airk-launcher runtime launch --install-dir <installation> -- --root <display-source> --data-dir <display-storage> --collection <name> --service-project <binding> mcp
```

The native launcher supplies its own installed-runtime context, refuses a competing context override and executes the selected approved image. This frontend checks the runtime family/version and elected service before using the broker and direct project transport. It never opens a local Kernel on failure. The opaque binding preserves retained queries when source paths are unavailable; displayed paths do not redirect that binding. MCP probes use the launcher directory as their working directory and a finite 90-second cold setup budget for installed service entries.

Native Windows/Linux tests cover read-only preparation, source overlap refusal, repeatable storage creation, conflicting sources after reservation, and paired CLI context arguments. Extension tests cover immutable reviews, exact reservation-before-create sequencing, copied review refusal, lost trust, saved workspace scopes, missing-source bindings, trusted console registration and highlighting by native binding. Windows and Linux pass 111 native library and three CLI tests; extension checks pass 194 tests with six skips. Python passes 129 tests with four skips. Workspace Clippy, separately run documentation tests, formatting, repository and whitespace checks pass. Linux uses image `a4bf781772e151db91506dcdeb1a2b273ab814e3605027d876663ce6f2ba55a7`.

The installed fixture passes reviewed registration, creation of its reserved data path, source indexing and stable-launcher MCP reads with sources present and unavailable. It exits successfully after uninstalling its owned runtime and removing its private fixture. It uses frozen development pair 44: source `fc6c455a76f0d53ad90a24066fac913aac17be7dfd4b8501368f6eec377f5f8c` (632 inputs), main `46eea2b9c8647abad19c357ca2624cd62d4dd53fb548babb4d4a75639b2e06b2`, helper `8b2d1b45bc1890a9d5d38d46dfec4c8f5a0c8a26d3620df4252dd21cb4585958`. Final default-adapter, native Mac and installed-editor qualification remain separate.
