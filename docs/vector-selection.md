# Explicit vector selection

AIRK indexes lexical passages, facts and original file evidence independently of vector selection. A collection can exclude selected files from semantic planning, embedding inference and DiskANN while retaining that other evidence. No generated-file, vendor-directory or filename classifier runs automatically: the default policy has no exclusions.

The selection policy is separate from the collection's vector enabled setting. Saving exclusions preserves an existing opt-out, and toggling vectors preserves exclusions. Policies are bound to the collection's owner root.

## Configure and inspect

Create a JSON file outside the indexed sources, for example:

```json
{
  "version": 1,
  "exclusions": [
    {
      "id": "generated-javascript",
      "path_prefix": "generated/",
      "suffix": ".js"
    },
    {
      "id": "dependency-lockfile",
      "basename": "Cargo.lock"
    },
    {
      "id": "external-generated-text",
      "path_prefix": "@reference/generated/",
      "language": "text"
    }
  ]
}
```

Each rule must have a unique `id` and at least one selector. Available selectors are `path_prefix`, `basename`, `suffix`, `adapter` and `language`. All selectors present in a rule must match; any matching rule excludes the file. Matching uses exact, case-sensitive strings, with no regular expressions or wildcards. Use a trailing `/` when a path prefix should select a directory. Paths are collection-relative citation paths, including `@alias/` for additional roots. Adapter and language match the indexed extraction profile and language, respectively.

When several rules match, the lexicographically smallest rule ID is the reported reason. Canonical policy ordering makes the digest independent of rule order.

```console
airk collections vectors selection show
airk collections vectors selection validate --file /path/to/vector-selection.json
airk collections vectors selection set --file /path/to/vector-selection.json
airk index
airk collections vectors show
airk info generated/example.js
```

`validate` checks a proposed policy without saving it. `set` saves it for the next successful index; it does not start indexing. Both accept exactly one `--file` or `--json` input, limited to 128 KiB. Invalid input leaves the saved policy unchanged. The usual `--root`, `--data-dir` and `--collection` options select the collection.

To remove all exclusions, save `{"version":1,"exclusions":[]}`. The existing `airk collections vectors set --enabled false` command disables vectors for the whole collection; `--enabled true` re-enables them with the saved exclusions.

## Interpret the result

Selection status reports the configured policy and digest, the applied digest, the publication token and `pending_reindex`. A policy change remains pending until a successful index publishes it, including when vectors are globally disabled. The broader vector status also reports model readiness; an enabled collection without a model remains pending independently of selection.

File information reports `vector_selection.eligible`, `vector_selection.excluded_by` and `vector_selection_digest` from that file's committed revision. An exclusion saved after that revision does not change its file information. Eligibility describes selection, not whether the file produced a vector: global opt-out, missing models or unrecognized model tokens can still result in no vector. Historical file information uses its requested revision's policy.

An excluded file remains available to lexical search, exact file reads and structural/document fact retrieval. Hybrid search can still return it through the lexical branch. Its semantic-only search contribution is removed on the next successful index.

The saved enabled setting and exclusions survive `airk index --full`. Obsolete derived indexes require that explicit rebuild; their evidence is not read to configure the replacement. Source files are never rewritten by selection changes or rebuilding.
