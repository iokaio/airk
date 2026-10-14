# Third-party dependencies and design inspiration

AIRK includes no vendored source from Munarium, VCP or `temp/`. The [inspiration record](docs/inspiration.md) describes the design inputs.

Runtime/build dependencies are resolved by [Cargo.lock](Cargo.lock). Important direct families include Serde, Clap, Tokio, the official MCP Rust SDK, Tree-sitter/native grammars, Ignore, SHA-2, Tempfile, Quick-XML, Zip, PDF-Extract, CSV, Pulldown-Cmark and Regex. The exact catalog uses pinned Rusqlite 0.40.2 with bundled SQLite; SCIP 0.10.0 and Protobuf 3.7.2 provide official format bindings, and URL normalizes project file URIs. Each retains its own license terms. No external code-graph engine is bundled.

[scripts/license-notices.py](scripts/license-notices.py) produces a locked dependency inventory with package/version/license/source and copies dependency license/notice files into a release bundle. The Docker build includes that bundle alongside AIRK's LICENSE and NOTICE. Native release packages must do the same. This Markdown page is an overview, not a substitute for third-party license text.

Some registry packages omit standalone license texts. Pinned upstream texts and their exact URLs/digests are recorded under [third_party/licenses](third_party/licenses). The refresh script retrieves only legal records, not implementation source. Where the upstream revision supplies no separate text, the bundle preserves the publisher's package license/author declaration and flags it for release review. Current flagged packages include PDF-Extract, Adobe-CMap-Parser, Type1-Encoding-Parser, Tree-Sitter-Lua and r-efi. Do not call this a completed legal release audit or fabricate copyright notices to fill gaps.

Extension build tools are locked separately in [packages/vscode/package-lock.json](packages/vscode/package-lock.json), including TypeScript, VS Code/Node API types and Microsoft's VSCE. They are development tools, excluded from the VSIX; the extension has no runtime npm dependencies. The VSIX embeds the native helper's Rust dependency notice bundle and AIRK legal files.

Review the generated inventory for missing/custom licenses and distribution obligations before publishing. Regenerate it when dependencies change. Compiler/system libraries and base-image packages are separate distribution inputs requiring an SBOM/license review at release qualification.
