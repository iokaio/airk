# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,difflib
b=Path('D:/code/Github/airk/artifacts/v190');w=b/'p10-worktree';old=b/'p11a-composed-current-proposed';out=b/'p11a-accepted-p10-r2-proposed';out.mkdir(exist_ok=True);prior=json.loads((b/'p11a-composed-current-base.json').read_text());sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
docs={'README.md','packages/vscode/README.md','docs/README.md','docs/releases/release-1.8.8.md'};files=[];parts=[]
for row in prior['files']:
 relative=row['path'];live=w/relative;base=live.read_bytes() if live.exists() else None;proposed=(old/relative).read_bytes()
 if relative not in docs:assert (hashlib.sha256(base).hexdigest() if base is not None else None)==row['base_sha256'],relative
 else:
  # Keep original P11a addition exact, graft onto accepted current document only.
  lines=proposed.decode('utf-8',errors='surrogateescape').replace('\r\n','\n').splitlines(keepends=True);assert lines[1]=='\n' and lines[2].startswith('P11a') and 'unaccepted' in lines[2]
  accepted=base.decode('utf-8').replace('\r\n','\n').splitlines(keepends=True);assert accepted[0]==lines[0]
  z=''.join(accepted[:1]+lines[1:3]+accepted[1:]);proposed=z.replace('\n','\r\n' if b'\r\n'in base else '\n').encode('utf-8')
 dest=out/relative;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(proposed)
 a=base.decode('utf-8',errors='surrogateescape').replace('\r\n','\n') if base is not None else '';z=proposed.decode('utf-8',errors='surrogateescape').replace('\r\n','\n')
 parts.append(''.join(difflib.unified_diff(a.splitlines(True),z.splitlines(True),fromfile='a/'+relative if base is not None else '/dev/null',tofile='b/'+relative,n=1 if relative in docs else 3)))
 files.append({'path':relative,'base_sha256':hashlib.sha256(base).hexdigest() if base is not None else None,'proposal_sha256':sha(dest),'delta':'Original P11a introduction exact; current accepted P10 body preserved' if relative in docs else 'Original frozen proposed bytes unchanged'})
p=b/'p11a-accepted-p10-r2.proposed.patch';assert not p.exists();p.write_bytes(''.join(parts).encode('utf-8',errors='surrogateescape'))
support=b/'p11a-effective-support-proposed';sf=json.loads((support/'base.json').read_text())
for row in sf['files']:
 path=out/row['path'] if (out/row['path']).exists() else w/row['path'];assert sha(path)==row['base_sha256'],row['path']
ledger='docs/research/implementation-v190.md';assert ledger not in [r['path'] for r in files]
manifest={'status':'INERT apply-ready additive composition atop accepted P10; not applied/compiled/tested','accepted_p10_commit':'c2584cb05dfdeba6e948b91e4964479a07f9d3fc','patch':p.name,'sha256':sha(p),'files':files,'old_composed_patch':'p11a-composed-current.proposed.patch','old_sha256':prior['combined_patch_sha256'],'docs_resolution':'Native21 targets unchanged; acceptance changed four existing document bases. All four receive EXACT original title-only P11a addition with minimal one-line context; no accepted P10 body is replaced. README/Marketplace original context conflicts resolved; docs index/release same semantic additions preserved with fresh base hashes.','preserved_accepted_ledger_sha256':sha(w/ledger),'apply_order':[{'step':1,'patch':p.name,'sha256':sha(p),'verify':'Every fresh25 base hash matches before applying.'},{'step':2,'patch':'p11a-effective-support-proposed/effective-support.patch','sha256':sha(support/'effective-support.patch'),'verify':'Two sourceio target base hashes match after step1. Original sourceio manifest stays unchanged; its old wrapper SHA differs only because accepted docs were rebased; targeted native bytes equal original frozen composition.'}],'witness_owners_excluded':['crates/airk-store/src/current/postings.rs','crates/airk-store/src/current/postings_tests.rs','crates/airk-kernel/src/split.rs'],'future_checks_correction':['Include airk-ingest in scoped Clippy packages.','Kernel profile_families filter expected2 tests (native+partial-scope), not old notes1.','Add Ingest metadata_support_requires_complete_producer_without_source_io expected1.','Existing actual CLI/MCP profile_families exact test expected1 in each required mode, using fresh new P11a runtime pair; preserve complete raw results and input schemas.'],'no_claim':'No Git, compiler, build, tests, live edits or SSH. P11a remains unaccepted; later full required qualification unchanged.'}
(b/'p11a-accepted-p10-r2-base.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
notes=json.loads((b/'p11a-composed-current-notes.json').read_text());notes['combined_patch']=p.name;notes['sha256']=sha(p);notes['base_manifest']='p11a-accepted-p10-r2-base.json';notes['phase_state']='P10 accepted locally at c2584cb; P11a remains unaccepted';notes['apply_order']=manifest['apply_order'];notes['documentation_base_resolution']=manifest['docs_resolution']
for command in notes['future_focused_commands']:
 if command['name']=='scoped-clippy-format-first':command['command'][command['command'].index('--all-features'):command['command'].index('--all-features')]=['-p','airk-ingest']
 if command['name']=='kernel-metadata-report':command['expected_passing_tests']=2
notes['future_focused_commands'].insert(2,{'name':'ingest-qualified-producer-support','command':['cargo','test','-p','airk-ingest','--lib','--all-features','--locked','--offline','--timings','metadata_support_requires_complete_producer_without_source_io','--','--test-threads=1'],'expected_passing_tests':1})
(b/'p11a-accepted-p10-r2-notes.json').write_text(json.dumps(notes,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'patch_sha256':sha(p),'files':len(files),'native_targets_identical':21,'additive_doc_bases':4,'support_patch_sha256':sha(support/'effective-support.patch')}))
