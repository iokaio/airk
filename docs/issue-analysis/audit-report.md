# AIRK 1.8.7 (VS Code extension): short audit

Date: 2026-11-03. Tested: v1.8.7, darwin-arm64, VS Code 1.143.1, macOS 26.4.1 on Apple Silicon. Both the GitHub release package and the Marketplace package were looked at.

## Short verdict

Nothing suspicious found. The code looks like careful, honest engineering and the search engine works very well. The Marketplace build of the extension works end to end in the editor. Two distribution problems were found: the GitHub release VSIX does not start on this macOS (#66), and the Marketplace package is a different, ad hoc signed build from the signed release asset (#67).

## What was checked

- Static review of the repository (about 1,600 files): no network calls in the extension code, no HTTP client among the Rust dependencies, the build script downloads nothing and pins model hashes, workflows use minimal permissions, npm packages come from the official registry.
- A scan aimed at malicious repositories (auto-run on open or install, obfuscation, wallet and browser targeting, exfiltration endpoints, suspicious dependencies, hidden Unicode, AI-agent traps): no critical or high signals, 20 medium items that all have an explanation.
- GitHub release: VSIX hash matches SHA256SUMS, release.json commit equals the v1.8.7 tag, binaries signed by Ioka LLC (Developer ID) and notarized.
- Marketplace package: different binaries (different hashes), signed ad hoc only, not notarized (#67). Its contents were not reviewed line by line beyond the manifest and signature.
- The extension's own 560 unit tests: 530 pass, 0 fail, 30 skipped (they need a built native binary).
- The engine on a fake project with traps (headless), then the Marketplace build in a real VS Code window: Restricted Mode, setup, Workbench search, Explorer search, all eight Explorer views, 13 palette commands, MCP server list.

## What is good

- Secrets: 8 fake secrets (.env, id_rsa, .pem, node_modules, a gitignored file, target folder, a file behind a symlink pointing outside, a NUL file) never reached the index, also not through the editor search. Files added later and a swap to an outside symlink stayed safe. Ordinary JSON such as credentials.json is indexed, which the docs say openly.
- Search quality: five languages, Word, PDF, Ukrainian with word endings, CRLF, BOM, a 5.6 MB log and a 1.2 MB line were found with exact citations (file and line range), in the CLI, the Workbench and the Explorer.
- Honest UI: the Explorer says "partial hybrid, lexical while vectors are pending" and "lexical results are ordered by index identity, not relevance".
- Editor integration: setup shows a clear review dialog of what it will change (runtime, indexes, PATH block, folders), the PATH edit is a clearly marked block written to .profile, .zshrc and .zprofile, no login startup is enabled, no network connections were seen, all 8 Explorer views render, the MCP server appears in the VS Code list.
- MCP server: 28 tools, clean errors on malformed JSON, invalid UTF-8, deep nesting and path tricks, except the 65 KB case (#63).
- Engineering habits: SBOM and licence inventory in every release, written release verification steps, honest SECURITY.md, a large test suite, an issue template, a maintainer who answers.

## Growth zones

1. Distribution: the release VSIX is killed by macOS 26.4.1 at launch (#66) and the Marketplace package differs from it (#67). What users install should be one signed, notarized, verifiable build.
2. Messages: a raw internal code in a dialog in Restricted Mode (#65), a bare "No such file or directory" without a path (#61), "exit code 1" instead of the real cause during setup. The "From an older index" label on fresh results is already reported as #59 and reproduced.
3. Freshness: edits wait behind the embedding job (#64). Embedding runs at about 16 vectors per second on one core, so a large repository takes hours to be fully semantic, and the first index of a small project keeps one core busy for minutes.
4. MCP silently exits on a single message over about 65 KB (#63).
5. Docs drift: a help line mentions an option that does not exist (#62), the headless quick start does not mention creating the data folder.
6. A very young project: created in October 2026, one maintainer, 12 Marketplace installs, no track record yet. The native crates contain a lot of unsafe code (airk-cli 173 spots, airk-store 118, airk-process 42), which deserves a focused review. The TypeScript is written in a very dense style (several statements per line), which slows review.
7. The extension keeps its own files outside any redirected folder: even with a fake home and a redirected state folder it wrote into ~/Library/Application Support/AIRK (launcher, data, uninstall record). Fine for a normal user, but worth knowing for testing and for locked-down machines.
8. The bundled embedding model says "qualification pending" with no verified platforms, so vector quality is not proven on Apple Silicon. The panel text says 27 tools while the connection check reports 28.

## Second pass on the Marketplace build (deeper checks, no new bugs)

- Odd queries in the Workbench were handled cleanly: empty query ("query is empty"), a 5,000 character query (size error), an unclosed quote ("unclosed quote"), emoji, a path trick (`../../../etc/passwd`, nothing found), upper case (found), `*` wildcard (no wildcard, all words must match).
- Live edits: a changed line was searchable 8 seconds after saving; new files, deletions and renames also showed up. Pause and Resume work.
- Connecting an agent (Claude Code): a clear guide page first, nothing written until you press Install. With a pre-existing `.mcp.json` the other server, its env and an unrelated top-level field were all preserved.
- Uninstall: the extension recorded what it owns, and the cleanup removed its entry from `.mcp.json`, stopped the service, removed all three PATH blocks, kept the project sources and index data, and wrote a result file with recovery notes. VS Code did not run the uninstall hook during a short automated restart (inconclusive); running the same hook by hand completed in about 18 seconds.
- Small notes: the `.mcp.json` entry contains absolute per-user paths (home folder, a path under VS Code's global storage), so a committed file will not work for colleagues and exposes the user name. A CLI error for an unknown project ends with an internal-sounding "Query returned no rows". The "From an older index" label (issue #59) reproduces on fresh results. The service keeps running after the editor closes and even after its folders were deleted, which is documented but surprised me while testing.

## Not covered

Windows and Linux, a full line-by-line source audit, long-running behaviour (days), the "native execution off" switch, the Customize path of the setup dialog, multi-root workspaces, a large repository.

## Bugs filed

Seven issues on github.com/iokaio/airk:

1. https://github.com/iokaio/airk/issues/61 register fails with an unclear error when the data folder is missing
2. https://github.com/iokaio/airk/issues/62 `airk files` help promises `--prefix`, which does not exist
3. https://github.com/iokaio/airk/issues/63 `airk mcp` exits silently on messages over about 65 KB
4. https://github.com/iokaio/airk/issues/64 new and edited files are not searchable while a vectors job runs
5. https://github.com/iokaio/airk/issues/65 raw error code in Restricted Mode (with screenshot)
6. https://github.com/iokaio/airk/issues/66 release VSIX: service killed with SIGKILL on macOS 26.4.1
7. https://github.com/iokaio/airk/issues/67 Marketplace build differs from the release asset (ad hoc signed)

Already reported by someone else and reproduced here: #59 ("From an older index" label on fresh results).
