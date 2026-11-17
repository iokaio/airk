# SPDX-License-Identifier: Apache-2.0
import difflib
import hashlib
import json
import shutil
from pathlib import Path

base = Path('D:/code/Github/airk/artifacts/v190')
old = base / 'p14-current-docs-composed-r4'
out = base / 'p14-current-docs-composed-r5'
out.mkdir(exist_ok=False)
shutil.copytree(old / 'base', out / 'base')
shutil.copytree(old / 'source', out / 'source')
prior = json.loads((old / 'manifest.json').read_text())
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()

common_old = 'Eight current-producer broader stages and three separately run ignored vector-performance tests passed. Usage accounting and real-corpus campaigns failed and remain unresolved. Native packaging, extension compilation and VSIX packaging passed. The first private editor attempt was blocked by the existing VS Code updater before AIRK loaded; editor qualification and primary P14 acceptance remain pending.'
common_new = 'All **10 selected broader stages** passed through exact evidence adoption, including the corrected usage campaign and three ignored vector-performance tests; the original failed queue remains failed. The real-corpus campaign reached its one-hour deadline with partial coverage and remains failed. Native packaging, extension compilation and VSIX packaging passed. The same-installation, different-installation and multiwindow editor scenarios passed using the verified official ZIP host and the unchanged VSIX; the subsequent usage administration UI scenario failed. Untrusted and legacy editor scenarios and primary P14 acceptance remain pending.'
short_old = 'Eight current broader stages and three separately run vector tests passed. Usage accounting and real-corpus campaigns failed; the editor attempt was host-blocked before AIRK loaded. Installed-editor qualification and primary acceptance remain pending.'
short_new = 'All 10 selected broader stages passed by exact evidence adoption; the failed original queue is preserved. The real-corpus campaign failed at its deadline. Three installed-editor scenarios passed; usage administration UI failed, with untrusted/legacy scenarios and primary acceptance pending.'
replacements = [
    (common_old, common_new),
    (short_old, short_new),
    ('Selected broader passes and usage/corpus failures retain their scoped results; editor qualification and primary acceptance remain unresolved.', 'Ten selected broader stages passed; the corpus deadline failure and partial editor results retain their scope. Primary acceptance remains unresolved.'),
    ('Selected broader passes, usage/corpus failures and host-blocked editor qualification retain their scoped results; primary review remains pending.', 'Ten selected broader stages passed; the corpus deadline failure and partial editor results retain their scoped results. Primary review remains pending.'),
    ('P14 final Windows qualification passed; eight broader stages and three separate vector tests passed, usage/corpus campaigns failed, and the first editor attempt was blocked by the existing VS Code updater before AIRK loaded. Installed-editor qualification and primary acceptance remain pending.', 'P14 final Windows qualification and all 10 selected broader stages passed. The corpus campaign failed at its deadline. Same-installation, different-installation and multiwindow editor scenarios passed; usage administration UI failed, and untrusted/legacy scenarios and primary acceptance remain pending.'),
    ('five Neo tests plus signed native lifecycle, eight broader stages and three separate vector tests passed; usage/corpus failed, editor host prerequisite and primary acceptance unresolved', 'five Neo tests plus signed native lifecycle and all 10 selected broader stages passed; corpus deadline failure and partial editor results retained; primary acceptance unresolved'),
]
for row in prior['files']:
    path = out / 'source' / row['path']
    data = path.read_text()
    for before, after in replacements:
        data = data.replace(before, after)
    if row['path'] == 'docs/research/implementation-v190.md':
        a = data.index('| Current broader / installed / corpus |')
        b = data.index('\n| Phase decision |', a)
        data = data[:a] + '| Current broader / installed / corpus | **10 selected broader stages passed** by exact adoption: eight original passes, corrected usage and three ignored vector tests. Original broader queue remains failed. Corpus **failed campaign_deadline / 3,668.425 s**, with native errors and partial semantic coverage retained. Package stages passed; same-installation, different-installation and multiwindow editor scenarios passed. Usage administration UI failed; untrusted/legacy and primary acceptance pending. |' + data[b:]
        a = data.index('The current broader queue passed its first eight stages, then failed usage accounting and remains failed.')
        b = data.index('\n\nThe real-corpus campaign', a)
        data = data[:a] + ('The original broader queue passed its first eight stages, then failed usage accounting and remains failed. The corrected usage harness establishes actual Verify/quiescence and holds both the existing writer lease and an explicit read-only SQLite transaction on the exact active retained publication. This prevents last-reader-close churn during fresh CLI reads. It passed **9 pure tests** and the actual current-runtime campaign in **32.7458 s**. The original **41 queries**, **four workers / forty requests**, attribution, BPE/pricing and enable/disable/clear assertions remain unchanged; no submitted query is replayed. Earlier audit-veto and typed `publication_busy` failures are retained. The separate current-source ignored vector-performance run passed **3 tests** in **76.6576 s**. The supplement adopts those results and the eight original passes as **10 selected passing stages**, without relabeling the failed original queue.') + data[b:]
        a = data.index('The current editor owner passed native packaging')
        b = data.index('\n\nDocker was available', a)
        data = data[:a] + ('The editor owner passed native packaging (**15.076 s**), canonical extension compilation (**10.822 s**) and VSIX packaging (**16.887 s**). The staged **1.8.8** VSIX contains **774 files / 73.82 MB**, SHA-256 `49170b51765048e438dfd9516a35e4884d7667efe5ad2652d02303caf22f4c90`. The first editor attempt failed before AIRK loaded because the normal VS Code updater held `vscode-updating`. A checksum-verified official **1.145.0** ZIP host at the same commit then launched private test profiles; all **3,280 stock files** were verified against that archive. Normal Code/updater processes were not killed or repinned. Subsequent harness failures exposed an obsolete UseGrant label and obsolete catalog-session CLI assumptions. Six editor-helper tests and nine installed-contract tests passed after correcting those test contracts. A targeted semantic TypeScript compile emitted only the three affected external test modules; production source, compiled production output, native input identity and the exact VSIX remained unchanged. The VSIX contains no test outputs.\n\nThe latest five-scenario owner remains **failed** after **449.7647 s**. Its same-installation, different-installation and multiwindow scenarios completed successfully before the usage administration UI timed out at action 48, `usageAdmin`; untrusted and fresh legacy scenarios were not reached. The failed usage fixture was subsequently proven quiescent using native all-role leases and verified inspection after health was already unavailable. This is separate cleanup evidence, not a fabricated UI Stop receipt or a passing editor scenario. No production defect is inferred from this unresolved test handoff. The three passing scenarios must retain their exact raw response, capture, lifecycle and cleanup evidence when adopted; later selected scenarios must use fresh private profiles. P14 acceptance remains pending.') + data[b:]
    path.write_text(data, encoding='utf-8', newline='\n')

