# Storage formats and release versions

Storage compatibility has its own identifiers. AIRK release versions such as 1.5.9 describe the executable and extension; they do not name an index format, authorize deletion, or require a new index directory.

| Component | Compatibility identifier | Fresh native default leaf |
| --- | --- | --- |
| Runtime installation | Selected executable version and verified payload digest | `runtime` |
| Index data | `index-v1` — Index format 1 | `data/index-v1` on Windows/macOS; `index-v1` under the Linux data base |
| Service metadata | `service-v1` — Service-state format 1 | `state/service-v1` on Windows/macOS; `service-v1` under the Linux state base |
| Cache | `cache-v1` | `cache/cache-v1` on Windows |
| Log records | Log format 1 | The service state's `logs` directory, or the native macOS Logs base |
| User configuration | No executable version in its location | `settings` under the native configuration base |

The preview channel appends `-preview` to each default leaf and keeps its separate launcher. Agent definitions continue to use the stable, unversioned launcher. `runtime location` and service health report the numeric format identities separately from the runtime version. Management displays the index format in Storage and the service details; diagnostic exports retain both identities.

Format identifiers advance only for an incompatible storage contract. Internal SQLite schema migrations, JSON record schemas and executable versions remain separately checked. Existing format-140 service indexes are **index-v1**; this naming change does not change their bytes or require rebuilding them.

## Existing installations

From 1.6.4, native installation renames a registered historical default index root from `data/service-1.4` to `data/index-v1` (with the corresponding Linux and preview paths). It drains native owners, journals the exact metadata changes, renames the directory without merging an existing destination, updates project bindings and their service references transactionally, and resumes interrupted migration before service startup. Collection ownership retains its original identity input so published revisions, cleanup guards, settings and index bytes remain valid. Custom data roots stay in place. The extension also updates a saved storage preference that exactly names the missing historical default, so later project registration follows the renamed directory; other custom locations and existing roots are preserved. Historical runtime-install and service-state locations retain their saved absolute locations and ownership records. That pathname is a compatibility alias, not a claim that AIRK 1.4 is running. The historical `collections-1.4`, registry and quota filenames are also compatibility filenames, not public format identifiers. A filename alone never establishes whether data is obsolete.

Fresh installations use the version-independent defaults. Existing default index roots migrate during activation; a completed namespace-bound receipt selects the renamed root on later discovery. Migration refuses occupied destinations, redirects and live workers, and never opens source contents. A durable barrier prevents partial migration from being treated as a ready index. Runtime-install and service-state relocation remain separate from this index-root migration. Internal compatibility filenames such as `collections-1.4` remain unchanged.

## Cleanup and downgrade policy

Superseded executable payloads and obsolete derived index objects need not be retained for a future downgrade. Older software can rebuild its own compatible index from sources. Cleanup still needs proof that the target belongs to AIRK and is no longer selected or referenced by current work or readers. Source files and saved user settings are separate from rebuildable data.

Within index-v1, the complete current-head reference proof protects retained objects. Cleanup removes unreferenced objects and receipt-bound retired index instances in batches of at most 4,096 files. It continues while deletion makes progress; consecutive failures or passes without progress remain bounded. Current-object cleanup requires no additional database/WAL reservation, so a full quota need not prevent deletion. Open readers and capture leases defer deletion. Paused projects remain paused.

Different-format legacy indexes retain their existing offline ownership and retirement checks. This change does not treat arbitrary old-looking folders as disposable or implement blind cleanup of every historical namespace. Unknown ownership or unsupported metadata is reported and preserved for diagnosis.

## Diagnosing excessive file counts

A targeted offline evidence set is preferable to copying millions of objects: retained logs, service registry with its sidecars, collection ownership/active manifests, selected current database with its sidecars, and counts/bytes by directory. A few representative objects can then be requested from the large directories. A random sample without reference metadata cannot prove that files are unused. Keep the original tree intact until the diagnosis is complete.

Local October 28 investigation: the shipped 1.5.9 runtime indexed the local Bookbuilder source into an isolated fresh store: 8,122 files, 65,364 chunks and ten publications in roughly 15 minutes. This does not reproduce the other machine's retained state, establish its failure cause, or qualify semantic indexing. The old remote storage tree has not yet been analyzed. See [raw diagnostic access](service-logging.md).


## 1.6.4 project removal

Registry schema 10 adds minimal completed-removal history and schedules full cleanup for existing inactive registrations. A removal barrier persists until the collection and its saved settings are deleted. Detailed project metadata is then deleted. Index-v1 and service-v1 locations remain unchanged; earlier native executables reject the newer registry schema. Ordinary obsolete-object cleanup continues to preserve active project settings; explicit project removal deletes them.

## Candidate query projection capability

The unpublished 1.8.0 writer admits `query_projection_version: 3` to the existing current owner header before publishing optional metadata. Ordinary SQLite schema 126, service schema 140 and index-v1 stay unchanged. Compatible legacy and capability-1/2 headers remain readable; writers upgrade the owner marker before new projected writes. Missing projections use existing strict evidence. Older readers that reject unknown owner fields refuse the new capability rather than silently ignoring retained page ownership. `StorageFormats.query_projection_records` reports binary support, not selected-publication coverage.

Current file metadata binds indexed-line scalars and optional native fact/relationship pages to the exact retained source, extraction profile and inventory metadata. Verified zero-edge files carry an explicit empty declaration instead of another resource file. Imported pages bind the exact authoritative analysis, manifest and complete source inventory. Global imported ordinals and per-path cursor ranks are separate stored values. Pages retain original fact ordinals and canonical discovery order. Preparation/budget omissions are explicit. Existing reference/replay/cleanup walks retain every declared page; the capability does not enable relaxed ordinary reads.

Projection preparation can use internal current-quota grant record version 2: its stable allocation filename is retained while the exact final transaction scope is atomically rebound. Version-1 grants remain readable. Old deny-unknown-field/version readers refuse these records. Recovery searches only the existing bounded current-grant directory when the final transaction differs from the allocation name. No public index-format or SQLite schema number changes.

Capability 2 also defines an optional version-1 projection summary on root resource references. It declares bounded metadata bytes/object counts for admission, which the reader checks against the authenticated header and actual metadata walk. It is not audit or query authority. Keyed selectors live in immutable bounded resource nodes selected by the existing authenticated component reference. Their data pages, selector nodes, path blocks and rosters share the existing replay/retention owner; no separate database or mutable registry is introduced.

Projection continuation receipt version 2 supports native-edge/import components. Compatible version-1 receipts trigger the required new projection checks while retaining original component dates and start time. The independent ordinary baseline continuation is unchanged. New pending certificates contain bounded hashes/counts and page positions, with no second path cursor; the 16-KiB ceiling remains.

Capability 3 adds optional native contract-2 UTF-8 line indexes under existing file metadata/resources. One bounded record contains per-64-KiB source block newline counts and SHA-256 digests, source/payload/profile identity and exact final-newline/line totals. It adds no per-line array or parallel store. The writer, replay and source/scalar audit derive and compare the complete index; its physical bytes are charged in that same certification pass. It needs no extra leaf continuation. Missing indexes preserve strict fallback. Public selection is not fast-read authority. Compressed current-resource reads enforce the exact declared logical byte bound before decoding.
