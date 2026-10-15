# Collection extraction policy

Extraction policy selects an existing native adapter for each eligible source file. It is persisted in collection state, separately from source files and editor settings. It does not install parsers, execute build files, follow imports, or enable an external producer.

## Inspect, validate and save

```powershell
airk collections extraction show
airk collections extraction validate --file extraction-policy.json
airk collections extraction set --file extraction-policy.json
airk index
```

Global `--root`, `--data-dir` and `--collection` select the collection as on other commands. `show` observes current policy; `validate` checks a proposed policy without saving or indexing. `set` saves the policy under the collection writer lock. Its effect becomes visible only in the next successful index publication. Existing reads continue using their published profiles if indexing has not run or fails.

Both `validate` and `set` require exactly one of `--file PATH` or `--json JSON`. Input is limited to 128 KiB; version 1 accepts at most 256 rules and rejects unknown fields and adapter IDs. For larger policies, use `--file` to avoid operating-system command-line limits. An explicit reset uses an empty rule list:

```powershell
airk collections extraction set --json '{"version":1,"rules":[]}'
```

The default is also version 1 with no overrides. No policy file is automatically read from the source workspace.

## Rules and selection

```json
{
  "version": 1,
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
