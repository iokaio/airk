# Native journey fixture compatibility — 8 November 2026

**Historical migration harness contract and evidence record.** The user approved
mandatory fresh-state reset for every pre-1.8.16 installation later on
8 November 2026. The imported-legacy and reviewed-catalog preserved-index journeys
below are prior experiments, not current requirements for transferring old index
state. Keep their failed fixtures and exact receipts. Current acceptance must
qualify the [clean-break lifecycle](../upgrade-rebuild-fallback.md): authenticated
source/preference capture, fresh authority and indexing, source preservation,
client reconnection and exact ownership-checked cleanup. Compatible-line
1.8.16+ upgrades retain their separate state-contract tests.

The original unpublished 1.8.14 matrix follows. Packaging schema identifies
executable topology, not registration authority; those ownership distinctions
still matter when collecting supported reset inputs or retiring old artifacts.

| Packaging and authority | Transport | Setup owner | Original evidence scope | Expected migration/retrieval authority |
| --- | --- | --- | --- | --- |
| Schema 1, legacy authority | Managed launcher / stdio, main plus helper | Native raw registration | Exact original service-project registration | `imported_legacy_snapshot` / `sealed_imported_snapshot` |
| Schema 1, catalog authority | Managed launcher / stdio, main plus helper | Native reviewed catalog setup | Exact catalog session, namespace, Index, Projects and grant revision | Preserved reviewed grant / `user_reviewed_project_grant` |
| Schema 2, catalog authority | Single main / HTTPS | Native reviewed catalog setup | Exact catalog session and native HTTPS enrollment | Reviewed grant / `user_reviewed_project_grant` |
| Unknown or crossed contracts | Unsupported | Refuse before service start or installation | No inferred scope | No inferred provenance |

Catalog authority also requires managed activation: a schema-1 catalog runtime
started through bare `serve` initializes a different authority state. The
independent review found this remaining startup dependency after the setup fix;
startup, control-directory selection, identity checks and Stop must share the
explicit lifecycle owner. Historical catalog transport remains stdio. General raw
service fixtures retain their foreground owner, while the installed legacy
producer separately requires original managed stage/activation and launcher.

Source-bearing preparation is an additional contract: source/model inputs must
use the predecessor's verified native preparation owner. Packaging or catalog
authority alone cannot establish its supported protocol. The installed legacy
producer creates fictional text and uses native registration and Index; it does
not prepare source-bearing model configuration locally. Modern source changes
continue through `reviewed-source-fixture.py`; that protocol is not automatically
qualified for historical catalog runtimes.

The first regression ran the real shared `Fixture.register` with schema-1 catalog
compatibility. It failed because `prepare-registration` was selected. One selected
test failed in 0.002 seconds; retained evidence:
`artifacts/local/one-airk-1.8.14/fixture-compatibility-red.txt`.
Passing synthetic producer bookkeeping previously missed this decision because
it replaced the entire Fixture, registration, indexing and original read.

## Chain reviewed before native acceptance

Registration uses authority; indexing, pause and Stop use the native management
owner and exact registration, independently of the evidence grant. Original
configuration must invoke the installation launcher with
`runtime launch --install-dir <install> --`, then the extension's pinned original
scope, then `mcp`. For imported legacy that scope is
`--root <root> --data-dir <data> --collection <collection> --service-project <binding>`.
This agrees with `packages/vscode/src/model.ts` and the owned-entry recognizer in
`agentMigration.ts`. Bare arguments and project-first extension declarations
would move the failure downstream. Production recognition must not be widened
to accept a broken fixture.

The original read runs before upgrade against that exact saved configuration.
Upgrade, inactive catalog readiness, predecessor refusal and retirement retain
the complete native receipts. Editor enrollment must preserve the original
registration, configuration digest, namespace, upgrade ID and plan digest;
retrieval must prove sealed imported provenance. Launcher and predecessor image
retirement, exact selected-runtime Stop and subsequent unavailable health remain
separate assertions. No restart, retry widening or synthetic cleanup qualifies
these boundaries.

The existing 1.8.12 reviewed-catalog helper remains `reviewed_fixture` in
`check-runtime-upgrade.py`. Shared evidence arguments must use its catalog session;
administrative Index/configuration arguments retain the returned backing
registration. Schema 2 continues to select HTTPS and in-process inference;
schema 1 continues to select stdio and the helper. These topology decisions must
not change when registration authority changes.

## Retained artifact audit and acceptance status

Primary review traced the full chain and shared callers. The independent
`predecessor_audit` reviewer authored none of the harness correction and cleared
the final diff after identifying the startup and unsupported-workload findings;
both findings were corrected and gained process-I/O regressions. The separate
`chain_review` investigator authored journey/admission tests and identified the
launcher-order/full-registration defects; it is not counted as the independent
final reviewer.

