# D8: repository profiles, revision 1

The nine P6 profiles use extraction contract 1, mapping 1, chunk strategy 2 and adapter version `repository-facts/1`. Existing unaffected profiles retain their identities. Publication, inventory, payload and lineage formats are unchanged.

Explicit policy rules address collection paths, including `@alias/` paths. Default repository conventions are evaluated relative to each registered source root. Only direct children of `.github/workflows/` select Actions; nested lookalike directories retain YAML. Compose filenames, `CMakeLists.txt` and `Makefile` are case-sensitive conventions. Ordinary supported suffixes retain the existing case-folded selection behavior.

## Parser provenance

All six added packages are exactly pinned in Cargo.lock and share Tree-sitter 0.25.10. Generated parser ABI versions are compatible with that runtime. Actions and Compose reuse YAML 0.7.2 and include that grammar identity in their profiles. MSBuild uses the existing bounded Quick-XML dependency.

| Package | Version | ABI | Native scanner | License |
|---|---|---|---|---|
| tree-sitter-hcl | 1.1.0 | 15 | C | Apache-2.0 |
| tree-sitter-dockerfile-updated | 0.2.0 | 14 | C | MIT |
| tree-sitter-proto | 0.4.0 | 15 | None | MIT |
| tree-sitter-graphql | 0.1.0 | 15 | None | MIT |
| tree-sitter-cmake | 0.7.1 | 14 | C | MIT |
| tree-sitter-make | 1.1.1 | 14 | None | MIT |

Registry archive checksums are retained in Cargo.lock. Dockerfile/Make omitted standalone license texts; exact upstream revision files and SHA-256 digests are pinned in [license sources](../third_party/licenses/sources.json). Generated notices contain 436 dependencies and the four pre-existing separate-text review notes.

## Evidence and continuity

HCL, Dockerfile, Protobuf and GraphQL expose selected written syntax, exact UTF-8 byte/line locations and conservative declaration continuity. Complete borrowed names are fingerprinted before bounded display truncation; unknown documentation/body/signature dimensions stay unknown. Docker stage qualification can apply to sibling instructions, but persisted syntax owner IDs require an enclosing span. A stage does not manufacture an enclosing syntax node.

CMake, Make, MSBuild, Actions and Compose emit `StructuralRule` evidence with producers `airk-build-structure/<profile>`. They advertise no declaration continuity. Build expressions, dependency spellings, commands and conditions remain unevaluated. No references are resolved, no actions/builds run and no external content is fetched.

The bounded YAML view preserves duplicate entries and original spans. Unsupported aliases/anchors/tags/merges, duplicate keys, malformed shapes and multiple documents are explicit partial diagnostics for Actions/Compose. Docker CRLF continuations and Make custom recipe prefixes remain partial syntax. Supported LF multiline and CRLF single-line locations are tested without source normalization. See [format limits](formats.md#repository-and-build-formats).

## Qualification

Native Windows and Linux-container ingest unit runs: 109 passed, one intentionally ignored subprocess helper. Two build-format integration cases exercise all five structural profiles plus additional-root selection. Four repository-syntax integration cases exercise each grammar, exact original bytes/spans, bounded facts/key reads, warm reuse, compatible moves, isolated policy changes, and continuity/citation preservation after line insertion. All passed, using focused correction receipts where initial fixtures violated the public query limit or used hydrated artifacts as retained-source references.

The initial Linux run compiled the added native grammars and ingest tests in 43.77 seconds using the existing dependency cache; the subsequent integration compilation took 39.14 seconds. Windows focused integration compilation took 31.79 seconds. These are incremental build measurements, not clean-build guarantees. Windows all-target warnings-denied clippy passed. Repository and license checks passed. Whitespace checks pass for original implementation files; the exact upstream Dockerfile license retains its extra final blank line, which `git diff --check` reports. Its pinned bytes are preserved. Release package size and clean-build comparisons remain part of P12; no native macOS or independent native Linux packaging qualification is claimed here.
