# SPDX-License-Identifier: Apache-2.0
import difflib
import json
from pathlib import Path

root = Path.cwd()
destination = Path(__file__).resolve().parent
proposed = {}
for name, old in [
    ('packages/ui/src/nativeMetadataCodec.ts', '||!origin.project_ids.length'),
    ('packages/vscode/src/explorerSearch.ts', ' || !origin.project_ids.length'),
    ('packages/vscode/src/workbenchRender.ts', '||!origin.project_ids.length||origin.project_ids.length>32'),
]:
    source = (root / name).read_text(encoding='utf-8')
    assert source.count(old) == 1, (name, source.count(old))
    proposed[name] = source.replace(old, '||origin.project_ids.length>32' if name.endswith('workbenchRender.ts') else '')

name = 'docs/split/contracts.md'
source = (root / name).read_text(encoding='utf-8')
needle = 'browser validator. Complete original native replies remain copy/export authority.\n'
assert source.count(needle) == 1
proposed[name] = source.replace(needle, needle + '''
Ranked-origin display decoding preserves native Open attribution as an empty
`project_ids` array through publication-reference restoration, combined rows and
file-specific follow-up selectors. Empty attribution creates no Project and
grants no read or editor authority. Existing exact index/source-root/path,
publication, bounded unique Project IDs and original receipt checks remain;
the host still requires an actual retained explicit Open selection before a
projectless source read or editor effect. Workbench plan admission retains its
separate nonempty Project requirement. Regression controls live in
`packages/vscode/src/test/nativeOpenOrigins.test.ts`.
''')

