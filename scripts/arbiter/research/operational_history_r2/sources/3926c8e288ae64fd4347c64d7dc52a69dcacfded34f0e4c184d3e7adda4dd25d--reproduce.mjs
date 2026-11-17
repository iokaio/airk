// SPDX-License-Identifier: Apache-2.0
// Private retained-packet diagnostic. No native/browser process or public file writes.
import assert from 'node:assert/strict';
import {readFile,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {build} from '../../../../packages/admin/node_modules/esbuild/lib/main.js';
const root=process.cwd(),codecPath=resolve('packages/ui/src/nativeMetadataCodec.ts');
const input='artifacts/advanced-split/receipts/runtime-r60/native-fixtures/airk-service-clients-0it75yai/workbench-observations.json.failure.json';
const bytes=await readFile(input),failure=JSON.parse(bytes),raw=failure.error.nativeResponse.structuredContent,before=structuredClone(raw);
const sourcePaths=['packages/ui/src/nativeMetadataCodec.ts','packages/vscode/src/explorerSearch.ts','packages/vscode/src/workbenchRender.ts'];
const identities={};for(const path of sourcePaths)identities[path]=createHash('sha256').update(await readFile(path)).digest('hex');
async function owners(proposed){
 const compiled=await build({stdin:{contents:"export {nativeMetadataOwner} from './packages/vscode/src/nativeMetadataOwner';export {combinedRows,originatingResult} from './packages/vscode/src/explorerSearch';export {answerSourceBinding} from './packages/vscode/src/workbenchAnswerNavigation';",resolveDir:root},bundle:true,write:false,platform:'node',format:'esm',plugins:[{name:'isolated-proposed-open-decoder',setup(p){
  p.onResolve({filter:/nativeMetadataCodec$/},()=>({path:codecPath}));
  p.onLoad({filter:/(nativeMetadataCodec|explorerSearch|workbenchRender)\.ts$/},async args=>{const source=await readFile(args.path,'utf8');return {contents:proposed?source.replace(/\|\|\s*!origin\.project_ids\.length/g,''):source,loader:'ts',resolveDir:resolve(args.path,'..')};});
 }}]});
 return import('data:text/javascript;base64,'+Buffer.from(compiled.outputFiles[0].contents).toString('base64'));
}
const current=await owners(false);let originalError;
try{current.nativeMetadataOwner.materializeNativeMetadata(raw);}catch(error){originalError=error.message;}
assert.equal(originalError,'Ranked publication reference crosses index scope.');
const api=await owners(true),logical=api.nativeMetadataOwner.materializeNativeMetadata(raw),rows=api.combinedRows(logical);
assert.equal(rows.length,1);assert.deepEqual(rows[0].origin.project_ids,[]);
const origin=rows[0].origin,body=logical.members[0].result;
const qualified=api.answerSourceBinding(body,body.path,body.content_hash,origin.index_id,origin.publication);
assert.ok(qualified);assert.deepEqual(qualified.project_ids,[]);
assert.deepEqual(api.originatingResult(logical,origin).payload,body);
assert.deepEqual(body.read.publication,logical.publication_vector.members[0].publication);
assert.deepEqual(body.read.current_publication,body.publication);
assert.deepEqual(raw,before);assert.deepEqual(body.source_verification,raw.members[0].result.source_verification);
let refused=0;
for(const mutate of [v=>v.ranking[0].origin.publication_ref={member:1},v=>v.ranking[0].origin.index_id='idx_'+'00'.repeat(16),v=>v.ranking[0].origin.source_relative_path='../outside',v=>v.ranking[0].origin.project_ids=['invalid'],v=>v.ranking[0].origin.project_ids=['prj_'+'01'.repeat(16),'prj_'+'01'.repeat(16)]]){
 const invalid=structuredClone(raw);mutate(invalid);assert.throws(()=>api.nativeMetadataOwner.materializeNativeMetadata(invalid));refused++;
}
const result={schema:1,command:'node artifacts/advanced-split/receipts/workbench-projectless-decoder-investigation/reproduce.mjs',input,input_sha256:createHash('sha256').update(bytes).digest('hex'),source_sha256:identities,original_error:originalError,proposal_only:true,actual_retained_packet_decoded:true,qualified_rows:rows.length,exact_source_receipt_preserved:true,source_binding_observed:true,projectless_attribution_preserved:true,conditional_publications_restored:true,raw_unchanged:true,malformed_refusals:refused,public_files_written:false};
await writeFile(new URL('result.json',import.meta.url),JSON.stringify(result,null,2)+'\n');
console.log(JSON.stringify(result));
