# SPDX-License-Identifier: Apache-2.0
"""Compose current measured facts in ignored copies; never edit package inputs."""
from pathlib import Path
import hashlib,json,shutil,difflib,subprocess
A=Path(__file__).parent; OLD=A/'p14-current-docs-composed-r3'; OUT=A/'p14-current-docs-composed-r4'; W=A/'p10-worktree'
shutil.copytree(OLD,OUT,dirs_exist_ok=True)
h=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
m=json.loads((OUT/'manifest.json').read_text())
old_summary='Broader current-producer, editor and corpus outcomes and primary acceptance remain pending.'
summary=('Eight current-producer broader stages and three separately run ignored vector-performance tests passed. '
    'Usage accounting and real-corpus campaigns failed and remain unresolved. Native packaging, extension compilation and VSIX packaging passed. '
    'The first private editor attempt was blocked by the existing VS Code updater before AIRK loaded; editor qualification and primary P14 acceptance remain pending. The ledger retains exact scope and failures.')
scope=('The selected current 10,000-file synthetic lexical campaign passed in **659.4201 s** of wrapper wall time, with '
    '**81,920,000 source bytes** and **317,580,564 logical index bytes**. These are owned-fixture/final logical footprint observations, '
    'not measured peak physical allocation or total process CPU/RSS. OS caches were not controlled. No 100,000-file campaign was selected; '
    'no extrapolated speed or 100k result is claimed.\n\n'
    'The current broader queue passed its first eight stages, then failed usage accounting and remains failed. The earlier usage fixture '
    'raced an automatic Verify and retained a strict unresolved-audit veto. The revised harness establishes native Verify/quiescence before '
    'the held publication interval and passed **7 pure tests**. Its actual current-runtime run still **failed** after **11.205 s** on typed '
    '`publication_busy`, with exact failed-read evidence retained. The prior audit veto was resolved; the origin of this remaining failure '
    'is under investigation. This observation neither establishes a new product-origin defect nor authorizes replay of an already submitted query. '
    'The original four-worker / forty-request workload and single-submission comparisons were not weakened. '
    'The separate current-source ignored vector-performance run passed **3 tests** in **76.6576 s**.\n\n'
    'The real-corpus campaign **failed** with `campaign_deadline` after **3,668.425 s** and observation outcome `native_errors_observed`. '
    'All four probes eventually became usable, but semantic coverage was partial; session, reopen, cancellation and the second corpus were '
    'not reached. No complete semantic-readiness, corpus performance or cross-corpus result is claimed. The collection recorded '
    '`cleanup_failed: false`; that does not convert the failed campaign into a pass. The actual AIRK source inventory remained identical: '
    '**1,789 files / 67,382,061 bytes**, and every sealed runtime file remained unchanged. No source excerpt was selected.\n\n'
    'The current-producer positive predecessor upgrade passed in **69.2571 s**. This is separate from the preserved never-indexed '
    '`ConfigurationUnavailable` refusal and from the prior D4e **67.743 s** result. The older fifteen-suite stress result remains historical; '
    'the current pair separately passed three selected installed native smoke suites.\n\n'
    'The current editor owner passed native packaging (**15.076 s**), canonical extension compilation (**10.822 s**) and VSIX packaging '
    '(**16.887 s**). The staged **1.8.8** VSIX contains **774 files / 73.82 MB**, SHA-256 '
    '`49170b51765048e438dfd9516a35e4884d7667efe5ad2652d02303caf22f4c90`. The first actual same-window scenario '
    '**failed after 36.379 s**, before AIRK loaded: the existing real VS Code updater held the global `vscode-updating` mutex. '
    'This is a host prerequisite blockage, not evidence of an AIRK defect or installed-editor acceptance. No fresh legacy fixture was created. '
    'Normal Code/updater processes were not killed or repinned. A later editor retry may reuse this exact verified package; the three passing '
    'package stages need not be rebuilt. An empty cleanup list does not prove an installed-editor lifecycle. Other private VSIX, multiwindow '
    'and legacy editor outcomes remain unqualified until the host update completes and actual scenarios run.\n\n'
    'Docker was available but no cached images were observed. Linux/container qualification was not selected, not reported unavailable or passed. '
    'Intel Mac, full macOS workspace, installed Mac editor, notarization, clean-machine/external-agent and controlled cold-cache/model-cost '
    'results remain unqualified without actual evidence. P13 retained current ranking and diversity. P14 is **not accepted**; version **1.8.8** '
    'is unpublished, with deleted-source recovery and the final local test installer still separate unfinished follow-ups.\n\n')
