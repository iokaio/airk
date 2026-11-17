# Embedded execution and proposal source

This source-only extraction preserves 101 distinct files from 124 authored embedded fields, including aliases. One field resolves to an existing current maintained source owner. The map records each container SHA-256/length, RFC6901 JSON pointer, decoded UTF-8 SHA-256/length, maintained path/hash/length and reversible SPDX prefix. No payload is copied from surrounding receipts. Forty-seven upstream commit patch fields are excluded with the actual upstream commit URL and container/source hashes.

Historical scripts keep exact source bodies, including original host paths, UID checks, input pins, output destinations and timing assumptions. Names carry decoded hashes to distinguish versions. Where absent, only an SPDX comment is added; stripping the declared prefix restores the exact decoded body. These scripts are inspectable historical templates, not authorized operational launchers. Do not execute them directly: some acquire packages, transfer source, launch owned processes, run tests or write receipts. A new run needs a separately reviewed current entry point, same-human identity verification, audited private output/cache parents, current process/resource policy and fresh complete seals. Original outcomes remain historical.

A separately mapped [source snapshot family](snapshots/README.md) preserves 71 additional decoded fields from eight reviewed native/Admin/Workbench containers, using 37 new distinct templates and existing current source owners. Its map and controls remain separate from the 124-field execution/proposal map.

The [supplemental family](supplemental/README.md) adds four rejected-hunk pointers (two exact patches) and one strict base64-decoded Mac audit-token probe. Its direct and encoded maps preserve the additional five fields independently; total preserved source is 200 field records and 141 new source files. No surrounding traceback is copied.

## Families

- [Linux](linux/README.md): source transport, explicit user-space package/toolchain provisioning, static runtime build ownership and source-equivalence probes.
- [Neo](neo/README.md): macOS account/platform/tool discovery, bounded process/evidence observation and guarded transfer receivers.
- [Validation](validation/README.md): retained local test orchestration and selected equivalence checks captured as command strings.
- [Supplemental sources](supplemental/README.md): rejected composition patches and a Python audit-token experiment recovered from a base64 literal.
- [Source snapshots](snapshots/README.md): native diagnostic preimages, removed parity helpers, Admin request extraction bodies and Workbench decoder proposals.
- [Patches and fragments](patches/README.md): AIRK review/proposal patch fields and partial substitution fragments; these are not standalone programs.

## Source-only verification

From the repository root:

```powershell
python -B -S -m unittest discover -s scripts/arbiter/source_rescue/embedded_history -p 'test_*.py' -v
```

The eleven controls parse Python and check maintained bytes/inverse identities without importing recovered programs, invoking models or creating caches. Snapshot controls additionally read the eight original private containers to verify exact pointer decoding; the original four execution/proposal controls require only maintained files. Both apply APIs refuse an existing provenance map before any writes. `extract.py plan --inventory <original-indicator-report>` is the reproducible read-only planner: it verifies container hashes, decodes selected pointers, checks current tracked raw/prefix identities and Git-reachable blob identities, and prints a plan to stdout. `apply` accepts only a reviewed plan on stdin and creates absent paths beneath this folder; its caller must preflight every output parent and audit every created file using the repository human-execution guard. Do not rerun apply over this archive.

## Coverage limits

This is not complete all-artifacts source coverage. The original report marks 2,064 files with source indicators; many are documentation, native read/export results, synthetic fixture input, dataset examples or upstream source snippets. Only explicit executable/source-proposal fields and reviewed patch fields are extracted here. Other indicator fields still require separate provenance classification. The report also records 25,221 invalid-JSON cases, 451 non-UTF8 inputs, 392 size-bound cases and 48,021 permission errors. Those are inspection gaps, not automatic vendor/data exclusions. The source map retains these counts and the original inventory hash. No ACLs, archived outcomes, native APIs or source/input pins were changed.

## Remaining indicator classification

`classify_remainder.py <private-remainder-report>` verifies pinned container/field
bytes and emits metadata-only dispositions. `classification-scope.json` restricts
eligibility to the exact 230 reviewed container paths, hashes and lengths; changed
bytes, substituted pointer text and generic unbound documents remain unresolved.
Rules inspect actual corpus schemas,
read-result/answer envelopes, native projection pages, package/help documentation
fields, or exact maintained/Git source excerpts. Unmatched implementation fields
stay unresolved; filenames alone never close a field. This helper does not copy
fixture inputs, training examples or indexed source text. The reviewed JSON subset
has 1,486 indicator fields across 230 containers. Its historical pending labels
for 16 upstream issue/discussion fields were subsequently resolved by exact member
comparison against the retained, digest-pinned SafeInt 3.0.28 archive; the separate
source-disposition evidence remains in the execution ledger. The 1,718 document/text
containers and invalid/non-UTF8/oversize/access inspection gaps are not closed by
this JSON classification. Classification is source-role evidence, not qualification.
