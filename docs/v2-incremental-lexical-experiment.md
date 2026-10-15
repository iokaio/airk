# O2b: private incremental lexical experiment

Decision: retain the full production lexical builder. The isolated prototype
shows a useful sparse-edit signal, but its timing omits final publication work
and it does not solve vector row identity or repeated-edit storage pressure.
No active publication path calls `lexical_experiment::prepare`.

The prototype authenticates an existing generation against its receipt and keeps
its reader lease while making independent byte copies of all lexical components.
Each copied component is rehashed against the manifest; symlinks and staging
inside retained generations, objects or history are rejected. The writer opens
only the new private directory. Copies, metadata replacements, new segments and
merge intermediates use the existing charge callback. There are no hardlinks,
active pointer writes, or changes to the original generation.

Exact `InventoryFile` comparison supplies added, removed and replaced paths.
Root/profile/extractor/schema/import changes reject incremental admission.
Vector-enabled generations are ineligible because their vector row identities
need a separate contract. Lexical global ordinals also matter: inserting an early
file, removing chunks or changing chunk counts requires reloading otherwise
unchanged files whose starting ordinal shifts. The prototype explicitly does so.

The original fixture has 16 or 1,000 text files with three chunks each, including
Unicode text. Sparse edits change one file without changing its chunk count;
early insertion adds `aaa.txt`, shifting every existing file. Three sequential
samples were collected on Windows with Rust 1.98.0, debug test profile, one Cargo
build job, on 2026-10-14. The table reports measured medians, not a cross-platform
or release-build performance claim.

| Files / operation | Full preparation ms | Prototype ms | Loaded artifacts | Copied bytes | Cumulative charged bytes | Final lexical bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 16 / sparse edit | 115.011 | 105.149 | 1 | 5,950 | 11,951 | 8,004 |
| 16 / early insert | 114.143 | 104.900 | 17 | 5,950 | 15,175 | 6,163 |
| 1,000 / sparse edit | 1,223.863 | 621.393 | 1 | 180,684 | 187,061 | 183,110 |
| 1,000 / early insert | 1,156.455 | 859.071 | 1,001 | 180,684 | 365,037 | 180,922 |

Full preparation uses the production retrieval writer, including canonical
inventory/manifest writes, verification and a tiny original closed-catalog
fixture. Prototype timing includes baseline authentication and copying, but
produces only private lexical components. It excludes final catalog, lineage,
inventory, manifest and pointer publication. Thus the apparent speedup is not an
equivalent full-publication benchmark. Charged bytes are cumulative allocations,
not measured peak physical usage; final lexical bytes exclude publication
metadata. No production quota saving is claimed.

Two focused tests passed: 36 query combinations across sparse edits, early
insertion and chunk-count/deletion scenarios compare complete serialized
responses with the production reader, including ordering, path filters,
Unicode byte truncation and limits. Quota denial, manifest tampering and unsafe
staging fail without changing the original generation. The explicit ignored
campaign also passed all twelve cases and their differential query checks.

Reproduce:

```powershell
cargo test -p airk-retrieval --locked --test lexical_experiment -j 1
cargo test -p airk-retrieval --locked --test lexical_experiment measure_sparse_and_ordinal_shift_workloads -j 1 -- --ignored --nocapture
```

Adoption requires an equivalent complete-publication comparison, repeated edits
under retention/quota pressure, and a demonstrated benefit after ordinal repair.
The current experiment is lexical-only; it makes no incremental vector-index
claim. Its private temporary output is cleaned when the result is dropped.

Windows retrieval all-target clippy passed with warnings denied.
