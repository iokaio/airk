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

Declaration files ending in .d.ts are recorded with exact source identity as
nonemitting candidates, never sent to transpileModule. Other emission errors
refuse the analysis and identify the exact candidate pathname and SHA256; they
are not silently treated as generated outputs or successful exclusions.

## Role-only linkage supplement

link_roles.py binds exact unresolved output bytes to observed esbuild module
labels or TypeScript emission structure, declared package scripts/build owners,
maintained current or reachable Git source paths, and pinned npm lock/license
metadata. These are declared owner and generation-role links, not evidence that
the original source version or exact bundled bytes were reconstructed. Both
source_version_established and exact_output_reproduced remain false. Missing
owners are explicit gaps; vendor module bytes are never copied. A separate exact
literal fixture case may be proved against its maintained test generator.
No bundler/compiler/product build is executed by this supplement.

The fixture proof owner parses static NoSubstitutionTemplateLiteral or output
StringLiteral nodes; it never evaluates template expressions or executes a
generator. Source/compiler/Node pins, reachable Git data, literal locations and
TAG parameter remain explicit. Exact fixture reproduction does not establish
which generator was actually used historically or confer runtime qualification.
Inherited role-report fields remain historical context; exact_fixture_reproduction
and its source proof describe the later byte comparison.

The standard disposition adapter accepts only the two reviewed exact-generation
proof kinds, retains the original source/size/hash aliases, and binds their
source authority and compiler metadata to the pinned analysis receipt. It never
adapts role-only evidence as exact reproduction. Private shards are create-only,
size-bounded and owner-audited; the standard consumer reopens their declarations.

Current product source and build recipes remain canonical. Obsolete bundle
output/source-version gaps are reported honestly rather than bulk-copied into
the maintained tree. Concrete authored gaps are preserved separately, with
exact reversible byte maps and owning purpose documentation.

Historical JS capture tests reconstruct the exact original body and verify original and maintained hashes/lengths for all three maps without reading artifact originals. Their explicit --audit-originals path is a custody audit, requiring the actual original or an authenticated successful removal/pre-removal chain through the shared source_cleanup owner. An unrecorded missing original remains a refusal. Parsing retained JS through vm.Script remains syntax-only; captured code is not executed.
