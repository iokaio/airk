// SPDX-License-Identifier: Apache-2.0
// Private replay of unchanged real R3 failure bytes; no live transport/editor effects.
import assert from 'node:assert/strict';
import {readFile,writeFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const {EvidenceClient}=require('../../../../packages/vscode/out/explorerEvidence.js');
const {combinedRows,originArgs,originatingResult}=require('../../../../packages/vscode/out/explorerSearch.js');
const {answerSourceBinding}=require('../../../../packages/vscode/out/workbenchAnswerNavigation.js');
const input='artifacts/advanced-split/receipts/runtime-r60/native-fixtures/airk-service-clients-0it75yai/workbench-observations.json.failure.json';
const bytes=await readFile(input),failure=JSON.parse(bytes),envelope=failure.error.nativeResponse,raw=envelope.structuredContent,before=structuredClone(envelope);
assert.deepEqual(JSON.parse(envelope.content[0].text),raw);
const requests=[],spec={command:'/diagnostic/airkctl',root:'/diagnostic/source',dataDir:'/diagnostic/data',collection:'fixture',name:'Private retained replay',service:{installDir:'/diagnostic',project:'ab'.repeat(32)}};
const index=raw.members[0].index_id,original=raw.members[0].result;
const arguments_={path:original.path,known_hash:original.content_hash,revision:raw.publication_vector.members[0].publication.token,project_ids:[],index_ids:[index]};
const client=new EvidenceClient(spec,'diagnostic',{transport:()=>({closed:false,close(){},async request(method,params){assert.equal(method,'tools/call');requests.push(structuredClone(params));return structuredClone(envelope);}})});
try{
 const result=await client.call('airk_file_info',arguments_),rows=combinedRows(result.payload);
 assert.equal(rows.length,1);assert.deepEqual(rows[0].origin.project_ids,[]);
 assert.deepEqual(originArgs(rows[0].origin),{project_ids:[],index_ids:[index],origin_publication:arguments_.revision});
 const body=result.payload.members[0].result;
 assert.deepEqual(originatingResult(result.payload,rows[0].origin).payload,body);
 const binding=answerSourceBinding(body,original.path,original.content_hash,index,rows[0].origin.publication);
 assert.ok(binding);assert.deepEqual(binding.project_ids,[]);
 assert.deepEqual(body.read.publication,raw.publication_vector.members[0].publication);
 assert.deepEqual(body.read.current_publication,body.publication);
 assert.deepEqual(body.source_verification,original.source_verification);
 assert.deepEqual(result.rawPayload,raw);assert.deepEqual(envelope,before);
 assert.deepEqual(requests,[{name:'airk_file_info',arguments:arguments_}]);
 const identities={};for(const path of ['packages/ui/src/nativeMetadataCodec.ts','packages/vscode/src/explorerSearch.ts','packages/vscode/src/workbenchRender.ts','packages/vscode/src/test/nativeOpenOrigins.test.ts','packages/vscode/src/nativeMetadataCodec.js'])identities[path]=createHash('sha256').update(await readFile(path)).digest('hex');
 const receipt={schema:1,command:'artifacts/tools/node-v24.15.0-win-x64/node.exe artifacts/advanced-split/receipts/workbench-projectless-decoder-investigation/replay_fixed.mjs',node:process.version,input,input_sha256:createHash('sha256').update(bytes).digest('hex'),source_sha256:identities,actual_retained_packet_decoded:true,evidence_client_ingress:true,ranked_rows:rows.length,exact_raw_and_source_receipt_preserved:true,conditional_publications_restored:true,projectless_source_binding:true,unchanged_request_selectors:true,live_native_or_browser_execution:false,editor_effect:false};
 await writeFile(new URL('fixed-replay.json',import.meta.url),JSON.stringify(receipt,null,2)+'\n');console.log(JSON.stringify(receipt));
}finally{client.dispose();}