for f in m['files']:
 p=OUT/'source'/f['path']; raw=p.read_bytes();eol='\r\n' if b'\r\n' in raw else '\n';s=raw.decode().replace('\r\n','\n')
 s=s.replace(old_summary,summary)
 s=s.replace('Broader current-producer campaigns, editor and real-corpus outcomes remain pending collection/review. ', '')
 s=s.replace('Broader resource/editor/corpus outcomes and primary review remain pending.',
     'Selected broader passes, usage/corpus failures and host-blocked editor qualification retain their scoped results; primary review remains pending.')
 s=s.replace('Broader/editor/corpus/platform outcomes and primary acceptance remain pending.',
     'Selected broader passes and usage/corpus failures retain their scoped results; editor qualification and primary acceptance remain unresolved.')
 s=s.replace('separately reviewed P12 refresh/requery candidate and its limits','separately reviewed P12 refresh/requery and its limits')
 if f['path']=='docs/research/implementation-v190.md':
  s=s.replace('its actual current-producer upgrade result remains pending.', 'its current-producer positive campaign subsequently passed in **69.2571 s**; the separately retained never-indexed negative case remains a refusal.')
  lines=s.splitlines()
  for i,line in enumerate(lines):
   if line.startswith('| P14 | In progress'):
    lines[i]='| P14 | In progress - not accepted | Current `5d8912b1...`: 2,626 Windows tests, 15 final stages / 84 gates, five Neo tests plus signed native lifecycle, eight broader stages and three separate vector tests passed; usage/corpus failed, editor host prerequisite and primary acceptance unresolved |'
   elif line.startswith('| Current broader / installed / corpus |'):
    lines[i]='| Current broader / installed / corpus | Current broader first **8** stages passed: 10k scale, service load, three installed native smoke suites, positive runtime upgrade, transformer, code64, retrieval-quality and storage lifecycle. The queue then failed usage accounting; the revised harness also failed on typed `publication_busy` (**11.205 s**) despite **7** pure tests passing. Separate vector tests **3 passed / 76.6576 s**. Corpus **failed campaign_deadline / 3,668.425 s**, native errors and partial semantic coverage retained. Native/extension/VSIX packaging passed; the first editor scenario was blocked by the real VS Code updater before AIRK loaded. Editor qualification and primary acceptance pending. |'
  s='\n'.join(lines)+'\n'
  anchor='### Dated D4e checkpoint (6 November 2026; superseded source)'
  assert s.count(anchor)==1;s=s.replace(anchor,scope+anchor)
 elif f['path']=='docs/index-splitting.md':
  s=s.replace('Broader current-producer, editor and corpus outcomes remain pending; P14 is not accepted.',
      'Eight current broader stages and three separately run vector tests passed. Usage accounting and real-corpus campaigns failed; the editor attempt was host-blocked before AIRK loaded. Installed-editor qualification and primary acceptance remain pending. P14 is not accepted.')
 elif f['path']=='docs/vscode-extension.md':
  s=s.replace('broader/editor/corpus outcomes and primary acceptance remain pending.',
      'eight broader stages and three separate vector tests passed, usage/corpus campaigns failed, and the first editor attempt was blocked by the existing VS Code updater before AIRK loaded. Installed-editor qualification and primary acceptance remain pending.')
 p.write_bytes(s.replace('\n',eol).encode());f['proposed_sha256']=h(p)
patch=''
for f in m['files']:
 name=f['path'];patch+=''.join(difflib.unified_diff((OUT/'base'/name).read_bytes().decode().splitlines(keepends=True),(OUT/'source'/name).read_bytes().decode().splitlines(keepends=True),fromfile='a/'+name,tofile='b/'+name))
(OUT/'current-docs.patch').write_bytes(patch.encode())
assert all(h(W/f['path'])==f['base_sha256'] for f in m['files'])
r=subprocess.run(['git','-c','safe.directory='+str(W.resolve()),'-C',str(W),'apply','--check',str((OUT/'current-docs.patch').resolve())],capture_output=True,text=True);assert r.returncode==0,r.stderr
evidence=['p14-closeout-supplement-draft-r1/manifest.json','p14-closeout-supplement-draft-r1/supplement.md',
 'p14-broader-qualification-r3.json','p14-broader-ignored-vector-performance-r2.receipt.json',
 'p14-usage-stable-native-r1.receipt.json','p14-usage-stable-native-r1.log','p14-usage-stable-unit-r1.receipt.json','p14-usage-stable-unit-r1.log',
 'p14-broader-real-corpora-r1.receipt.json','p14-real-corpora-collection-r1.json','p14-package-editor-owner-r2.json',
 'p14-package-native-r2.receipt.json','p14-package-extension-compile-r2.receipt.json','p14-package-vsix-r2.receipt.json','p14-package-five-private-scenarios-r2.receipt.json']
for name in ['p14-usage-stable-native-r1','p14-usage-stable-unit-r1','p14-broader-real-corpora-r1']:
 receipt=json.loads((A/(name+'.receipt.json')).read_text());assert h(A/(name+'.log'))==receipt['log_sha256']
corpus=json.loads((A/'p14-real-corpora-collection-r1.json').read_text());assert corpus['completed'] is False and corpus['cleanup_failed'] is False and corpus['airk_source_identity_preserved'] and corpus['runtime_all_sealed_files_unchanged']
m.update({'status':'INERT15docR4 composition; P14 unaccepted; no live package-input edits','patch_sha256':h(OUT/'current-docs.patch'),
 'base_composition_sha256':h(OLD/'current-docs.patch'),'evidence_r4':{name:h(A/name) for name in evidence},
 'pending_placeholders':['Resolve/collect usage-accounting and real-corpus failures without weakening workload or claims','Actual installed-editor/private VSIX/multiwindow/legacy outcomes','PrimaryP14 acceptance; subsequent deleted-source recovery and local test installer'],
 'static_validation':'All15currentbase/proposed hashes verified; applycheck0. Current editor guards untouched. No runtime/compile/editor execution.'})
(OUT/'manifest.json').write_bytes((json.dumps(m,indent=2)+'\n').encode())
print('PATCH',h(OUT/'current-docs.patch'));print('MANIFEST',h(OUT/'manifest.json'))
