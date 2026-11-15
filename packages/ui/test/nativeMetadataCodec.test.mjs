// SPDX-License-Identifier: Apache-2.0
// Both adapters exercise the same owner; native golden literals remain independent.
import assert from 'node:assert/strict';
import test from 'node:test';
import {createHash,webcrypto} from 'node:crypto';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
import {readFile} from 'node:fs/promises';
const require=createRequire(new URL('../../vscode/package.json',import.meta.url)),{build}=require('esbuild');
const compiled=await build({entryPoints:[fileURLToPath(new URL('../src/nativeMetadataCodec.ts',import.meta.url))],bundle:true,write:false,format:'esm',platform:'node'});
const actual=await import(`data:text/javascript;base64,${Buffer.from(compiled.outputFiles[0].text).toString('base64')}`);
const node=actual.createNativeMetadataCodec(bytes=>createHash('sha256').update(bytes).digest('hex'));
const index=n=>'idx_'+n.toString(16).padStart(32,'0');
const publication=n=>({collection:'fixture',incarnation:'11'.repeat(16),generation:n,revision:'22'.repeat(32),token:`airk-r1:${'11'.repeat(16)}:${n}:${'22'.repeat(32)}`});
function headerFixture(){
 const template={fields:{freshness:{state:'idle',observed_unix_ms:null},collection:'fixture'},slots:[{path:['freshness','observed_unix_ms'],kind:'integer'}]},id=node.nativeHeaderTemplateDigest(template);
 const members=[1,2].map(n=>{const body={path:`source-${n}.rs`,publication:{member:n-1},[actual.NATIVE_HEADER_REFERENCE]:{id,values:[n]},future_inline:{all:['retained','é中']}};return {index_id:index(n),result:{content:[{type:'text',text:JSON.stringify(body)}],structuredContent:body,isError:false,future_wrapper:{complete:true}}};});
 return {schema:'airk.combined-read/1',shared_values:{schema:'airk.shared-native-header-templates/1',values:{[id]:template}},publication_vector:{members:[1,2].map(n=>({index_id:index(n),publication:publication(n)}))},members,future_outer:{complete:true}};
}
function capabilityFixture(){const fields={workbench:{'10':'a','2':'b'},adapters:[{id:'actual'}]},id=node.sharedCapabilityDigest(fields);return {schema:'airk.combined-read/1',shared_values:{schema:'airk.shared-capability-fields/1',values:{[id]:fields}},members:[{index_id:index(1),result:{[actual.SHARED_CAPABILITY_REFERENCE]:id,future_inline:{all:true}}}]};}

