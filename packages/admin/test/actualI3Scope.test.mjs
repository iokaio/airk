// SPDX-License-Identifier: Apache-2.0
// Complete packets from a separately observed real TLS/I3 fixture. The browser
// callbacks replay those exact packets; they do not replace live transport proof.
import assert from 'node:assert/strict';
import test from 'node:test';
import {readFile} from 'node:fs/promises';
import {createHash,webcrypto} from 'node:crypto';
import {createServer} from 'node:http';
import {once} from 'node:events';
import {resolve,join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {build} from 'esbuild';
import {browserAvailable,browserHarness} from '../../ui/test/browserHarness.mjs';

const root=fileURLToPath(new URL('../',import.meta.url));
const capture=process.env.AIRK_ACTUAL_I3_SCOPE_CAPTURE;
const skip=capture?false:'Set AIRK_ACTUAL_I3_SCOPE_CAPTURE to a complete private native capture.';
async function packets(){
 const directory=resolve(capture),read=async name=>JSON.parse(await readFile(join(directory,name),'utf8'));
 const execution=await read('execution.json');assert.equal(execution.complete,true);assert.deepEqual(execution.post_input_drift,[]);assert.equal(execution.fixture_removed,true);
 assert.equal(execution.result.cleanup.managed_stop.record.phase,'complete');
 const grant=await read('grant.json'),result=await read('read.json'),pages={};
 for(const kind of ['indexes','projects','memberships'])pages[kind]=await read(`catalog-${kind}.json`);
 const grantRaw=await readFile(join(directory,'grant.response.raw')),readRaw=await readFile(join(directory,'read.response.raw'));
 assert.equal(createHash('sha256').update(grantRaw).digest('hex'),'2a47cb34a1c4a7716363d7e649ffe48edbf138843f6cf4e69f4717373a2ad6e3');
 assert.equal(createHash('sha256').update(readRaw).digest('hex'),'ac626fa8150aa9d489175dfd0e0902a4ebeca8ecf51ad2c92030ccbe47955a87');
 assert.deepEqual(JSON.parse(grantRaw),grant.envelope);assert.deepEqual(JSON.parse(readRaw),result.envelope);
 const body=result.envelope.result.result.structuredContent;
 assert.notEqual(body.publication_vector.authority,grant.envelope.result.scope_identity);
 assert.equal(body.selection.selection.owner,'admin');
 assert.equal(body.selection.selection.scope_identity,grant.envelope.result.scope_identity);
 return {grant,result,pages,execution};
}
async function owners(){
 const compiled=await build({absWorkingDir:root,stdin:{contents:"export {createSearchWorkspace,searchBody} from './src/SearchScreen';export {createBrowserNativeMetadataCodec} from '../ui/src/nativeMetadataCodec';",resolveDir:root},bundle:true,write:false,format:'esm',platform:'node',jsx:'automatic',jsxImportSource:'preact'});
 return import(`data:text/javascript;base64,${Buffer.from(compiled.outputFiles[0].contents).toString('base64')}`);
}

test('actual native Admin grant and distinct combined authority pass current controller and asynchronous metadata owner',{skip},async()=>{
 const fixture=await packets(),{createSearchWorkspace,createBrowserNativeMetadataCodec}=await owners();
 let session={},held=null;const calls=[],codec=createBrowserNativeMetadataCodec(webcrypto.subtle);
 const owner=createSearchWorkspace({session:()=>session,changed(){},materialize:(raw,check)=>codec.materializeNativeMetadata(raw,check),async run(operation,request){
  calls.push({operation,request:structuredClone(request)});
  if(operation==='catalog.page')return {kind:'ok',status:200,envelope:structuredClone(fixture.pages[request.catalog_page.request.kind].envelope)};
  if(operation==='search.select_scope'){assert.deepEqual(request,fixture.grant.request);return {kind:'ok',status:200,envelope:structuredClone(fixture.grant.envelope)};}
  assert.equal(operation,'search.call');assert.deepEqual(request,fixture.result.request);
  return held?await held:{kind:'ok',status:200,envelope:structuredClone(fixture.result.envelope)};
 }});
 const selection=fixture.grant.request.admin_scope_grant.selection;
 await owner.reload();owner.choose(selection.project_ids,selection.index_ids);await owner.apply();
 const result=await owner.execute('search',fixture.result.request.tools_call.arguments,'full');
 assert.ok(result);assert.equal(owner.view().verifiedScope,true);assert.deepEqual(result.raw,fixture.result.envelope.result);
 assert.equal(result.logical.members.length,2);assert.ok(result.logical.members.every(member=>member.result.scope_observation.selection.owner==='admin'));
 assert.equal(result.scopeIdentity,fixture.grant.envelope.result.scope_identity);
 const snapshot=structuredClone(fixture.result.envelope);assert.deepEqual(fixture.result.envelope,snapshot);
 let release;held=new Promise(resolve=>release=resolve);const pending=owner.execute('search',fixture.result.request.tools_call.arguments,'full');
 session={replacement:true};owner.view();release({kind:'ok',status:200,envelope:structuredClone(fixture.result.envelope)});await pending;
 assert.equal(owner.view().logicalBody,null);assert.equal(owner.view().applied,null);
 owner.dispose();
});

test('actual I3 catalog grant and full compressed read enter real Search DOM with Web Crypto and exact copy',{skip:skip||(!browserAvailable?'Chromium unavailable.':false)},async t=>{
 const fixture=await packets();let browser,primary,stage='start-owned-local-document';const server=createServer((_,response)=>{response.setHeader('Content-Type','text/html');response.end('<!doctype html><div id="root"></div>');});
 server.listen(0,'127.0.0.1');await once(server,'listening');
 try {
  stage='start-owned-browser';browser=await browserHarness();stage='navigate-owned-loopback-document';await browser.call('Page.navigate',{url:`http://127.0.0.1:${server.address().port}/`});await browser.observed(`document.getElementById('root')!==null`);
  assert.equal(await browser.evaluate('typeof crypto.subtle.digest'), 'function');
  await browser.evaluate(`window.nativePackets=${JSON.stringify(fixture)};window.calls=[];window.session={nativeCapture:true};window.listeners=new Set();window.copied=undefined;Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async text=>window.copied=text}});window.testRun=async(operation,request)=>{window.calls.push({operation,request});const p=window.nativePackets;let envelope;if(operation==='catalog.page')envelope=p.pages[request.catalog_page.request.kind].envelope;else if(operation==='search.select_scope')envelope=p.grant.envelope;else if(operation==='search.call')envelope=p.result.envelope;else return {kind:'transport',status:null,message:'Optional metadata was not part of this native capture.'};return {kind:'ok',status:200,envelope:structuredClone(envelope)};};`);
  const bundle=await build({absWorkingDir:root,stdin:{contents:"import {h,render} from 'preact';import {Search} from './src/SearchScreen';render(h(Search,{}),document.getElementById('root'));",resolveDir:root},bundle:true,write:false,format:'iife',platform:'browser',jsx:'automatic',jsxImportSource:'preact',plugins:[{name:'captured-native-i3',setup(plugin){
   plugin.onResolve({filter:/^\.\/api$/},()=>({path:'api',namespace:'actual-i3'}));plugin.onLoad({filter:/.*/,namespace:'actual-i3'},()=>({contents:'export const call=(...args)=>window.testRun(...args);export const isUnknownOutcome=()=>false;export const preparationAdmissionRefused=()=>false;export const preparationOf=()=>null;export const reviewOf=()=>null;'}));
   plugin.onResolve({filter:/^\.\/session$/},()=>({path:'session',namespace:'actual-session'}));plugin.onLoad({filter:/.*/,namespace:'actual-session'},()=>({contents:'export const currentSession=()=>window.session;export const onSessionChange=listener=>{window.listeners.add(listener);return()=>window.listeners.delete(listener);};'}));
  }}]});stage='mount-real-search';await browser.evaluate(bundle.outputFiles[0].text);
  const selected=fixture.grant.request.admin_scope_grant.selection;
  const projectRows=fixture.pages.projects.envelope.result.page.items.filter(row=>row.kind==='project');
  for(const id of selected.project_ids){const offset=projectRows.findIndex(row=>row.value.id===id);assert.ok(offset>=0);await browser.observed(`document.querySelectorAll('fieldset').length>0`);await browser.evaluate(`(()=>{const field=[...document.querySelectorAll('fieldset')].find(node=>node.querySelector('legend')?.textContent==='Named index sets');field.querySelectorAll('input')[${offset}].click();})()`);await browser.observed(`[...document.querySelectorAll('fieldset')].find(node=>node.querySelector('legend')?.textContent==='Named index sets').querySelectorAll('input:checked').length===${selected.project_ids.indexOf(id)+1}`);}
  for(let offset=0;offset<selected.index_ids.length;offset++){await browser.evaluate(`(()=>{const field=[...document.querySelectorAll('fieldset')].find(node=>node.querySelector('legend')?.textContent==='Indexes in these sets');field.querySelectorAll('input')[${offset}].click();})()`);await browser.observed(`[...document.querySelectorAll('fieldset')].find(node=>node.querySelector('legend')?.textContent==='Indexes in these sets').querySelectorAll('input:checked').length===${offset+1}`);}
  await browser.evaluate(`[...document.querySelectorAll('button')].find(node=>node.textContent==='Use selected sources').click()`);await browser.observed(`document.body.textContent.includes('Applied scope:')`);
  await browser.evaluate(`(()=>{const label=[...document.querySelectorAll('label')].find(node=>node.textContent.startsWith('Use an advanced native tool'));label.querySelector('input').click();const area=document.querySelector('form textarea');area.value=JSON.stringify(window.nativePackets.result.request.tools_call.arguments);area.dispatchEvent(new Event('input',{bubbles:true}));})()`);
  await browser.observed(`!document.querySelector('form button[type="submit"]').disabled`);await browser.evaluate(`document.querySelector('form').dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}))`);
  await browser.observed(`document.body.textContent.includes('boundaryscopeproof')&&[...document.querySelectorAll('button')].some(node=>node.textContent==='Copy complete JSON')`);
  await browser.evaluate(`[...document.querySelectorAll('button')].find(node=>node.textContent==='Copy complete JSON').click()`);await browser.observed(`window.copied!==undefined`);
  assert.deepEqual(JSON.parse(await browser.evaluate('window.copied')),fixture.result.envelope.result);
  const calls=await browser.evaluate('window.calls');assert.deepEqual(calls.find(call=>call.operation==='search.select_scope').request,fixture.grant.request);assert.deepEqual(calls.find(call=>call.operation==='search.call').request,fixture.result.request);
  assert.equal(await browser.evaluate(`document.body.textContent.includes('Search scope could not be confirmed')||document.body.textContent.includes('Native metadata could not be restored')`),false);
 } catch(error) {primary=error;t.diagnostic(JSON.stringify({failed_stage:stage,primary_error:String(error)}));throw error;}
 finally {try{await browser?.close();}catch(cleanup){t.diagnostic(JSON.stringify({cleanup_error:String(cleanup)}));if(!primary)throw cleanup;}finally{const closed=once(server,'close');server.close();await closed;t.diagnostic(JSON.stringify({browser_ownership:browser?.observation}));}}
});