proposed['packages/vscode/src/test/nativeOpenOrigins.test.ts'] = '''// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import {materializeNativeMetadata} from '../nativeMetadata';
import {EvidenceClient} from '../explorerEvidence';
import {combinedRows,originArgs,originatingResult} from '../explorerSearch';
import {evidenceRows} from '../workbenchRender';
import {answerSourceBinding} from '../workbenchAnswerNavigation';
import type {LaunchSpec} from '../model';
type Value=Record<string,any>;
const index='idx_'+'12'.repeat(16),root='src_'+'23'.repeat(16),hash='34'.repeat(32);
const publication={collection:'fixture',incarnation:'45'.repeat(16),generation:7,revision:'56'.repeat(32),token:`airk-r1:${'45'.repeat(16)}:7:${'56'.repeat(32)}`};
function fixture():Value {
 const receipt={schema:'airk.saved-source-verification/1',method:'sha256-full-saved-source/1',status:'unverified',
  origins:[{id:'origin',index_id:index,project_ids:[],publication:publication.token}],
  sources:[{id:'source',origin_id:'origin',path:'fixture.py',expected_hash:hash,source_root_id:root,source_relative_path:'fixture.py',reference_ids:['ref']}],
  references:[{id:'ref',source_id:'source',kind:'file_path',result_pointer:''}]};
 return {schema:'airk.combined-read/1',selection:{project_ids:[],index_ids:[index],selection:{owner:'agent',catalog_revision:7,scope_identity:'67'.repeat(32)}},
  publication_vector:{authority:'78'.repeat(32),members:[{index_id:index,publication}]},
  members:[{index_id:index,outcome:'complete',is_error:false,result:{path:'fixture.py',collection:'fixture',content_hash:hash,publication:{member:0},
   read:{publication:{member:0},current_publication:{member:0},status:'content'},source:{root:'/fictional/source',relative_path:'fixture.py'},source_verification:receipt,verification_owner_publication:publication}}],
  ranking:[{index_id:index,result_pointer:'/members/0/result',origin:{index_id:index,source_root_id:root,source_relative_path:'fixture.py',project_ids:[],publication_ref:{member:0}}}],
  delivery_observation:{receipt:{opaque:'raw-only'}}};
}
test('Open file metadata restores all conditional publication references and preserves raw attribution and source receipts',()=>{
 const raw=fixture(),before=structuredClone(raw),logical=materializeNativeMetadata(raw),body=(logical.members as Value[])[0].result as Value;
 assert.deepEqual(body.publication,publication);assert.deepEqual(body.read.publication,publication);assert.deepEqual(body.read.current_publication,publication);
 assert.deepEqual(body.source_verification,raw.members[0].result.source_verification);
 assert.deepEqual(body.verification_owner_publication,publication);
 assert.deepEqual((logical.ranking as Value[])[0].origin,{index_id:index,source_root_id:root,source_relative_path:'fixture.py',project_ids:[],publication});
 assert.equal(logical.delivery_observation,undefined);assert.deepEqual(raw,before);
 const binding=answerSourceBinding(body,'fixture.py',hash,index,publication);
 assert.deepEqual(binding,{index_id:index,project_ids:[],source_root_id:root,source_relative_path:'fixture.py',publication});
});
test('Open combined display and source follow-ups preserve empty Project attribution for inline and factored publications',()=>{
 for(const factored of [false,true]){
  const raw=fixture();if(!factored){raw.ranking[0].origin.publication=publication;delete raw.ranking[0].origin.publication_ref;raw.members[0].result.publication=publication;raw.members[0].result.read={publication,current_publication:publication};}
  const logical=materializeNativeMetadata(raw),before=structuredClone(logical),rows=combinedRows(logical)!;
  assert.equal(evidenceRows(logical).length,1);assert.equal(rows.length,1);assert.deepEqual(rows[0].origin.project_ids,[]);
  assert.deepEqual(originArgs(rows[0].origin),{project_ids:[],index_ids:[index],origin_publication:publication.token});
  assert.deepEqual(originatingResult(logical,rows[0].origin).payload,(logical.members as Value[])[0].result);
  const wrongRoot={...rows[0].origin,source_root_id:'src_'+'99'.repeat(16)};
  assert.throws(()=>originatingResult(logical,wrongRoot),/source-root identity/);assert.deepEqual(logical,before);
 }
});
test('EvidenceClient decodes projectless Open file_info while retaining exact raw metadata and unchanged request selectors',async()=>{
 const raw=fixture(),before=structuredClone(raw),requests:unknown[]=[];
 const spec:LaunchSpec={command:'/fictional/airkctl',root:'/fictional/source',dataDir:'/fictional/data',collection:'fixture',name:'Open fixture',service:{installDir:'/fictional',project:'ab'.repeat(32)}};
 const client=new EvidenceClient(spec,'test',{transport:()=>({closed:false,close(){},async request(_method,params){requests.push(structuredClone(params));return {isError:false,content:[{type:'text',text:JSON.stringify(raw)}],structuredContent:structuredClone(raw)};}})});
 const args={path:'fixture.py',known_hash:hash,revision:publication.token,project_ids:[],index_ids:[index]};
 try{const result=await client.call('airk_file_info',args);assert.deepEqual(requests,[{name:'airk_file_info',arguments:args}]);assert.deepEqual(result.rawPayload,raw);
  const rows=combinedRows(result.payload)!;assert.equal(rows.length,1);assert.deepEqual(rows[0].origin.project_ids,[]);assert.deepEqual(result.payload.members&&((result.payload.members as Value[])[0].result as Value).source_verification,raw.members[0].result.source_verification);
  assert.deepEqual(raw,before);
 }finally{client.dispose();}
});
test('Open metadata keeps malformed project, publication, index, root and path refusals',()=>{
 const mutations:Array<(value:Value)=>void>=[v=>v.ranking[0].origin.project_ids=null,v=>v.ranking[0].origin.project_ids=['invalid'],v=>v.ranking[0].origin.project_ids=['prj_'+'01'.repeat(16),'prj_'+'01'.repeat(16)],
  v=>v.ranking[0].origin.project_ids=Array.from({length:33},(_,i)=>'prj_'+i.toString(16).padStart(32,'0')),v=>v.ranking[0].origin.index_id='idx_'+'99'.repeat(16),v=>v.ranking[0].origin.source_root_id='invalid',
  v=>v.ranking[0].origin.source_relative_path='../outside',v=>v.ranking[0].origin.source_relative_path='C:outside',v=>v.ranking[0].origin.publication_ref={member:1},v=>v.ranking[0].origin.publication_ref={member:0,extra:true}];
 for(const mutate of mutations){const raw=fixture();mutate(raw);const before=structuredClone(raw);assert.throws(()=>materializeNativeMetadata(raw));assert.deepEqual(raw,before);}
 for(const projects of [null,['invalid'],['prj_'+'01'.repeat(16),'prj_'+'01'.repeat(16)]]){const logical=materializeNativeMetadata(fixture());(logical.ranking as Value[])[0].origin.project_ids=projects;assert.throws(()=>combinedRows(logical));}
});
'''

(destination/'proposed-sources.json').write_text(json.dumps(proposed,indent=2)+'\n',encoding='utf-8')
patch=[]
for name, after in proposed.items():
    before=(root/name).read_text(encoding='utf-8') if (root/name).exists() else ''
    patch.extend(difflib.unified_diff(before.splitlines(keepends=True),after.splitlines(keepends=True),fromfile='a/'+name if before else '/dev/null',tofile='b/'+name))
(destination/'projectless-origin.patch').write_text(''.join(patch),encoding='utf-8')
print(json.dumps({'public_files_written':False,'proposed_files':list(proposed),'patch':str(destination/'projectless-origin.patch')}))
