// SPDX-License-Identifier: Apache-2.0
// Real Preact browser callbacks over explicitly synthetic I3 responses.
import assert from 'node:assert/strict';
import {before,after,test} from 'node:test';
import {build} from 'esbuild';
import {fileURLToPath} from 'node:url';
import {readFile} from 'node:fs/promises';
import {browserAvailable,browserHarness} from '../../ui/test/browserHarness.mjs';
const root=fileURLToPath(new URL('../',import.meta.url));let browser;
before(async()=>{if(browserAvailable)browser=await browserHarness();});
after(async t=>{try{await browser?.close();}finally{t.diagnostic(JSON.stringify({browser_ownership:browser?.observation}));}});
test('browser guided Search chooses names without IDs, retains full advanced fields and cited output and copies exact complete native bytes',{skip:browserAvailable?false:'Chromium unavailable.'},async t=>{
 let stage='prepare-browser-fixture';
 try {
 const ns='12'.repeat(16),index='idx_'+'23'.repeat(16),project='prj_'+'34'.repeat(16),source='src_'+'45'.repeat(16);
 const pages={indexes:[{kind:'index',value:{id:index,name:'日本語 library',source_import:'complete',tombstone:false}}],projects:[{kind:'project',value:{id:project,name:'Application sources'}}],memberships:[{kind:'membership',value:{project_id:project,index_id:index,scope:[{source_root_id:source}]}}]};
 const raw={status:'tool_result',result:{content:[{type:'text',text:JSON.stringify({hits:[{text:'Untrusted <img src=x onerror="window.untrusted=true">',citation:{path:'api/é-日本語.rs',start_line:7,end_line:9,content_hash:'56'.repeat(32)}}],scope_observation:{selection:{scope_identity:'67'.repeat(32)}},truncated:true,warnings:['bounded source evidence'],source_verification:{status:'not_checked'},future_native:{kept:['every','field']}})}]}};
 await browser.evaluate(`document.body.innerHTML='<div id="root"></div>';window.calls=[];window.session={owner:'fixture'};window.pages=${JSON.stringify(pages)};window.nativeResult=${JSON.stringify(raw)};window.reads=[];window.testRun=async(operation,request)=>{window.calls.push({operation,request});let result;if(operation==='catalog.page')result={page:{namespace:'${ns}',revision:7,items:window.pages[request.catalog_page.request.kind],continuation:null}};else if(operation==='search.select_scope')result={status:'admin_scope_granted',catalog_revision:7,scope_identity:'${'67'.repeat(32)}'};else if(operation==='service.resources')result={status:'resources',configuration_error:false,observation:{configuration:{query_concurrency:2}}};else if(operation==='search.call'){window.reads.push(request);result=window.nativeResult;}return {kind:'ok',status:200,envelope:{schema:'airk-admin/1',ok:true,result}};};Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async text=>{window.copied=text;}}});`);
 stage='compile-search';
 const bundle=await build({absWorkingDir:root,stdin:{contents:"import {h,render} from 'preact';import {Search} from './src/SearchScreen';render(h(Search,{}),document.getElementById('root'));",resolveDir:root,sourcefile:'guided-search-browser.tsx'},bundle:true,write:false,format:'iife',platform:'browser',jsx:'automatic',jsxImportSource:'preact',plugins:[{name:'synthetic-i3',setup(plugin){plugin.onResolve({filter:/^\.\/api$/},()=>({path:'api',namespace:'search-fixture'}));plugin.onLoad({filter:/.*/,namespace:'search-fixture'},()=>({contents:'export const call=(...args)=>window.testRun(...args);export const isUnknownOutcome=()=>false;export const preparationAdmissionRefused=()=>false;export const preparationOf=()=>null;export const reviewOf=()=>null;'}));plugin.onResolve({filter:/^\.\/session$/},()=>({path:'session',namespace:'search-session'}));plugin.onLoad({filter:/.*/,namespace:'search-session'},()=>({contents:'export const currentSession=()=>window.session;'}));}}]});
 stage='mount-search';
 await browser.evaluate(bundle.outputFiles[0].text);
 stage='catalog-observation';
 await browser.observed(`document.body.textContent.includes('Application sources')`);
 assert.equal(await browser.evaluate(`document.querySelector('input[placeholder="idx_…"]')!==null`),false);
 stage='named-selection';
 await browser.evaluate(`(()=>{const label=[...document.querySelectorAll('label')].find(node=>node.textContent==='Application sources');label.querySelector('input').click();})()`);
 await browser.observed(`!([...document.querySelectorAll('button')].find(node=>node.textContent==='Use selected sources').disabled)`);
 await browser.evaluate(`[...document.querySelectorAll('button')].find(node=>node.textContent==='Use selected sources').click()`);
 await browser.observed(`document.body.textContent.includes('Applied scope: Application sources')`);
 await browser.evaluate(`(()=>{const input=[...document.querySelectorAll('label')].find(node=>node.textContent.startsWith('Query')).querySelector('input');input.value='owner cancellation';input.dispatchEvent(new Event('input',{bubbles:true}));const area=document.querySelector('textarea');area.value=JSON.stringify({limit_per_index:2,verification:'require',verification_max_sources:3,future_request:{kept:true}});area.dispatchEvent(new Event('input',{bubbles:true}));})()`);
 await browser.observed(`!document.querySelector('form button[type="submit"]').disabled`);
 stage='submit-native-search';
 await browser.evaluate(`document.querySelector('form').dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}))`);
 await browser.observed(`document.body.textContent.includes('api/é-日本語.rs')`);
 assert.equal(await browser.evaluate('window.untrusted'),undefined);
 const reads=await browser.evaluate('window.reads');assert.equal(reads.length,1);assert.deepEqual(reads[0].tools_call.arguments,{limit_per_index:2,verification:'require',verification_max_sources:3,future_request:{kept:true},query:'owner cancellation',mode:'lexical',limit:10});
 const selection=await browser.evaluate(`window.calls.find(call=>call.operation==='search.select_scope').request`);assert.deepEqual(selection,{admin_scope_grant:{selection:{namespace:ns,expected_catalog_revision:7,index_ids:[],project_ids:[project]}}});
 assert.ok(await browser.evaluate(`document.body.textContent.includes('bounded source evidence')&&document.body.textContent.includes('truncated')`));
 await browser.evaluate(`[...document.querySelectorAll('button')].find(node=>node.textContent==='Copy complete JSON').click()`);await browser.observed('window.copied!==undefined');assert.deepEqual(JSON.parse(await browser.evaluate('window.copied')),raw);
 await browser.evaluate(`[...document.querySelectorAll('button')].find(node=>node.textContent==='Copy complete Markdown').click()`);await browser.observed(`window.copied.startsWith('# AIRK indexed evidence')`);assert.ok((await browser.evaluate('window.copied')).includes(JSON.stringify(raw,null,2)));
 } catch(error) { t.diagnostic(JSON.stringify({stage,browser_ownership:browser?.observation}));throw error; }
});

