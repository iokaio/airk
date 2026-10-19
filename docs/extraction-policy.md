# Collection extraction policy

Extraction policy selects an existing native adapter for each eligible source file and, in version 3, the collection’s indexing detail. It is persisted in collection state, separately from source files and editor settings. It does not install parsers, execute build files, follow imports, or enable an external producer.

## Inspect, validate and save

```powershell
airk collections extraction show
airk collections extraction validate --file extraction-policy.json
airk collections extraction set --file extraction-policy.json
airk index
```

Global `--root`, `--data-dir` and `--collection` select the collection as on other commands. `show` observes current policy; `validate` checks a proposed policy without saving or indexing. `set` saves the policy under the collection writer lock. Its effect becomes visible only in the next successful index publication. Existing reads continue using their published profiles if indexing has not run or fails.

Both `validate` and `set` require exactly one of `--file PATH` or `--json JSON`. Input is limited to 128 KiB and 256 rules. Versions 1 through 3 reject unknown fields and adapter IDs. Versions 2 and 3 accept captured header-context rules. Version 3 additionally accepts `detail`: `essential`, `balanced` or `detailed`. For larger policies, use `--file` to avoid operating-system command-line limits. An explicit reset to Balanced with no adapter overrides uses an empty rule list:

```powershell
airk collections extraction set --json '{"version":3,"detail":"balanced","rules":[]}'
```

New collections default to version 3, Balanced detail and no overrides. Existing indexed collections without a saved policy keep their published detail; legacy indexes and version 1/2 policies use Detailed. A version 3 policy without `detail` also retains the legacy Detailed interpretation. No policy file is automatically read from the source workspace.

