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