test('framed bytes and canonical digests retain the exact independent native goldens',()=>{
 const domain=Buffer.from('airk/shared-capability-fields/1'),body=Buffer.from('{"workbench":{"10":"a","2":"b"}}');
 const length=bytes=>{const value=Buffer.alloc(8);value.writeBigUInt64BE(BigInt(bytes.length));return value;};
 const framed=Buffer.concat([Buffer.from('AIRK-DIGEST-1\0'),length(domain),domain,length(body),body]);
 assert.deepEqual(Buffer.from(actual.framedNativeMetadataBytes(domain.toString(),{workbench:{'2':'b','10':'a'}})),framed);
 assert.equal(node.sharedCapabilityDigest({workbench:{'10':'a','2':'b'}}),'ba2e0fd072e154161d7a53c77f61ea42c8dc4f8d9f6f4beaf9f7dc0a9824e5e0');
 assert.equal(node.nativeHeaderTemplateDigest({fields:{freshness:{observed_unix_ms:null,state:'unavailable'}},slots:[{kind:'integer',path:['freshness','observed_unix_ms']}]}),'3c2b1d950550b1d7514895aa1645c730a1221132ba50b864215167c9d49f3bc9');
});
test('actual WebCrypto restores the same full values as Node without mutating any raw fields',async()=>{
 for(const raw of [headerFixture(),capabilityFixture()]){
  const before=structuredClone(raw),algorithms=[];
  const browser=actual.createBrowserNativeMetadataCodec({digest:(algorithm,bytes)=>{algorithms.push(algorithm);return webcrypto.subtle.digest(algorithm,bytes);}});
  assert.throws(()=>browser.codec.materializeNativeMetadata(raw),/exact response/);
  const restored=await browser.materializeNativeMetadata(raw);assert.deepEqual(restored,node.materializeNativeMetadata(raw));
  assert.deepEqual(raw,before);assert.ok(algorithms.length>0);assert.ok(algorithms.every(algorithm=>algorithm==='SHA-256'));
  assert.deepEqual(browser.codec.materializeNativeMetadata(raw),restored);
  assert.throws(()=>browser.codec.materializeNativeMetadata(structuredClone(raw)),/exact response/,'proof cache is exact raw object custody');
 }
});
test('browser rejects all strict unknown namespaces collisions slot and publication errors using the same owner',async()=>{
 const changes=[raw=>{raw.shared_values.schema='future/9';},raw=>{raw.shared_values.extra=true;},raw=>{Object.values(raw.shared_values.values)[0].fields.collection='changed';},raw=>{raw.members[0].result.structuredContent[actual.NATIVE_HEADER_REFERENCE].values=['wrong'];},raw=>{raw.members[0].result.structuredContent.publication.member=1;},raw=>{raw.members[0].result.structuredContent.collection='collision';}];
 for(const change of changes){const raw=headerFixture();change(raw);const before=structuredClone(raw);assert.throws(()=>node.materializeNativeMetadata(raw));await assert.rejects(actual.createBrowserNativeMetadataCodec(webcrypto.subtle).materializeNativeMetadata(raw));assert.deepEqual(raw,before);}
});
test('stale scope while real WebCrypto is held never publishes a verified response or partial graph',async()=>{
 let release,entered;const barrier=new Promise(resolve=>{entered=resolve;}),hold=new Promise(resolve=>{release=resolve;});let current=true,checks=0;
 const browser=actual.createBrowserNativeMetadataCodec({digest:async(algorithm,bytes)=>{entered();await hold;return webcrypto.subtle.digest(algorithm,bytes);}}),raw=headerFixture(),before=structuredClone(raw);
 const pending=browser.materializeNativeMetadata(raw,()=>{checks++;if(!current)throw Error('replaced_scope');});await barrier;current=false;release();await assert.rejects(pending,/replaced_scope/);assert.ok(checks>1);assert.throws(()=>browser.codec.materializeNativeMetadata(raw),/exact response/);assert.deepEqual(raw,before);
});
test('both backends retain original UTF8 structural and numeric refusal boundaries',async()=>{
 for(const fields of [{workbench:{value:0.5}},{workbench:{value:Number.MAX_SAFE_INTEGER+1}},{workbench:{value:'\ud800'}},{'naïve':true}])assert.throws(()=>node.sharedCapabilityDigest(fields));
 const raw=capabilityFixture(),id=Object.keys(raw.shared_values.values)[0];raw.shared_values.values[id].workbench={text:'x'.repeat(65536)};
 await assert.rejects(actual.createBrowserNativeMetadataCodec(webcrypto.subtle).materializeNativeMetadata(raw),/budget|table/);
});
test('browser bundle resolves Node bridge imports to the one original browser-safe owner',async()=>{
 const result=await build({stdin:{contents:`export {WORKBENCH_MAX_STEP_REPORTS} from './packages/vscode/src/workbenchContracts';export {createAnswerNavigation} from './packages/vscode/src/workbenchAnswerNavigation';export {createBrowserNativeMetadataCodec} from './packages/ui/src/nativeMetadataCodec';`,resolveDir:fileURLToPath(new URL('../../../',import.meta.url))},bundle:true,write:false,platform:'browser',format:'esm',metafile:true});
 const inputs=Object.keys(result.metafile.inputs).map(name=>name.replaceAll('\\','/'));
 assert.equal(inputs.filter(name=>name.endsWith('/ui/src/nativeMetadataCodec.ts')).length,1);
 assert.equal(inputs.some(name=>name.endsWith('/vscode/src/nativeMetadataCodec.js')||name.includes('nativeMetadataOwner')),false);
 assert.equal(Object.values(result.metafile.outputs).flatMap(output=>output.imports).length,0);
});


