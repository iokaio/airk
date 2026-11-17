# JavaScript provenance analyzer

`analyze.py` accounts for generated/packaged JavaScript replicas by exact bytes.
It reuses authenticated prior generated proofs and tries current or reachable
Git TypeScript with the installed pinned compiler, entirely in memory. It never
imports historical JavaScript or runs a product build. Unmatched code is listed
for owner review rather than labeled generated from its folder name.

Run with Python `-B`, an exact `--delta`/`--delta-sha256`, and a finite
`--seconds` bound through the verified human shell. Capture stdout to a guarded
private receipt. The analysis needs the retained inputs, Git history, Node and
existing TypeScript package; it downloads nothing. Tests use only pure seams.

The [contract](CONTRACT.md) defines the evidence limits. Source capture or an
exact compiler-output match does not imply runtime, model or release qualification.

The companion capture_standalone.ps1 accepts an exact analysis report and hash,
selects only unmatched historical editor driver/test and help-search sources,
and defaults to a read-only plan. Its explicit -Write mode uses CreateNew under
the maintained same-human output guard, audits new files, and records reversible
SPDX prefixes. It preserves inert .js.in templates in the separate
javascript_history_r1 family; generated bundles and vendor code are excluded.
Run test_capture.py with Python -B -S for exact inverse/roster checks and V8
syntax-only compilation; no historical script is executed.

link_roles.py supplements an exact analysis receipt with source-module labels,
tracked/reachable Git owner identities and pinned build/npm lock metadata.
It observes role and ownership links only: original source versions and exact
bundle regeneration remain unproved. It never runs a bundler or compiler.
Use --analysis PATH --sha256 SHA through the verified shell and retain stdout
in a guarded private receipt. Missing source/vendor links remain explicit.
test_link_roles.py exercises label boundaries without reading historical inputs.

fixture_proof.cjs parses current or reachable Git fixture generators using the
pinned TypeScript parser. It decodes static literals and the reviewed four TAG
prefixes without evaluating source expressions, running an editor or creating
fixture payloads. Its report distinguishes exact reproduced fixture bytes from
older unmatched behavior. fixture_proof.test.cjs exercises this inert parser seam.

The explicit -FixtureSupplement capture mode preserves four identified older
editor test-code variants; -SourcePolicySupplement preserves one historical
policy module. Their standard maps live in fixtures/ and source_policy/ below
the historical-source family. The normal231-path capture remains separate.

disposition_adapter.ps1 takes the exact generation analysis/hash and an approved
private output root. Default mode only reports row/shard counts. -Write creates
fresh standard source-disposition/1 shards (maximum2000 rows and16MiB each),
using the maintained output guard and CreateNew. It includes only exact proved
TypeScript generation rows; role-only observations never become exact proof.
Use the source inventory verifier to consume those private shards, not directory
names or an unchecked success flag. Full alias data remains private evidence.
