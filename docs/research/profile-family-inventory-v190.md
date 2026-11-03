# P3 profile-family inventory for 1.9.0

Date: 2 November 2026. Source baseline: accepted P2b. This inventories design
inputs for P11a/P11b; it adds no effective-profile reporting or new retained profile.

Live contracts: [IndexDetail/adapter capabilities](../../crates/airk-core/src/extraction.rs),
[adapter registry](../../crates/airk-ingest/src/adapters/registry.rs),
[native continuity](../../crates/airk-ingest/src/continuity/mod.rs),
[policy application](../../crates/airk-kernel/src/extraction.rs),
[explicit providers/OCR](../../crates/airk-kernel/src/analysis.rs), and
[P2a reply projection](../reply-detail.md). Those files, not zero query hits,
establish the statements below. Adapter support remains narrower than complete
language/document understanding.

## Three separate observations

P11a reports **adapter support**, **published effective retention** and **requested
reply projection** separately, with metadata basis and current configured-versus-
applied identity. A pending configured change cannot describe the old publication
as migrated. Use existing versioned adapter/profile metadata; no corpus/extraction
scan or repeated diagnostics. A supported/retained family need not have a record
matching this query. Unknown coverage stays unknown. Use the P2b reason vocabulary
where an observation actually establishes it.

New family reporting is not the adapter catalog's broad capability boolean:
`structural_facts: true` does not prove every manifest/date/framework family.
`producer_assertions: false` on a native adapter does not disable explicit imported
analysis. Written symbol IDs are separate from continuity keys. Text/vector policy,
retention and compact replies are independent.

## Legacy semantics to preserve

| Policy | Existing native retention | Continuity / important limit |
| --- | --- | --- |
| Essential (`essential`, Search Only) | No native facts/relationships; saved text/identity remains | No native fine-grained continuity; explicit imported analysis is separate |
| Agent Lean (`agent_lean`) | Exact existing `retains_fact` kind allowlist | No native fine-grained continuity; broad framework/standalone-date families omitted; a manifest-specific kind can be omitted entirely |
| Rich Structure (`balanced`, legacy) | All emitted native fact kinds supported by that adapter | Declaration continuity adapter-dependent; document-unit continuity is not equivalent to Detailed |
| Detailed (`detailed`) | All emitted adapter-supported native fact kinds | Supported declaration/document-unit continuity, with original conservative/partial limits |

Retain missing historical-policy meaning and all existing profile IDs until an
explicit migration. Never relabel Balanced as the new Rich or equate Balanced and
Detailed continuity. P11a first exposes this truth; P11b later adds versioned
file-aware **Lean/Rich** with one selected payload per file. New Rich must retain
the new Lean families. No separate rich object, lazy enrichment, model swap or
ranking change is bundled into that migration.

## Family map and required observations

| Family key | Support/retention basis | Reply / qualification boundary |
| --- | --- | --- |
| `saved_identity` | Authenticated source digest/root/profile/native locations for indexed input | Always keep applicable citations/provenance; equality to live disk needs P4 |
| `saved_text` | Native extraction/eligibility and retained text | Focused/full chunks stay explicit and budgeted; extraction can be partial |
| `lexical_candidates` | Current lexical/token boundary coverage | Text references are candidates, not complete resolved references |
| `vectors` | Independent selected-source/model/pipeline readiness | Disabled/excluded/pending is not native-fact retention; no implicit model setup |
| `declarations` | Adapter definitions plus existing `definition` retention | Minimal omits IDs/details; Rich does not recreate unretained data |
| `imports` | Adapter import support and kind retention | Existing syntax/producer basis; no universal dependency resolution |
| `written_calls` | Adapter call support, call/dynamic_call/inherits retention and edge metadata | Candidate-only; native graph scope stays within the origin index |
| `framework_structure` | Specific emitted kinds, not merely broad structural capability | Agent Lean's allowlist is selective; unknown fine-family support stays unknown |
| `manifest_packages_dependencies` | Effective manifest adapter and its emitted kinds | The current allowlist omits `package_identity`, `package_dependency`, `package_script`, `workspace_member`, `workspace_configuration` and `project_configuration`. New file-aware Lean must qualify the motivating Cargo/package fixtures |
| `requirements_design_decisions` | Eligible document cues and retained requirement/decision/acceptance/architecture/design kinds | Embedded dates/bounds inside a retained fact differ from standalone date facts |
| `headings_sections` | Document/structured adapter and section retention | Keep original hierarchy/locations within existing bounds |
| `standalone_dates_bounds` | Specific date/bound emission and retention policy | Standalone dates are omitted by Agent Lean; do not infer a missing deadline from no rows |
| `tables_rows` | Native document/table family and table/table_row retention | Keep row/column limits, original coordinates and truncation; broader formats need evidence |
| `mapped_regions` | Contract-2 host/child mapping and adapter-specific limits | Region-local spans stay distinct from host coordinates; no full compiler/framework claim |
| `declaration_continuity` | Published native profile plus adapter continuity support | Distinct from a written symbol ID; minimal reply can omit available continuity |
| `document_unit_continuity` | Detailed and supported conservative document-unit metadata | Legacy Balanced/Agent Lean behavior cannot silently become Detailed |
| `imported_assertions` | Explicit source/build/producer-bound import | Independent producer provenance/coverage; no AIRK compiler certification |
| `ocr_text_facts` | Explicit native OCR/provisioning/capture; applicable effective native fact policy | Recognition and layout remain fallible; no automatic OCR or uniformly profile-exempt fact family |

P11a's per-family result distinguishes support (`supported/unsupported/unknown`),
retention (`retained/selected/not_retained/unknown`) and reply inclusion, with
`metadata_only` basis. Actual record counts are included only if already observed
and with their precision; a policy statement cannot invent an observed total.
Unqualified formats/compound children remain on their documented legacy behavior.

## Fixtures required before reporting or new profiles

1. The four legacy policies × Lean/Rich/default replies, including keyed reads,
   compact tables, actual transports and empty matches with supported retention.
2. Cargo/package manifest under current Agent Lean, then the separately qualified
   new Lean: useful package/dependency evidence without broad all-fact retention.
3. Prose requirement with embedded date/bound versus standalone date; Markdown and
   DOCX table rows, partial PDF/OCR, spreadsheets/slides and mapped-host limits.
4. Declaration ID present but no continuity; Balanced declaration continuity versus
   absent document-unit continuity; Detailed partial/unsupported adapter dimensions.
5. Explicit imported analysis and OCR retain their own producer/source/extraction
   meanings, including stale source/build manifests and unsupported evidence.
6. Configured new policy with old applied publication; no-op effective-key reuse,
   single-file edit, quota failure/cancel/restart and no hidden profile migration.

The [contract pack](contracts-v190/README.md) contains original policy/manifest
expectations and gates. These are future acceptance fixtures, not a claim that
P11 reporting/migration or every format matrix has passed.
