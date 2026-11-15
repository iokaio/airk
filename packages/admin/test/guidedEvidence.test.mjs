// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import {build} from '../../vscode/node_modules/esbuild/lib/main.js';
import {readFile} from 'node:fs/promises';
const result=await build({entryPoints:[new URL('../src/guidedEvidence.ts',import.meta.url).pathname.replace(/^\/([A-Za-z]:)/,'$1')],bundle:true,write:false,format:'esm',platform:'browser'});
const api=await import(`data:text/javascript;base64,${Buffer.from(result.outputFiles[0].text).toString('base64')}`);
const manifest=JSON.parse(await readFile(new URL('../../ui/generated/help-manifest.json',import.meta.url),'utf8'));
const fields={name:'refreshSession',path:'src/session.rs',prefix:'src/',kind:'requirement',since:'opaque-native-original',exact:true,limit:50,text:'src/session.rs:3'};
test('seven named guided inputs target the existing canonical read tools and preserve future native options',()=>{
 const expected=[['code/symbols','code_facts'],['code/references','references'],['code/outline','outline'],['docs/facts','document_facts'],['evidence/inspector','file_info'],['evidence/changes','changes']];
 for(const [route,tool]of expected){const read=api.guidedEvidenceRequest(route,fields,{verification:'require',verification_max_sources:3,future:{all:['kept']}});assert.equal(read.tool,tool);assert.deepEqual(read.args.future,{all:['kept']});assert.equal(read.args.verification,'require');assert.ok(manifest.tools.find(t=>t.name===tool));}
 assert.equal(api.GUIDED_EVIDENCE_ROUTES.length,7);assert.equal(api.isGuidedEvidenceRoute('review/git'),false);
});
test('file metadata does not receive unsupported common count or byte arguments and outline uses its actual source path',()=>{
 assert.deepEqual(api.guidedEvidenceRequest('evidence/inspector',fields),{tool:'file_info',args:{path:'src/session.rs'}});
 assert.deepEqual(api.guidedEvidenceRequest('code/outline',{...fields,limit:100}),{tool:'outline',args:{path:'src/session.rs',limit:100,max_bytes:32768}});
 for(const route of ['evidence/inspector','code/outline'])assert.throws(()=>api.guidedEvidenceRequest(route,{...fields,path:' '}),/relative file path/);
});
test('references and changes require meaningful native inputs and pasted citation checking cannot become a fake native tool',()=>{
 assert.throws(()=>api.guidedEvidenceRequest('code/references',{...fields,name:''}),/exact-case identifier/);
 assert.throws(()=>api.guidedEvidenceRequest('evidence/changes',{...fields,since:''}),/publication token/);
 assert.throws(()=>api.guidedEvidenceRequest('evidence/checker',fields),/existing parser/);
 assert.equal(api.guidedEvidenceRequest('code/symbols',{...fields,name:'',exact:true}).tool,'symbols');
});

test('open native kind labels and complete advanced selectors survive guided document inputs',()=>{
 assert.equal(api.guidedEvidenceRequest('docs/facts',{...fields,kind:'publisher_custom_kind'},{document_key:'nativekey',language:'native/profile',reply_detail:'rich'}).args.kind,'publisher_custom_kind');
 const read=api.guidedEvidenceRequest('docs/facts',{...fields,kind:'any'},{kind:'native_other',document_key:'opaque/native',verification_deadline_ms:5000});
 assert.deepEqual([read.args.kind,read.args.document_key,read.args.verification_deadline_ms],['native_other','opaque/native',5000]);
});

test('guided native path names prefixes and opaque baseline bytes are preserved rather than trimmed or normalized',()=>{
 const supplied={...fields,path:' src/日本語.rs ',prefix:' literal prefix/',name:'qualified name ',since:' opaque-baseline '};
 assert.equal(api.guidedEvidenceRequest('evidence/inspector',supplied).args.path,' src/日本語.rs ');
 assert.equal(api.guidedEvidenceRequest('code/symbols',supplied).args.name,'qualified name ');
 assert.equal(api.guidedEvidenceRequest('docs/facts',supplied).args.path_prefix,' literal prefix/');
 assert.equal(api.guidedEvidenceRequest('evidence/changes',supplied).args.since,' opaque-baseline ');
});

test('guided file inventory uses the owning list_files prefix spelling and generated native arguments',()=>{
 const request=api.guidedIndexedFiles(' exact prefix/');assert.deepEqual(request,{tool:'list_files',args:{prefix:' exact prefix/',limit:50,max_bytes:32768}});
 const properties=manifest.tools.find(tool=>tool.name==='list_files').inputSchema.properties;for(const key of Object.keys(request.args))assert.ok(Object.hasOwn(properties,key),key);assert.equal(Object.hasOwn(request.args,'path_prefix'),false);
 for(const route of ['code/symbols','code/references','code/outline','docs/facts','evidence/inspector','evidence/changes']){const value=api.guidedEvidenceRequest(route,fields),native=manifest.tools.find(tool=>tool.name===value.tool);for(const key of Object.keys(value.args))assert.ok(Object.hasOwn(native.inputSchema.properties,key),`${route}:${key}`);}
});

test('advanced native byte budgets are preserved and an explicitly chosen outline count is not replaced by a hidden default',()=>{
 for(const route of ['code/symbols','code/references','code/outline','docs/facts','evidence/changes'])assert.equal(api.guidedEvidenceRequest(route,{...fields,limit:7},{max_bytes:1234}).args.max_bytes,1234,route);
 assert.equal(api.guidedEvidenceRequest('code/outline',{...fields,limit:7}).args.limit,7);
});
test('explicit native code fact selectors cannot silently switch to the symbols tool when the visible name is blank',()=>{
 assert.equal(api.guidedEvidenceRequest('code/symbols',{...fields,name:''},{symbol_key:'opaque-native-symbol-key'}).tool,'code_facts');
 assert.equal(api.guidedEvidenceRequest('code/symbols',{...fields,name:''},{kind:'route_registration'}).tool,'code_facts');
});
