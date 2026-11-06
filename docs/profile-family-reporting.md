# Per-file profile family reporting — P11a

The unpublished **1.8.8 working build** includes this locally accepted P11a feature.
Focused native tests, Mac ARM tests, Workbench tests and actual native/CLI/console/MCP
parity checks passed, followed by final qualification and primary review. See the
[dated evidence and limits](research/implementation-v190.md#p11a--local-phase-checkpoint-6-november-2026).
No new extraction profile is introduced.

Opt in on the existing file-information read:

```sh
airk info src/example.rs --include-profile-families --verification off
airk tools call airk_file_info --args '{"path":"src/example.rs","include_profile_families":true,"verification":"off"}'
```

MCP uses `include_profile_families: true` on `airk_file_info` or its public
`file_info` alias. Workbench's File info form uses **Include effective profile
families**: Default omits the input, No explicitly selects false, and Yes selects
true. Default/false retain the old result shape and evidence validator. A report
has its own conditional representation: a validator for old reduced FileInfo
cannot suppress a newly requested report. Existing revision, hash, origin and
follow-move checks still apply. Full/Terse response modes and Lean/Rich reply
choices preserve the complete report;
Rich does not recreate facts that were never retained.

Reporting itself uses one authenticated current reader and the requested file's
retained profile metadata. It does not reopen originals, rerun header selection,
walk the corpus, extract content, load models or invoke producers. Existing
[saved-source verification](source-verification.md) remains independent: the
examples explicitly select Off to request metadata only. A family report is not
a live-file equality, extraction-quality or completeness claim.

## Independent identities and observations

The report binds the exact file path, source hash and publication. Its applied
file `effective_profile_digest` comes from that retained `ExtractionProfile`,
including adapter ID/version and legacy detail. Configured/published collection
`policy_digest` values have a different meaning and different identity framing.
Saving a new collection policy does not reinterpret an old file profile.
Configured effective per-file selection remains unknown when fresh source or
header inputs would be required. Mixed indexes have no universal file adapter.

Authorized partial Project scopes still receive the file report. Collection
policy observations are explicitly `unavailable` with reason `selected_scope`;
`pending_collection_reindex: null` means unknown, not false. Private policy bytes
are not read in that branch. Real I/O/integrity failures retain normal errors.

Each of the following eighteen families independently reports adapter support
(`supported`, `unsupported`, `unknown`), retention eligibility (`retained`,
`selected`, `not_retained`, `unknown`) and actual operation payload inclusion
(`included`, `selected`, `omitted`, `unknown`):

| Family | Qualification |
| --- | --- |
| Saved identity | Retained file/profile/native-source identity, not live equality |
| Saved text | Eligibility does not promise complete extraction |
| Lexical candidates | Written token candidates, not resolved references |
| Vectors | Independent selection/model/pipeline metadata |
| Declarations | Exact versioned adapter support |
| Imports | Written imports, not universal resolution |
| Written calls | Syntax candidates, not resolved call graphs |
| Framework structure | Fine-family support can remain unknown |
| Manifest packages/dependencies | Legacy Agent Lean omits native package kinds |
| Requirements/design/decisions | Independent from standalone dates |
| Headings/sections | Adapter and original-location limits remain |
| Standalone dates/bounds | Legacy Agent Lean does not retain these kinds |
| Tables/rows | Legacy Agent Lean retains eligible table kinds |
| Mapped regions | Needs independent mapping metadata |
| Declaration continuity | Legacy Balanced/Detailed policy remains distinct |
| Document-unit continuity | Legacy Detailed eligibility differs from Balanced |
| Imported assertions | Independent producer/build/source provenance |
| OCR text/facts | Independent explicit OCR evidence and limitations |

Zero matches cannot establish unsupported/not-retained status. Fine support
without versioned registry evidence stays unknown; broad structural capability
flags do not certify every family. FileInfo returns metadata/counts rather than
the underlying facts/text/vectors, so those payload families are omitted even
when eligible for retention. Saved-identity final renderer inclusion may remain
unknown. A partial field selection is not a wholly omitted family.

Missing historical profile detail keeps its existing Detailed meaning. Legacy
Agent Lean, Balanced and Detailed profile IDs are preserved; Balanced declaration
continuity is not Detailed document-unit continuity. OCR/model adapters and their
dependencies, retained evidence and ranking remain unchanged.

## Workbench details, copies and persistence

Structured step details show the complete opaque native report, including unknown
future fields. JSON/Markdown copies and exports preserve it; selected exports
retain the qualifying metadata for their selected source steps and disclose the
subset. Session restoration preserves the original envelope and opt-in form
choice. Saved-result privacy settings may remove an entire result as documented;
they do not silently rewrite family rows.

Automatic Inspector/source-open/checker calls keep absent opt-in behavior. Use the
Workbench File info form for this candidate report. Strategy conversion refuses
unsupported native inputs and directs users to Save export rather than dropping
the flag or reducing the result.

Per-phase checks and actual native transport tests passed as recorded in the
[implementation ledger](research/implementation-v190.md). Renderer mocks alone
do not establish parity. Broader P14 qualification and manual acceptance of the
final test installer remain pending.

## Separate P11b retained profiles

P11b's [versioned retained profiles](extraction-policy.md#explicit-versioned-retained-profiles)
adds explicit Lean/Rich selections while retaining historical profile identity.
The report carries new selected file identity and configured/published
retention separately, including legacy fallback and scoped unavailable policy.
This extends the accepted P11a read contract; it does not reinterpret old levels
or change legacy level semantics. Full/Terse and reply Lean/Rich remain
independent controls. Scoped P11b qualification and primary review passed. P11b is accepted for unpublished local development.