Final scoped Python checks selected **156**, with **155 passing / 1 existing
skip**, in **8.818 s** test time (**9.085 s** wrapper). All **107** CJS harness
tests passed in **4.940 s**. Formatting, repository and whitespace checks passed.
The full Python selection ran **865** tests in **77.583 s** but reported **5
failure events / 3 error events / 13 skips** under the sandbox identity. The
affected PowerShell TLS fixture failed before sending a request because Windows
SSPI returned `No credentials are available in the security package`; the Git
tree test failed on the ownership mismatch. Both affected modules passed all
**22** tests in **33.355 s** under the contributor identity with command-scoped
Git configuration. The original full-suite failure remains retained; this is
not a claim of a clean full-suite run under that identity.

The separate freeze is
`artifacts/local/one-airk-1.8.14/completion-fixture-compatibility-freeze-r2.json`:
harness **251 inputs**, SHA-256
`8065142aaaaf8bf0e898cc5ddb14df05b791951790d68329677d46a7717a78b1`.
Read-only verification checked all **811** original r26 runtime seals and the
original VSIX hash. Native production inputs are unchanged; only the already
recorded two external test modules differ from the compiled r26 source. No
runtime build or acceptance run was performed. Full Clippy/workspace/native and
platform gates were not repeated for this Python/CJS/documentation correction.

The earlier authentic producer audit stopped **before installation**:

- Published Windows 1.5.12 package: install manifest
  `1ec0c84311eab5362e6404205c0ba8aa65b7bf92f7f0b278c76c35d4c80e4651`,
  source `b98e439b8eab96b8825f0786af68a6d6d6a01f349ae7a6ace101d95d3718c20b`
  recomputed from clean revision `d3b80ddf1c9af3acf97eb8f2f97960a7c2d6c3b7`.
  All 763 inventory hashes verified. Raw registration and original installer
  launcher support are present. The follow-up source audit below identified the
  existing candidate-startup schema-8–11 bridge. Its authentic regression has
  since passed; the exact artifact is eligible for the authentic producer
  preflight. The complete installed producer chain remains unqualified.
- Retained 1.7.6 runtime: all 674 inventory hashes verified, but recorded source
  `593480ef…` differs from the recomputed tagged source `18ac0296…` (703 inputs).
  Retained 1.8.7 source identities `84b59dcb…` and `015c322a…` likewise have no
  matching reviewed clean revision among the audited candidates. Stable payload
  bytes do not prove those unmapped local builds' contracts. This bounded audit
  does not establish that no matching revision exists elsewhere.
- Reviewed catalog 1.8.12: install manifest
  `be73a42fb700d058dd4b2719a672d82c7a82ad9e0e5eb527529c2f23ac4ae30a`,
  source `51536a705cf9cbabe4947cacddf43a8cb29b0af8bad4e23ca7f293aa6dd347fa`,
  historical contract-review revision
  `3c6e81cf5911751aac8dbe6767ea77abab11d206`. Its recorded build source is stable
  but does not equal the recomputed clean revision (`349d5884…`, 978 inputs).
  The retained stopped fixture's actual authority-1 state and the existing
  reviewed-setup evidence support this artifact's catalog classification;
  clean-source correspondence is not claimed. It remains the separate
  reviewed-catalog case, with catalog-session scope and reviewed-grant authority.

The exact published 1.5.12 artifact now meets the reviewed source/artifact
prerequisite for a producer preflight through its canonical startup bridge. A
standalone pre-service binary, an unreviewed local source seal or catalog 1.8.12
does not satisfy it. The harness admits only the pinned manifest/source pair;
changed bytes or version labels cannot inherit that admission. Producer success
remains a separate observation.

The original r26 runtime and VSIX retain their original versions, hashes and
compiled-source identity. Harness changes require a separate freeze; unchanged
production inputs do not require rebuilding them. The failed 1.8.12 producer and
its Stop evidence remain retained. No new native producer or editor acceptance
is implied by this matrix or the cheap regression. Artifact/source audit,
combined primary and independent review, T0/T1 and a separate harness freeze
must precede one authentic producer preflight; the six editors follow only its
success. All expensive results belong in the implementation ledger when collected.

## Completion follow-up: ordinary startup bridge

The first bounded audit stopped at the catalog reader's schema-11–13 check.
That omitted the preceding candidate-startup path. In
`runtime_activation::resume`, the candidate probe precedes the first
`catalog_upgrade::begin_if_enabled` call. `service_runtime::serve_with_idle`
opens the canonical registry before its supervisors/listener; ordinary
`ServiceRegistry::open` accepts schema 8 and runs the existing setup/INT8,
project-removal and audit migrations through schema 11. The subsequent catalog
reader's schema restriction therefore does not by itself exclude managed 1.5.12.
Checkpoint preflight uses the saved reader, which accepts schema 8 and captures
the tables that are actually present.