// Unchanged Rust-emitted full combined packets enter the actual Search component.
// Only I3 catalog/grant transport callbacks are synthetic; this is UI qualification.
const nativeCases=JSON.parse(await readFile(new URL('../../../fixtures/split/arbiter/combined_answer_cases.json',import.meta.url),'utf8')).cases;
const contextCases=JSON.parse(await readFile(new URL('../../../fixtures/split/arbiter/context_action_cases.json',import.meta.url),'utf8')).cases;
const direct=contextCases.find(item=>item.accepted&&item.input?.records?.length&&item.input?.assessment_history?.length);
// Preserve every Rust packet field; the sole extra observation is the synthetic
// I3 transport scope header required by the actual Search admission callback.
nativeCases.push({name:'actual_context_packet_with_synthetic_i3_scope_observation',output:{...direct.input,scope_observation:{selection:{scope_identity:'07'.repeat(32)}}}});
for(const nativeCase of nativeCases)test(`guided Search shared Answer entrypoint: ${nativeCase.name}`,{skip:browserAvailable?false:'Chromium unavailable.'},async()=>{
 const raw={status:'tool_result',result:{structuredContent:nativeCase.output}};
 const ids=nativeCase.output.members?nativeCase.output.members.map(member=>member.index_id):[nativeCase.output.assessment.scope.index_id];
 const authority=nativeCase.output.publication_vector?.authority??nativeCase.output.scope_observation.selection.scope_identity;
 const ns='12'.repeat(16),project='prj_'+'34'.repeat(16);
 const pages={indexes:ids.map((id,i)=>({kind:'index',value:{id,name:`Native fixture index ${i+1}`,source_import:'complete',tombstone:false}})),projects:[{kind:'project',value:{id:project,name:'Native fixture sources'}}],memberships:ids.map((id,i)=>({kind:'membership',value:{project_id:project,index_id:id,scope:[{source_root_id:'src_'+String(i+1).padStart(32,'0')}]}}))};
 await browser.evaluate(`document.body.innerHTML='<div id="root"></div>';window.calls=[];window.session={owner:'fixture-${nativeCase.name}'};window.pages=${JSON.stringify(pages)};window.nativeResult=${JSON.stringify(raw)};window.testRun=async(operation,request)=>{window.calls.push({operation,request});let result;if(operation==='catalog.page')result={page:{namespace:'${ns}',revision:7,items:window.pages[request.catalog_page.request.kind],continuation:null}};else if(operation==='search.select_scope')result={status:'admin_scope_granted',catalog_revision:7,scope_identity:'${authority}'};else if(operation==='service.resources')result={status:'resources',configuration_error:false,observation:{configuration:{query_concurrency:2}}};else if(operation==='search.call')result=window.nativeResult;return {kind:'ok',status:200,envelope:{schema:'airk-admin/1',ok:true,result}};};Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async text=>{window.copied=text;}}});window.copied=undefined;`);
 const bundle=await build({absWorkingDir:root,stdin:{contents:"import {h,render} from 'preact';import {Search} from './src/SearchScreen';render(h(Search,{}),document.getElementById('root'));",resolveDir:root,sourcefile:'guided-search-browser.tsx'},bundle:true,write:false,format:'iife',platform:'browser',jsx:'automatic',jsxImportSource:'preact',plugins:[{name:'synthetic-i3',setup(plugin){plugin.onResolve({filter:/^\.\/api$/},()=>({path:'api',namespace:'search-fixture'}));plugin.onLoad({filter:/.*/,namespace:'search-fixture'},()=>({contents:'export const call=(...args)=>window.testRun(...args);export const isUnknownOutcome=()=>false;export const preparationAdmissionRefused=()=>false;export const preparationOf=()=>null;export const reviewOf=()=>null;'}));plugin.onResolve({filter:/^\.\/session$/},()=>({path:'session',namespace:'search-session'}));plugin.onLoad({filter:/.*/,namespace:'search-session'},()=>({contents:'export const currentSession=()=>window.session;'}));}}]});
 await browser.evaluate(bundle.outputFiles[0].text);
 await browser.observed(`document.body.textContent.includes('Native fixture sources')`);
 await browser.evaluate(`[...document.querySelectorAll('label')].find(node=>node.textContent==='Native fixture sources').querySelector('input').click()`);
 await browser.observed(`![...document.querySelectorAll('button')].find(node=>node.textContent==='Use selected sources').disabled`);
 await browser.evaluate(`[...document.querySelectorAll('button')].find(node=>node.textContent==='Use selected sources').click()`);
 await browser.observed(`document.body.textContent.includes('Applied scope: Native fixture sources')`);
 await browser.evaluate(`(()=>{const select=[...document.querySelectorAll('select')].find(node=>[...node.options].some(option=>option.value==='answer'));select.value='answer';select.dispatchEvent(new Event('change',{bubbles:true}));})()`);
 await browser.observed(`[...document.querySelectorAll('label')].some(node=>node.textContent.startsWith('Question'))`);
 await browser.evaluate(`(()=>{const input=[...document.querySelectorAll('label')].find(node=>node.textContent.startsWith('Question')).querySelector('input');input.value='Where is café defined?';input.dispatchEvent(new Event('input',{bubbles:true}));})()`);
 await browser.observed(`!document.querySelector('form button[type="submit"]').disabled`);
 await browser.evaluate(`document.querySelector('form').dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}))`);
 await browser.observed(`document.body.textContent.includes('Evidence-grounded context')`);
 const successful=nativeCase.output.members?nativeCase.output.members.filter(member=>member.outcome==='complete'):[nativeCase.output];
 assert.equal(await browser.evaluate(`document.querySelectorAll('[id^="answer-management-answer-"]').length`),successful.length);
 assert.equal(await browser.evaluate(`new Set([...document.querySelectorAll('[id^="answer-management-answer-"]')].map(node=>node.id)).size`),successful.length);
 const targets=await browser.evaluate(`(()=>{const buttons=[...document.querySelectorAll('button')].filter(node=>node.textContent.startsWith('Read evidence '));return buttons.map(button=>{button.click();return document.activeElement.id;});})()`);
 assert.equal(new Set(targets).size,successful.length,'same record SHA in distinct native members must retain distinct citation targets');
 assert.ok(await browser.evaluate(`document.body.textContent.includes('Local partition assessment')&&document.body.textContent.includes('Assessment history')&&document.body.textContent.includes('Original evidence records')`));
 await browser.evaluate(`[...document.querySelectorAll('button')].find(node=>node.textContent==='Copy complete JSON').click()`);
 await browser.observed('window.copied!==undefined');assert.deepEqual(JSON.parse(await browser.evaluate('window.copied')),raw);
 await browser.evaluate(`[...document.querySelectorAll('button')].find(node=>node.textContent==='Copy complete Markdown').click()`);
 await browser.observed(`window.copied.startsWith('# AIRK indexed evidence')`);assert.ok((await browser.evaluate('window.copied')).includes(JSON.stringify(raw,null,2)));
 const request=await browser.evaluate(`window.calls.find(call=>call.operation==='search.call').request.tools_call.arguments`);
 assert.equal(request.schema,'airk.arbiter.context.request/2');assert.equal(request.question,'Where is café defined?');
});
