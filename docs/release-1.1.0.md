# AIRK 1.1.0 release and qualification record

**Published:** [v1.1.0-build.8](https://github.com/iokaio/airk/releases/tag/v1.1.0-build.8), October 17, 2026 at 02:50 MDT, from `9bb7cce5d94edd8ed7cc4535b8e4edfad720a232`. The native and extension package version is **1.1.0, stable**. See the [combined 1.1.0–1.1.2 summary](releases-2026-10-17.md) for current behavior and upgrade guidance. The preparation checklist below is a historical qualification record, not current publication status or proof of Marketplace availability.

This release raises workstation ingestion allowances and makes limits inspectable in the extension. Automatic eligible-source sizing uses one eighth of detected memory, capped at 2 GiB, with a 512 MiB fallback. The largest automatic profile permits 256 MiB raw files, 32 MiB extracted text per file, 4 GiB extracted text per collection, 100,000 files, one million discovery entries and 8 GiB pass reads. Existing explicit policies remain unchanged.

UTF-8 sources above 4 MiB retain exact searchable text with explicit partial status instead of entering structured parsers. Format/fact bounds, disk quotas, source isolation and atomic publication remain enforced. Coverage adds measured source/extracted byte totals and the largest eligible files; limit failures identify measured totals and the triggering file. See [ingestion limits](ingestion-limits.md) for configuration and remaining parser limits.

The shared inventory ceiling increases from 128 MiB to 512 MiB, including full change-tracking metadata. Real-repository testing exposed the old aggregate limit on a code-heavy workspace. Publication and retrieval readers now use the same ceiling; continuity is retained rather than omitted. Encoded metadata limits do not bound process memory, and larger inventories also consume disk quota.

The branch also retains CI cache fallback and authenticated reuse of signed extension rehearsal artifacts. See [CI builds](ci-and-builds.md) and [release workflow](release-workflow.md).

When a collection budget stops indexing, the extension can offer a reviewed, explicit increase up to the native hard ceiling and retry once. The approval explains memory, runtime and storage costs; cancellation makes no policy change. Saving compares the reviewed policy with current state to preserve concurrent edits. Native per-file and storage bounds remain enforced.

## Upgrade and qualification

Keep the existing extension identity, collection IDs and storage configuration. Use **AIRK: Configure Ingestion Limits… → Use automatic machine sizing** only when deliberately replacing an old explicit policy. Collections without a saved override adopt the new automatic defaults. Revised extraction profiles invalidate earlier extraction reuse and can make the first indexing pass more expensive.

New larger retained objects can exceed the limits understood by 1.0.0. Do not assume an older helper can read newly published 1.1.0 generations; test rollback with isolated or backed-up state. No source files are rewritten. A 2 GiB source allowance is not a 2 GiB storage requirement or a measured performance guarantee. Version 1.1.0 retained the 4 GiB default collection disk quota; 1.1.1 raised new defaults to 100 GiB per collection and 500 GiB per storage root, preserving saved policies. Native parser process isolation and hard per-file CPU/RSS enforcement remain unimplemented.

## Historical preparation checklist

- [ ] Review the `feat/filesizing` PR, including the retained CI enhancements and local qualification results.
- [ ] Pass repository checks and Windows/macOS/Linux CI; record large-file, low-memory, explicit-policy, upgrade and prior-generation preservation results.
- [ ] Merge through the normal reviewed PR process and wait for successful main CI at the exact merged SHA.
- [ ] Rehearse Extension release with `version=1.1.0`, `channel=stable`, `publish=false`. Verify all four final packages and their authenticated receipts.
- [ ] Install a verified candidate and test a 1.0.0 upgrade, configured limits, text-only evidence, storage inspection and MCP retrieval. Record performance and unresolved qualification limits.
- [ ] Publish the reviewed 1.1.0 stable Marketplace packages after authorization and verify all four platforms.
- [ ] Publish matching GitHub downloads from the same main SHA after authorization; verify the downloads page, assets, checksums, signing/notarization and provenance.

Local edits and passing tests do not complete any publication step. Preserve the historical [1.0.0 release notes](release-1.0.0.md).
