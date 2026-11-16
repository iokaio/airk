// SPDX-License-Identifier: Apache-2.0
// Actual frontend components with a synthetic host; no native or installed parity claim.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile} from 'node:fs/promises';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
import {browserAvailable,browserHarness} from './browserHarness.mjs';
const require=createRequire(new URL('../../vscode/package.json',import.meta.url)),{build}=require('esbuild');
const root=fileURLToPath(new URL('../../../',import.meta.url));
const builder=require(fileURLToPath(new URL('../../vscode/scripts/build-explorer.cjs',import.meta.url)));
const catalog=builder.helpCatalog();

test('editor Evidence tools routes to original guided owner and preserves same-owner form full result and return position only',
 {skip:browserAvailable?false:'Chromium unavailable.'},async t=>{
 let browser;let stage='compile';
 try{
  const result=await build({stdin:{contents:`import {h,render} from 'preact';import {App} from './app';render(h(App,{}),document.getElementById('root'));`,resolveDir:`${root}/packages/vscode/src/explorerWebview`,sourcefile:'hub-test.tsx'},bundle:true,write:false,outdir:'out/unused-hub-test',alias:{preact:`${root}/packages/vscode/node_modules/preact`},format:'iife',platform:'browser',jsx:'automatic',jsxImportSource:'preact',plugins:[builder.sharedAdminStylePlugin(),{name:'bundled-help',setup(plugin){plugin.onResolve({filter:/^airk-explorer-help$/},()=>({path:'catalog',namespace:'catalog'}));plugin.onLoad({filter:/.*/,namespace:'catalog'},()=>({contents:JSON.stringify(catalog),loader:'json'}));}}]});
  browser=await browserHarness();
  const css=await readFile(new URL('../../vscode/src/explorerWebview/styles.css',import.meta.url),'utf8');
  const state={extensionVersion:'1.9.0',trusted:true,nativeExecutionEnabled:true,connection:'ready',projects:[{project:'a'.repeat(64),name:'Synthetic project',root:'/fixture',collection:'fixture'}],selectedProject:'a'.repeat(64),selectionEpoch:1,features:{'code/symbols':'implemented','search/query':'implemented','search/recipes':'implemented','search/tools':'implemented',help:'implemented'}};
  const native={schema:'synthetic-native-response-for-view-test/1',publication:{token:'preserved-publication'},future_field:{all:['retained','é中']},hits:Array.from({length:20},(_,i)=>({id:i,original:{all:true}}))};
  const facts={tool:'code_facts',items:Array.from({length:20},(_,i)=>({path:`api/source-${i}.rs`,startLine:7,endLine:9,kind:'Function',name:`retained-symbol-${i}`,evidenceClass:'E2'})),returned:20,limit:50,truncated:false,publication:'preserved-publication',provenance:[],notes:['Original source read remains bounded.'],nativeResult:native};
  await browser.call('Page.navigate',{url:'about:blank'});
  await browser.evaluate(`document.open();document.write('<!doctype html><html><head></head><body><div id="root"></div></body></html>');document.close();{const style=document.createElement('style');style.textContent=${JSON.stringify(css)};document.head.append(style);}window.requests=[];window.saved={route:'#code/symbols'};window.hostState=${JSON.stringify(state)};window.factReply=${JSON.stringify(facts)};window.acquireVsCodeApi=()=>({getState:()=>window.saved,setState:value=>window.saved=value,postMessage:message=>{window.requests.push(message);if(message.type==='ready'){queueMicrotask(()=>window.dispatchEvent(new MessageEvent('message',{data:{v:1,type:'state',state:window.hostState}})));return;}queueMicrotask(()=>window.dispatchEvent(new MessageEvent('message',{data:{v:1,type:'result',requestId:message.requestId,ok:true,data:message.type==='code.facts'?window.factReply:{}}})));}});`);
  await browser.evaluate(result.outputFiles.find(file=>file.path.endsWith('.js')).text);stage='guided-input';
  await browser.observed(`document.querySelector('input[placeholder^="Exact definition name"]')`);
  await browser.evaluate(`{const input=document.querySelector('input[placeholder^="Exact definition name"]');input.value='original-symbol';input.dispatchEvent(new Event('input',{bubbles:true}));const path=document.querySelector('input[placeholder="e.g. src/"]');path.value='api/';path.dispatchEvent(new Event('input',{bubbles:true}));}`);
  await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Find').click()`);
  await browser.observed(`document.body.textContent.includes('retained-symbol-19')`);
  const originalRaw=await browser.evaluate(`JSON.parse([...document.querySelectorAll('details')].find(detail=>detail.firstElementChild?.textContent==='Complete native response').querySelector('pre').textContent)`);assert.deepEqual(originalRaw,native);
  const scroll=await browser.evaluate(`{const main=document.querySelector('main.content');main.scrollTop=140;document.querySelector('input[placeholder^="Exact definition name"]').focus({preventScroll:true});main.scrollTop;}`);
  stage='hub';await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Back to Evidence tools').click()`);
  await browser.observed(`document.querySelector('[aria-label="Evidence tools hub"]')`);
  assert.equal(await browser.evaluate(`document.querySelector('[data-retained-evidence-view="code/symbols"]').hidden`),true);
  assert.equal(await browser.evaluate(`window.requests.filter(message=>message.type==='code.facts').length`),1);
  assert.equal(await browser.evaluate(`window.saved.lastEvidenceRoute`),'code/symbols');
  stage='continue';await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Continue Code: Symbols').click()`);
  await browser.observed(`!document.querySelector('[data-retained-evidence-view="code/symbols"]').hidden`);
  assert.equal(await browser.evaluate(`document.querySelector('input[placeholder^="Exact definition name"]').value`),'original-symbol');
  assert.equal(await browser.evaluate(`document.querySelector('input[placeholder="e.g. src/"]').value`),'api/');
  assert.equal(await browser.evaluate(`document.querySelector('main.content').scrollTop`),scroll);
  assert.deepEqual(await browser.evaluate(`JSON.parse([...document.querySelectorAll('details')].find(detail=>detail.firstElementChild?.textContent==='Complete native response').querySelector('pre').textContent)`),native);
  assert.equal(await browser.evaluate(`window.requests.filter(message=>message.type==='code.facts').length`),1,'return never replays a native read');
  stage='new-owner';await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Back to Evidence tools').click();window.hostState={...window.hostState,selectionEpoch:2};window.dispatchEvent(new MessageEvent('message',{data:{v:1,type:'state',state:window.hostState}}));`);
  await browser.observed(`!document.querySelector('[data-retained-evidence-view="code/symbols"]')`);
  await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Continue Code: Symbols').click()`);
  await browser.observed(`document.querySelector('input[placeholder^="Exact definition name"]')`);
  assert.equal(await browser.evaluate(`document.querySelector('input[placeholder^="Exact definition name"]').value`),'');
  assert.equal(await browser.evaluate(`document.body.textContent.includes('retained-symbol-19')`),false);
  for(const boundary of ['trust','native-consent']){
    stage=`${boundary}-custody`;
    await browser.evaluate(`{const input=document.querySelector('input[placeholder^="Exact definition name"]');input.value='not-for-replacement-owner';input.dispatchEvent(new Event('input',{bubbles:true}));}`);
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Back to Evidence tools').click()`);
    await browser.observed(`document.querySelector('[aria-label="Evidence tools hub"]')`);
    await browser.evaluate(`window.hostState={...window.hostState,...${JSON.stringify(boundary==='trust'?{trusted:false}:{nativeExecutionEnabled:false})}};window.dispatchEvent(new MessageEvent('message',{data:{v:1,type:'state',state:window.hostState}}));`);
    await browser.observed(`!document.querySelector('[data-retained-evidence-view="code/symbols"]')`);
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Continue Code: Symbols').click()`);
    await browser.observed(`document.querySelector('input[placeholder^="Exact definition name"]')`);
    assert.equal(await browser.evaluate(`document.querySelector('input[placeholder^="Exact definition name"]').value`),'',`${boundary} discards the previous owner form`);
  }

 }catch(error){t.diagnostic(JSON.stringify({stage,browser_ownership:browser?.observation}));throw error;}finally{try{await browser?.close();}finally{t.diagnostic(JSON.stringify({browser_ownership:browser?.observation}));}}
});