patch = ''
rows = []
for row in prior['files']:
    rel = row['path']
    before = out / 'base' / rel
    after = out / 'source' / rel
    assert sha(before) == row['base_sha256']
    rows.append({'path': rel, 'base_sha256': sha(before), 'proposed_sha256': sha(after)})
    patch += ''.join(difflib.unified_diff(before.read_text().splitlines(True), after.read_text().splitlines(True), fromfile='a/' + rel, tofile='b/' + rel))
(out / 'current-docs.patch').write_text(patch, encoding='utf-8', newline='\n')
evidence_names = ['p14-broader-qualification-supplement-r1.json', 'p14-usage-stable-native-r2.receipt.json', 'p14-usage-stable-unit-r2.receipt.json', 'p14-editor-ui-node-tests-r2.receipt.json', 'p14-installed-editor-node-tests-r1.receipt.json', 'p14-installed-editor-compile-r2.json', 'p14-package-editor-continuation-r3.json', 'p14-editor-cleanup-native-installed-cUDfG8-r1.json', 'p14-real-corpora-collection-r1.json']
manifest = {
    'status': 'INERT R5 partial-evidence proposal; P14 NOT accepted; final editor outcome and late checks still pending',
    'files': rows, 'patch_sha256': sha(out / 'current-docs.patch'),
    'previous_manifest_sha256': sha(old / 'manifest.json'),
    'observed_evidence': {n: sha(base / n) for n in evidence_names},
    'inherited_evidence': prior.get('evidence_r4', {}),
    'pending': ['Actual selected usage/untrusted/legacy editor outcomes; preserve three prior passing scenarios with exact adoption', 'Final late Python discovery/repository/version/whitespace checks', 'Independent complete proposal review and root primary review', 'Primary P14 acceptance before post-P14 recovery'],
    'python_count': '459 from finalR4 observed; nine new usage tests statically inventoried; 468 is expected, not an observed full-suite result',
    'scope': 'Only ignored proposal files written. No tracked docs, package inputs, native/editor execution or acceptance changed.'
}
(out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(json.dumps({'directory': str(out), 'patch_sha256': manifest['patch_sha256'], 'manifest_sha256': sha(out / 'manifest.json'), 'files': len(rows)}))
