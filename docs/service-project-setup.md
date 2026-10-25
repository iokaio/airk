# Reviewed project registration and launcher binding

Current onboarding follow-up (October 25): trusted VSIX activation automatically prepares and submits registrations for new workspace folders using native defaults or User storage selection, then queues model setup and indexing with automatic watching. The native canonical-path, reservation, isolation and trust checks below still apply. Existing saved 1.4 bindings are retained without changing their storage, watching or index work. Manual storage changes still show an exact review. The dated checkpoint and artifact results below describe the earlier manual-only setup.

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
