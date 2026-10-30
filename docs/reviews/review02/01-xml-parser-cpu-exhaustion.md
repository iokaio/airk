# Bound XML attribute processing and resolve RUSTSEC-2026-0194

Draft for private security reporting under `SECURITY.md`; maintainer review required before public disclosure.

AIRK's XML and Office extraction paths reach the attribute iterator affected by RUSTSEC-2026-0194. A document admitted for indexing can consume excessive parser CPU, delaying indexing and potentially causing a project worker to be restarted.

## Version, environment and evidence

Reviewed source: `c62c1e6936500801dbf41dffdd6e21453e48335d`, the 1.7.2 development candidate, on 2026-10-30. This is static call-path analysis and a lockfile audit, not a reproduced exploit or a claim about a published release.

`Cargo.lock` resolves `quick-xml` 0.39.4. The [upstream advisory](https://rustsec.org/advisories/RUSTSEC-2026-0194.html) describes quadratic duplicate-attribute checking and lists 0.41.0 or later as patched. [OOXML validation](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/crates/airk-ingest/src/documents.rs#L396) and [XML/SVG facts](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/crates/airk-ingest/src/documents.rs#L879) iterate attributes without a per-element count limit. The separate MSBuild adapter already limits attribute counts and should not be described as equally exposed.

The [attribute helper](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/crates/airk-ingest/src/documents.rs#L780) is another iteration path to cover. Existing file-size, nesting and fact-count limits do not directly bound this work. The service's approximately 600-second no-progress watchdog and 30-second drain grace limit a stalled attempt; they do not provide prompt parser rejection. Post-watchdog retry and restart handling for an unchanged input was not established by this review. No code-execution or data-disclosure consequence was established.

## Expected behavior and verification

Indexing an admitted document should have bounded attribute-processing work and report a useful extraction failure when limits are exceeded. Maintainers should verify this with small synthetic fixtures in an isolated test environment; no weaponized document is attached to this draft.

## Proposed acceptance criteria

- **AC1:** The resolved XML parser is outside the affected advisory range, and the dependency update passes the relevant XML/SVG/Office extraction tests.
- **AC2:** Every affected attribute-iteration path has a documented work budget that rejects excessive input without disabling duplicate-attribute validation.
- **AC3:** Synthetic boundary and over-budget tests demonstrate bounded failure and continued indexing of another valid document, without relying on the service watchdog.
- **AC4:** Extraction diagnostics distinguish a budget rejection from successful or empty extraction; ordinary documents retain their facts and source locations.
- **AC5:** Repeat-index and service-restart tests with the same unchanged over-budget fixture demonstrate bounded extraction failure and continued progress for other sources without recurring watchdog exits. Record the observed retry behavior; this criterion does not assert that the current version has an infinite restart loop.
