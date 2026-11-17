# SPDX-License-Identifier: Apache-2.0
import difflib,hashlib,json,re,shutil
from pathlib import Path
B=Path(__file__).resolve().parent
OLD=B/'p14-current-docs-composed-r6'
NEW=B/'p14-current-docs-composed-r7'
NEW.mkdir(exist_ok=False)
shutil.copytree(OLD/'base',NEW/'base');shutil.copytree(OLD/'source',NEW/'source')
m=json.loads((OLD/'manifest.json').read_text())
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
for rel,release,ledger in [
 ('README.md','docs/releases/release-1.8.8.md','docs/research/implementation-v190.md'),
 ('docs/README.md','releases/release-1.8.8.md','research/implementation-v190.md'),
 ('packages/vscode/README.md','https://github.com/iokaio/airk/blob/main/docs/releases/release-1.8.8.md','https://github.com/iokaio/airk/blob/main/docs/research/implementation-v190.md')]:
 p=NEW/'source'/rel;s=p.read_text();title=s.split('\n',1)[0]
 boundary=s.index('| Current guide |') if rel=='docs/README.md' else s.index('**Find the code and documents behind a feature.')
 intro=('**1.8.8 is an unpublished manual-test working build.** Accepted local P9–P12 work adds permanent exclusions, reviewed index splitting, per-file profile reporting, explicit Lean/Rich retained profiles and reviewed reference refresh with separate requery. P13 retains current ranking and diversity. P14 remains in progress: selected Windows editor coverage passed on documented source identities; final current-source checks and primary acceptance are pending. See the [working-build record]('+release+') and [implementation ledger]('+ledger+') for exact evidence, failures and platform limits.\n\n'
 'Opening a trusted folder does not register or index it. Use **Indexing > Index library** to review sources, storage and watching; create Project memberships and review an access grant, then explicitly **Use Grant for this window**. Evidence reads and source verification do not start indexing.\n\n')
 p.write_text(title+'\n\n'+intro+s[boundary:],encoding='utf-8',newline='\n')
 if rel=='README.md':
  original=re.search(r'<details>.*?</details>',s,re.S).group(0)
  assert re.search(r'<details>.*?</details>',p.read_text(),re.S).group(0)==original
# Remove repeated old qualification data from the working release introduction;
# the complete labeled prior-source summary and ledger remain intact.
p=NEW/'source/docs/releases/release-1.8.8.md';s=p.read_text()
a=s.index('P13 retained current ranking and diversity; no candidate was adopted.')
z=s.index('**Prior source `5d8912b1...` evidence',a)
s=s[:a]+('P13 retained current ranking and diversity; no candidate was adopted. P14 remains\n**in progress, not accepted** on source `237f8969...` / 960 native inputs.\nThe current and historical observations below retain their original identities.\n\n')+s[z:]
p.write_text(s,encoding='utf-8',newline='\n')
patch=''
for row in m['files']:
 rel=row['path'];p=NEW/'source'/rel;row['proposed_sha256']=sha(p)
 patch+=''.join(difflib.unified_diff((NEW/'base'/rel).read_text().splitlines(True),p.read_text().splitlines(True),fromfile='a/'+rel,tofile='b/'+rel))
(NEW/'current-docs.patch').write_text(patch,encoding='utf-8',newline='\n')
m.update(status='INERT R7 frontpage-compressed draft; current fullnative results pending; NOT accepted',patch_sha256=sha(NEW/'current-docs.patch'),previous_manifest_sha256=sha(OLD/'manifest.json'))
m['primary_review_correction']='README/docs index/Marketplace front status reduced to two short paragraphs with setup/access behavior and evidence links; exact timings/digests/history stay in release1.8.8+ledger; README older-release details block byte-identical; no live edits.'
(NEW/'manifest.json').write_text(json.dumps(m,indent=2)+'\n')
print(json.dumps({'patch_sha256':m['patch_sha256'],'manifest_sha256':sha(NEW/'manifest.json'),'files':len(m['files'])}))