test('verified browser proof refuses inline slot wrapper and complete-raw mutations including removal of all references',async()=>{
 const mutations=[raw=>{raw.members[0].result.structuredContent.future_inline.all[0]='changed';},raw=>{raw.members[0].result.structuredContent[actual.NATIVE_HEADER_REFERENCE].values[0]=17;},raw=>{raw.members[0].result.future_wrapper.complete=false;},raw=>{raw.future_outer.complete=false;},raw=>{delete raw.shared_values;for(const member of raw.members)delete member.result.structuredContent[actual.NATIVE_HEADER_REFERENCE];}];
 for(const mutate of mutations){const browser=actual.createBrowserNativeMetadataCodec(webcrypto.subtle),raw=headerFixture();await browser.materializeNativeMetadata(raw);mutate(raw);assert.throws(()=>browser.codec.materializeNativeMetadata(raw),/changed/);await assert.rejects(browser.materializeNativeMetadata(raw),/changed/,'a replayed object cannot acquire a new snapshot');}
});
test('retained scope check also fences sync reuse and replay after successful verification for inline and encoded graphs',async()=>{
 for(const raw of [headerFixture(),{schema:'inline-future-class',source_data:{'日本語':0.125,constructor:'inert data',future:[1,2,3]}}]){
  let current=true;const browser=actual.createBrowserNativeMetadataCodec(webcrypto.subtle),check=()=>{if(!current)throw Error('replaced_after_verified');};
  const result=await browser.materializeNativeMetadata(raw,check);assert.deepEqual(raw.source_data,result.source_data,'arbitrary source numbers and Unicode keys are not canonical metadata');
  current=false;assert.throws(()=>browser.codec.materializeNativeMetadata(raw),/replaced_after_verified/);await assert.rejects(browser.materializeNativeMetadata(raw,()=>undefined),/replaced_after_verified/,'a fresh caller callback cannot rebind an old verified response');
  assert.throws(()=>browser.codec.materializeNativeMetadata(structuredClone(raw)),/exact response/);
 }
});

test('repeated async reuse retains one original custodian without recursive callback chains',async()=>{
 const browser=actual.createBrowserNativeMetadataCodec(webcrypto.subtle),raw={source_data:{value:0.125}};let originalChecks=0,current=true;
 const original=()=>{originalChecks++;if(!current)throw Error('original_scope_revoked');};
 await browser.materializeNativeMetadata(raw,original);originalChecks=0;
 for(let call=0;call<12000;call++)await browser.materializeNativeMetadata(raw,()=>undefined);
 assert.equal(originalChecks,24000,'exactly two original checks per reuse, independent of prior call count');
 assert.deepEqual(browser.codec.materializeNativeMetadata(raw),raw);current=false;
 await assert.rejects(browser.materializeNativeMetadata(raw,()=>undefined),/original_scope_revoked/);
});

test('generated Node owner authenticates captured runtime bytes rather than a mutable staged reread',()=>{
 const producer=require('./scripts/build-metadata-codec.cjs'),runtime=producer.outputs()[2][1];
 producer.verifyCaptured(runtime);const changed=Buffer.from(runtime);changed[changed.length-2]^=1;
 assert.throws(()=>producer.verifyCaptured(changed),/Captured metadata runtime differs/);
 assert.throws(()=>producer.verifyCaptured(Buffer.concat([runtime,Buffer.from('\n// extra')])) ,/Captured metadata runtime differs/);
});
