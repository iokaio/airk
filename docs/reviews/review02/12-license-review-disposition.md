# Record release disposition for dependencies missing standalone license texts

The dependency notice generator preserves publisher license declarations when upstream packages have no standalone license text, but the repository still flags three packages for release review. A successful notice-generation step does not close those recorded exceptions.

## Current evidence

Source: `c62c1e6936500801dbf41dffdd6e21453e48335d`, on 2026-10-30. [Third-party notices](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/THIRD_PARTY_NOTICES.md#L23) identify `pdf-extract`, `adobe-cmap-parser` and `type1-encoding-parser` as unresolved standalone-text cases. Pinned declaration provenance already exists. The historical `htmlescape` record should not be mistaken for a current dependency exception.

[The generator](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/scripts/license-notices.py#L93) fails on missing license declarations, but writes a declaration record and review message when standalone text is absent. The issue is incomplete release disposition, not demonstrated license incompatibility or an assertion that absence of a separate file makes redistribution unlawful.

## Proposed acceptance criteria

- **AC1:** Each of the three current packages has a dated maintainer release disposition identifying the exact version/source and evidence considered, or an explicit unresolved blocker/action.
- **AC2:** Any recovered upstream text has provenance and integrity checks; no substitute copyright grant is invented or copied from another project.
- **AC3:** Generated notices and release review records distinguish completed dispositions from unresolved cases, and a changed dependency/version triggers re-review rather than inheriting an unrelated approval.
- **AC4:** Documentation and release checks agree on whether unresolved cases block publication or require a recorded exception. Choosing that policy remains the maintainer's responsibility.
