# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import shutil,json,hashlib,difflib,subprocess
a=Path(__file__).parent
old=a/'p14-current-docs-composed-r2'; new=a/'p14-current-docs-composed-r3'
shutil.copytree(old,new,dirs_exist_ok=True)
m=json.loads((new/'manifest.json').read_text(encoding='utf-8'))
fresh='The fresh Windows producer `p14-runtime-final-r2` passed in **764.1665 s** with incremental compilation off. All **15 final R4 stages** passed, including all **84 unchanged gates**, the fresh dependency audit and **459 Python tests** (**9 expected skips**). Broader current-producer, editor and corpus outcomes and primary acceptance remain pending.'
for f in m['files']:
 p=new/'source'/f['path']; raw=p.read_bytes(); text=raw.decode('utf-8'); eol='\r\n' if b'\r\n' in raw else '\n'; text=text.replace('\r\n','\n')
 # Current summaries share a short paragraph immediately before remaining outcomes.
 if f['path']=='docs/research/implementation-v190.md':
  text=text.replace('absent. The new Windows producer, fresh final R4 queue, broader/editor/corpus\noutcomes and primary review remain pending.','absent. '+fresh)
  lines=text.splitlines()
  for i,l in enumerate(lines):
   if l.startswith('| P14 | In progress'):
    lines[i]='| P14 | In progress - not accepted | Current `5d8912b1...`: 2,626 Windows tests, 15 final stages / 84 unchanged gates, five Neo tests, fresh ARM64 release and Developer ID/copied-launcher/isolated-install checks passed; broader/editor/corpus and primary acceptance pending |'
   if l.startswith('| Windows full / fresh final |'):
    lines[i]='| Windows full / fresh final | **2,626 passed**, **0 failed**, **50 selection exclusions**; Nextest **1,599.125 s**, wrapper **1,648.753 s**. '+fresh+' |'
  text='\n'.join(lines)+'\n'
  text=text.replace('retains the original refusal and pending manual recovery.', 'retains the original `ConfigurationUnavailable` refusal, nonzero exit and pending manual recovery. The current negative observation retained exact source and registration; the read-only base-grant census was zero. The not-yet-created provenance column was unavailable, not inferred.')
 elif f['path']=='docs/releases/release-1.8.8.md':
  start=text.index('**P14 evidence scope:**'); end=text.index('\n\nThe [refresh guide]',start)
  text=text[:start]+'**Dated, superseded P14 evidence:** source `d4e48a53...` included the cold-default model correction. Its 15 final Windows stages, 2,624-case workspace suite and two scoped Neo cases passed on that earlier source. These records retain their original identity and do not qualify the newer producer. The P12 record below likewise retains its original source and limits. P14 is not accepted.'+text[end:]
  text=text.replace('The new Windows producer and final R4 queue, broader current-producer campaigns,\neditor and real-corpus outcomes remain pending collection/review.',fresh)
 elif f['path']=='docs/index-splitting.md':
  text=text.replace('mutations. Focused Windows and five Neo cases passed. Full current-source,\nbroader and installed/platform outcomes remain pending; P14 is not accepted.', 'mutations. Current Windows full Nextest passed **2,626 tests**; five Neo cases and the signed/copied-launcher/isolated-install checks passed. The fresh Windows final R4 queue passed all 15 stages and 84 unchanged gates. Broader current-producer, editor and corpus outcomes remain pending; P14 is not accepted.')
 elif f['path'] in ['README.md','docs/README.md','packages/vscode/README.md','docs/releases/release-1.9.0.md','docs/architecture.md','docs/vscode-extension.md']:
  # Append the newly observed evidence once, leaving dated records intact.
  pos=text.find('\n\n',text.find('5d8912b1')) if '5d8912b1' in text else -1
  if pos>=0: text=text[:pos]+'\n\n'+fresh+text[pos:]
 text=text.replace('The new Windows producer and final R4 queue, broader current-producer campaigns,\neditor and real-corpus outcomes remain pending collection/review.', 'Broader current-producer campaigns, editor and real-corpus outcomes remain pending collection/review.')
 text=text.replace('The new Windows producer/final queue,\nbroader resource/editor/corpus outcomes and primary review remain pending.', 'Broader resource/editor/corpus outcomes and primary review remain pending.')
 text=text.replace('The fresh Windows producer/final R4 queue and broader/editor/corpus\noutcomes remain pending.', 'Broader/editor/corpus outcomes remain pending.')
 text=text.replace('The new\nWindows producer and final/broader/editor/corpus/platform outcomes remain pending.', 'Broader/editor/corpus/platform outcomes and primary acceptance remain pending.')
 text=text.replace('P14 final qualification remains pending; see', 'P14 final Windows qualification passed; broader/editor/corpus outcomes and primary acceptance remain pending. See')
 p.write_bytes(text.replace('\n',eol).encode('utf-8'))
 # Remove stale current pending phrases after fresh receipt collection, excluding historical sections.
 # The primary reviewer fills broader/editor/corpus and acceptance after actual runs.
 f['proposed_sha256']=hashlib.sha256(p.read_bytes()).hexdigest()
patch=''
for f in m['files']:
 rel=f['path']; b=(new/'base'/rel).read_bytes().decode('utf-8').splitlines(keepends=True); s=(new/'source'/rel).read_bytes().decode('utf-8').splitlines(keepends=True)
 patch+=''.join(difflib.unified_diff(b,s,fromfile='a/'+rel,tofile='b/'+rel))
(new/'current-docs.patch').write_bytes(patch.encode('utf-8'))
m['patch_sha256']=hashlib.sha256((new/'current-docs.patch').read_bytes()).hexdigest()
m['status']='INERT15docR3 composition; no live edits/P14 acceptance'
m['pending_placeholders']=['Current broader10/10k/service-load/upgrade/installed/corpus/editor outcomes','Primary P14 acceptance and post-P14 missing-source recovery/local installer']
m['latest_parent_observations']['final_r4']='All15 passed;84 unchanged gates; Python459/9 expected skips. Producer764.1665s inc0; logSHA916c7f5cbc10307cc573767dcb7aeaab5a8dc409108d3aac201311d8faa807db'
m['static_validation']='R3 three independent-review stale-current findings corrected; all15 bases/proposals checked; applycheck0'
(new/'manifest.json').write_bytes((json.dumps(m,indent=2)+'\n').encode())
w=a/'p10-worktree'; h=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
assert all(h(w/f['path'])==f['base_sha256'] for f in m['files'])
r=subprocess.run(['git','-c','safe.directory='+str(w.resolve()),'-C',str(w),'apply','--check',str((new/'current-docs.patch').resolve())],capture_output=True,text=True)
assert r.returncode==0,r.stderr
print('PATCH',m['patch_sha256']); print('MANIFEST',h(new/'manifest.json'))