test('browser Evidence tools shows its supported guided routes and retains its actual query owner while toggling views',
 {skip:browserAvailable?false:'Chromium unavailable.'},async t=>{
 const browser=await browserHarness();try{
  const result=await build({stdin:{contents:`import {h,render} from 'preact';import {useState} from 'preact/hooks';import {SearchWorkspace} from './app';function Host(){const [tools,setTools]=useState(true);return h(SearchWorkspace,{route:{page:'search',params:new URLSearchParams(tools?{view:'tools'}:{}),seq:1},onView:setTools});}render(h(Host,{}),document.body);`,resolveDir:`${root}/packages/admin/src`,sourcefile:'browser-hub-test.tsx'},bundle:true,write:false,format:'iife',platform:'browser',jsx:'automatic',jsxImportSource:'preact'});
  await browser.evaluate(`window.nativeFetches=0;window.fetch=()=>{window.nativeFetches++;throw Error('No authenticated native session in this DOM test');};`);await browser.evaluate(result.outputFiles[0].text);
  await browser.observed(`document.querySelector('[aria-label="Evidence tools hub"]')`);
  assert.equal(await browser.evaluate('window.nativeFetches'),0,'hub first entry never mounts or runs the query owner');
  assert.equal(await browser.evaluate(`document.body.textContent.includes('Browser workflow migration is not complete')`),false,'completed guided routes are no longer described as unavailable');
  assert.deepEqual(await browser.evaluate(`Object.fromEntries([...document.querySelectorAll('[aria-label="Evidence tools hub"] section section')].map(section=>[section.querySelector('h3').textContent,[...section.querySelectorAll('button')].map(button=>button.textContent)]))`),{Code:['Code: Symbols','Code: Textual references','Code: Selected-source outline'],Documents:['Documents: Document facts'],Evidence:['Evidence: Inspector','Evidence: Check citations','Evidence: Indexed changes'],Review:['Review: Git changes','Review: Impact candidates','Review: Test candidates'],Strategies:['Strategies: Library','Strategies: Studio','Strategies: Planner']});
  assert.equal(await browser.evaluate(`document.querySelectorAll('[aria-label="Evidence tools hub"] a[href^="#help?guide="]').length`),5);
  await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Query').click()`);
  await browser.observed(`document.querySelector('input[placeholder="Where is request cancellation handled?"]')`);
  await browser.evaluate(`{const input=document.querySelector('input[placeholder="Where is request cancellation handled?"]');input.value='retained browser question';input.dispatchEvent(new Event('input',{bubbles:true}));}`);
  await browser.evaluate(`[...document.querySelectorAll('[aria-label="Search views"] button')].find(button=>button.textContent==='Evidence tools').click()`);
  await browser.observed(`document.querySelector('[aria-label="Evidence tools hub"]')`);
  await browser.evaluate(`[...document.querySelectorAll('[aria-label="Search views"] button')].find(button=>button.textContent==='Query').click()`);
  assert.equal(await browser.evaluate(`document.querySelector('input[placeholder="Where is request cancellation handled?"]').value`),'retained browser question');
 }finally{try{await browser?.close();}finally{t.diagnostic(JSON.stringify({browser_ownership:browser?.observation}));}}
});
