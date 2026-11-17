# JavaScript source provenance analysis

This owner reads an authenticated source-inventory delta and proves exact bytes,
not execution or product qualification. Every selected `.js` path is reopened;
size/SHA mismatch refuses the report. Paths are kept as aliases of their exact
content. Identical bytes may share one proof; filenames alone confer no status.

Existing generated-disposition evidence is reused only after authenticating the
JS, TypeScript, compiler and evidence pins. Fresh candidates are TypeScript blobs
reachable from the captured HEAD in packages/vscode/src, plus current files.
The pinned current compiler/options emit each candidate in memory without
executing source or writing JavaScript. Exact raw output hash and length must
match a selected JS object. This proves reproducibility under those stated
options, not that the historical packaging build used them. Git blob IDs and
current-source SHA remain explicit authorities, and original receipts stay intact.

Unmatched outputs remain unresolved; vendor paths await their separate signed
package proof. No source, dependency, corpus, credential or model payload is
copied. Output is JSON on stdout only, with original finite analysis deadline.
Caller owns same-human identity, guarded receipt retention and postwrite audit.

Git object format must explicitly be SHA1. A reachable blob's rev-list pathname
is only a label, not a claim that it exists at that path in the captured HEAD.
Node is resolved once to an absolute executable, pinned by bytes/SHA and version,
then invoked through that path. Delta and prior-disposition documents are parsed
from their authenticated reads. Before reporting, those documents, every prior
proof input consumed, matched current TypeScript, Node and compiler/config are
rechecked; drift refuses the result.