The new `service_predecessor_schema8_tests.rs` regression independently constructs
the original registry DDL and completed owner from reviewed clean revision
`d3b80ddf1c9af3acf97eb8f2f97960a7c2d6c3b7`; it does not relabel a current schema.
It exercises saved reads and checkpoint capture before canonical startup,
schema-8–11 migration with the original namespace, paused registration and watch
settings, the pending INT8 setup intent, and catalog admission/migration/restore.
The primary Cargo owner ran the authentic bridge regression successfully:
**1 selected / 1 passed**, within a scoped **8/8 passing** selection. It does
not qualify source-bearing configuration import,
installed process ownership, original retrieval, sealed imported authority or
readiness; those remain the authentic producer's job.
The retained receipt is
`artifacts/local/one-airk-1.8.14/completion-recovery-store-t0-r1.json`, with its
log and JUnit siblings: **839 excluded**, **0.519 s** tests / **69.049 s** wrapper.
The store inputs were frozen for this T0 development check; other crates were
being edited, so its global `native_inputs_unchanged=false` is retained and the
run is not a frozen native qualification.

Independent `recovery_design` source review compared all 14 retained SQL literals
with that clean revision and reviewed the original initialization order and
completed-owner checksum. It found no blocking source issue. The fixture's
nonexistent source/data paths and `PendingMetadata` plan deliberately limit it to
the registry/checkpoint bridge and catalog Admission/Imports/restore; they provide
no retained-publication or installed-producer evidence.

The exact manifest/source pair above is now classified
`migration_path=audited_imported_legacy`, meaning source-audited preflight
eligibility, not successful installed migration. The classifier test exercises
that exact tuple and refuses changed manifest/source/version and an unmapped
local 1.5.12 tuple. The full `test_service_clients.py` module passed **45/45** in
**0.377 s**. Read-only inspection of the retained published
`artifacts/releases/1.5.12/published/airk-1.5.12-win32-x64.vsix` reconfirmed the
original manifest/source pins. The original audit's 763 verified payload seals
remain its separate inventory evidence. No native producer was started by this
follow-up; review and a new candidate/harness freeze still precede that T2 run.

### Alternate historical schema-11 candidate

A further read-only audit on 8 November identified clean historical revision
`44cce94dc296d47c4d9e2e7009f19500b0bf7f6f` (release 1.8.2) as a concrete
candidate for resolving the prerequisite without substituting catalog authority.
At that revision, `main.rs` dispatches `service prepare-registration` to the
original native registration owner; `service_registry.rs` invokes
`background::audit::migrate`, which raises `user_version` to 11. There is no
`service_catalog.rs` in that revision. `runtime_install.rs` retains managed
Stage/Activate and the original installation-launcher forwarding command. These
are source observations, not a claim of migration or installed acceptance.

The retained 1.8.2 runtime under
`artifacts/installation-review-1.8.2-20261101/runtime` records source
`ac65496e944b032928e3fb1b584efea777a013d4043fd3d3db5bfcf2c40f9a4c`
(737 inputs). Recomputing the historical builder's exact source path selection
from clean Git blobs at `44cce94` yields
`119db61c065201a6b5b68164df6e3612d146c59e80afa1178c562e219a521a64`
(737 inputs). The corrected staged copy records the same unmatched build source.
Those retained payloads therefore remain unverified for this journey; matching
file counts or release versions do not establish source correspondence.

An alternate route, if a concrete blocker disqualifies 1.5.12, is a separately
sealed managed predecessor built from
that exact historical revision in an isolated checkout, with its own historical
builder and lockfile. Before its build, review native registration, Index, pause,
launcher scope forwarding, activation, Stop and the current schema-11 import
reader together; run their applicable scoped cheap checks and record a source
freeze. Run historical `python scripts/build-service-runtime.py --destination
<fresh-predecessor-runtime> --profile debug` only after the review gate, through
the single build owner with retained Cargo timing evidence. Verify the complete
resulting payload inventory and source seal before adding an exact reviewed
artifact contract with `migration_path=audited_imported_legacy`. The authentic
producer must still establish original retrieval and the complete migration
chain; this route neither grants that eligibility in advance nor replaces its
preflight. The source audit did not start a service or modify old state.

The existing producer/consumer guard regressions were rerun with
`python -m unittest discover -s scripts/tests -p test_fixture_journey_contract.py -v`:
**4 selected / 4 passed**, **0.564 s**. No T2/T3 run or predecessor build was
performed for this audit. The authentic producer preflight is now eligible on
the exact reviewed 1.5.12 artifact; no historical rebuild is necessary solely
because the original registry uses schema 8.