Use `airk collections extraction detail balanced` (or `essential` / `detailed`) to change only detail while preserving adapter rules. The sidebar offers the same selection with a review of rebuild costs. Status includes `configured_detail`, `published_detail` and `pending_reindex`; [index detail](ingestion-limits.md#index-detail) explains the evidence retained at each level.

## Detail-aware extraction

For qualified common grammar profiles, detail is selected before rich syntax evidence is built. Essential still parses and walks the source, but captures only cheap written definition/signature spans for semantic planning. It does not build optional code facts, scope/documentation metadata or continuity fingerprints. Balanced omits generic call evidence while preserving declarations, imports, structural facts, recognized framework call/annotation cues and declaration continuity. Detailed keeps the existing supported evidence. The qualified shared path covers Rust, Python, JavaScript, TypeScript/TSX, Go, Java, C/C++, C#, Bash, PowerShell, Lua, Swift, Kotlin, JSON, YAML, TOML and CSS; this optimization does not expand those adapters' language understanding.

Essential Markdown, instruction Markdown and plain text collect heading spans without date, quantity, modality or requirement classification. PDF and PPTX apply that selected prose work after native text extraction. Qualified Markdown/code children in composite hosts receive the parent detail, with child coordinates and native locations preserved. Detailed document continuity uses a separate unit-only pass that does not repeat factual classification. DOCX/XLSX, repaired PHP/Ruby/SQL, repository/project/specialist adapters and other unqualified producers still build their supported raw evidence before detail selection. Composite host structure likewise retains its existing producer path. No reduction in native PDF/Office conversion cost is claimed.

Transient semantic seeds hold bounded written coordinates separately from retained facts. They produce the same complete semantic projection under all detail levels, including gaps and original native/mapped domains; they are not a second set of published facts. After selection, retained fact references and syntax-node references are normalized in separate namespaces before continuity generation. Complete selected-payload tests compare optimized paths with full-analysis references, including facts, identities, continuity, chunks and locations. Specialized fallbacks are kept until equivalent selected output is qualified.

The current extraction identity is `native-facts/9`, native adapter versions include `/sizing-1/detail-2`, and chunk strategy remains 3. This is a rebuild boundary for older derived indexes: run `airk index --full` to rebuild from source while preserving saved collection configuration. Supporting extraction-policy versions 1–3 preserves configuration semantics; it does not make old index formats readable. Worker count is separate from policy and identity; see [bounded extraction workers](ingestion-limits.md#bounded-extraction-workers).

## Rules and selection

```json
{
  "version": 3,
  "detail": "balanced",
  "rules": [
    {
      "selector": {"kind": "exact", "path": "src/generated.rs"},
      "case_sensitive": true,
      "adapter_id": "text"
    },
    {
      "selector": {"kind": "scoped", "prefix": "include", "suffix": ".h", "basename": null},
      "case_sensitive": true,
      "adapter_id": "cpp"
    }
  ]
}
```

Selectors use collection-relative paths with `/` separators. Absolute paths, backslashes, empty path components, `.` and `..` are rejected. `exact` matches one path. `scoped` matches descendants of its prefix, optionally restricted by literal suffix and/or basename; an empty prefix covers the collection. These are literal selectors, not glob patterns. A case-insensitive rule uses ASCII case folding; source paths retain their original spelling.

An exact match precedes scoped matches. Among scoped matches, the longest prefix wins; case-sensitive matches precede case-insensitive matches at equal specificity. Multiple matching rules with equal precedence cause selection to fail rather than silently choosing the first rule. `validate` checks rule shape and registered adapters; it does not scan source paths, so a path-specific conflict can first appear during indexing.

Without an override, selection uses the registry's known basenames, registered compound suffixes, ordinary suffixes, bounded content discriminators, then text fallback. Available adapters and their capability limitations appear in `show`. Selecting an adapter does not expand its supported dialects or turn syntax references into resolved references. For ambiguous formats, choose a qualified adapter explicitly rather than assuming a filename establishes project semantics.

Index reports expose bounded `selection_diagnostics` warnings, including the adapter actually selected, and `selection_diagnostics_truncated` when the list exceeds its limit. These warnings are regenerated during scans, including extraction reuse. They do not mean the file was skipped and do not replace native parse status or OCR provenance.

### Captured header context

Version 2 can select C/C++ headers from explicitly declared local compilation arguments:

```json
{
  "version": 2,
  "rules": [{
    "selector": {"kind": "header_context", "prefix": "build/include", "configuration": "build/compile_commands.json"},
    "case_sensitive": true,
    "adapter_id": "cpp"
  }]
}
```

This rule accepts a bounded JSON array of compilation entries whose relative `file` is resolved beneath the configuration file's directory. An eligible entry has `directory` absent, empty or `.` and an `arguments` array containing explicit `-x c++` or `-xc++` (likewise `c`). The rule's adapter must agree. Compiler names and shell command strings are not interpreted. Absolute paths, escaping paths, unsupported bases, conflicting entries and missing context do not imply C++ selection; unresolved headers retain C and a warning. Exact/scoped overrides take precedence over context rules.

Only the declared configuration files are captured, using registered-root discovery, ignore and symlink rules. At most 16 files, 128 KiB per file, 1,024 entries and 128 arguments per entry are admitted; argument strings are capped at 4,096 bytes. Discovery and reads consume the existing scan budgets. The same captured bytes supply the indexed configuration artifact, and the actual SHA-256 enters affected header profiles. Changes apply on publication and invalidate only affected effective profiles. AIRK does not run a compiler or claim build-accurate configuration. Version 1 rejects header-context rules.

## Configured versus published state

All three commands return policy status:

| Field | Meaning |
|---|---|
| `policy`, `digest` | Configured policy, or the proposed policy for validation, and its canonical digest |
| `origin` | `default`, `explicit`, or `preview` |
| `applied_digest` | Policy digest recorded by the published inventory, or null when unavailable |
| `pending_reindex` | Configured/proposed digest differs from the published digest; true when no applied digest exists |
| `publication` | Current exact publication token, or null when unavailable |
| `adapters` | Registered profiles, capability dimensions and limitations |

Selection is recomputed before extraction reuse. Files whose effective profile remains unchanged can reuse extraction; changing a relevant profile invalidates that file's evidence without pretending its source bytes changed. Saving a policy does not rewrite existing citations or reinterpret a reader's pinned publication. `airk index --full` deliberately rebuilds extraction instead of relying on that reuse.

## VS Code

**AIRK: Show Extraction Policy** displays collection/root/storage and native policy status. **AIRK: Configure Extraction Policy** selects a JSON file, captures a bounded private copy, validates it, and shows the current and proposed state. Saving requires confirmation and sends the same captured bytes even if the selected file changes while the preview is open. Temporary input is removed on completion, cancellation or failure.

After saving, the extension offers **Index now** for that collection. Declining leaves the policy pending until a later successful index. Both controls require workspace trust. They do not rewrite the selected file, workspace settings, agent configuration or tool approvals. See [VS Code extension](vscode-extension.md) for scheduling behavior.
