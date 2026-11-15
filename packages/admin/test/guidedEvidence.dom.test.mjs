// SPDX-License-Identifier: Apache-2.0
// Actual guided components/browser events over explicitly synthetic capture replies. Not native transport parity.
import assert from 'node:assert/strict';
import {before,after,test} from 'node:test';
import {build} from 'esbuild';
import {fileURLToPath} from 'node:url';
import {browserAvailable,browserHarness} from '../../ui/test/browserHarness.mjs';
const root=fileURLToPath(new URL('../',import.meta.url));let browser;
before(async()=>{if(browserAvailable)browser=await browserHarness();});
after(async t=>{try{await browser?.close();}finally{t.diagnostic(JSON.stringify({browser_ownership:browser?.observation}));}});
const routes=[['code/symbols','code_facts','Identifier or symbol name','refreshSession'],['code/references','references','Identifier or symbol name','verifyToken'],['code/outline','outline','Indexed relative source path','src/session.rs'],['docs/facts','document_facts','Text contains','cancellation'],['evidence/inspector','file_info','Indexed relative source path','src/session.rs'],['evidence/changes','changes','Earlier native publication token','original-native-token'],['evidence/checker','file_info','Report or citations','src/session.rs:3']];
const bundle=await build({absWorkingDir:root,stdin:{contents:"import {h,render} from 'preact';import {EvidenceToolsScreen} from './src/EvidenceToolsScreen';window.mountGuided=()=>render(h(EvidenceToolsScreen,{route:window.route,scopeIdentity:window.scope??'56'.repeat(32),disabled:false,capture:()=>window.capture}),document.getElementById('root'));window.mountGuided();",resolveDir:root,sourcefile:'guided-tools-dom.tsx'},bundle:true,write:false,format:'iife',platform:'browser',jsx:'automatic',jsxImportSource:'preact',metafile:true});
assert.equal(Object.keys(bundle.metafile.inputs).some(path=>path.includes('mcpStdioSession')||path.includes('airkctl/runner')||path.endsWith('/explorerEvidence.ts')),false,'browser graph must never gain native process transport');
for(const [route,tool,label,value]of routes)test(`guided browser workflow ${route} submits native arguments and preserves complete observations`,{skip:browserAvailable?false:'Chromium unavailable.'},async()=>{
 await browser.evaluate(`document.body.innerHTML='<div id="root"></div>';window.route=${JSON.stringify(route)};window.calls=[];window.current=true;window.capture={indexIds:['idx_'+'23'.repeat(16)],checkCurrent(){if(!window.current)throw Error('scope_changed');},async read(tool,args){this.checkCurrent();window.calls.push({tool,args});const info={content_hash:'78'.repeat(32),indexed_lines:4,source:{root:'synthetic',relative_path:'src/session.rs'},future_native:{retained:['all','fields']}};const logical=${JSON.stringify(route)}==='evidence/checker'?{members:[{index_id:this.indexIds[0],result:{structuredContent:info}}]}:{files:[{path:'src/session.rs',content_hash:'78'.repeat(32)}],items:[{file:0,kind:'definition',lines:[3,4],name:'refreshSession',signature:'fn refreshSession()'}],future_native:info.future_native};return {scopeIdentity:'56'.repeat(32),request:{tool,arguments:args},raw:{complete_original:logical,unknown_outer:{kept:true}},logical};}};Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async text=>window.copied=text}});`);
 await browser.evaluate(bundle.outputFiles[0].text);
 await browser.observed(`!!document.querySelector('form')`);
 await browser.evaluate(`(()=>{const label=[...document.querySelectorAll('label')].find(el=>el.textContent.startsWith(${JSON.stringify(label)})),input=label.querySelector('input,textarea');input.value=${JSON.stringify(value)};input.dispatchEvent(new Event('input',{bubbles:true}));})()`);
 await browser.evaluate(`document.querySelector('form').dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}))`);
 await browser.observed(`window.calls.length===1&&document.body.textContent.includes('Complete native read 1')`);
 const read=await browser.evaluate('window.calls[0]');assert.equal(read.tool,tool);if(route!=='evidence/checker')assert.equal(read.args.name??read.args.path??read.args.since,value);else assert.equal(read.args.path,'src/session.rs');
 await browser.evaluate(`[...document.querySelectorAll('button')].find(el=>el.textContent==='Copy complete guided run JSON').click()`);
 await browser.observed(`!!window.copied`);const copied=JSON.parse(await browser.evaluate('window.copied'));assert.deepEqual(copied.reads[0].raw.unknown_outer,{kept:true});assert.equal(copied.reads[0].request.tool,tool);
 assert.equal(await browser.evaluate(`document.body.textContent.includes('matches_index')`),false,'missing saved-source verification must never be presented as matched');
});

test('clipboard denial is an explicit recoverable error and retains complete evidence', {skip:browserAvailable?false:'Chromium unavailable.'},async()=>{
 await browser.evaluate(`Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async()=>{throw Error('denied_by_browser');}}});[...document.querySelectorAll('button')].find(el=>el.textContent==='Copy complete guided run JSON').click()`);
 await browser.observed(`document.querySelector('[role="alert"]')?.textContent.includes('Clipboard write failed')`);
 assert.equal(await browser.evaluate(`document.body.textContent.includes('Complete native read 1')`),true);
});
test('citation check visibly discloses its25distinct-path bound and preserves every attempted raw reply', {skip:browserAvailable?false:'Chromium unavailable.'},async()=>{
 await browser.evaluate(`window.calls=[];const area=document.querySelector('form textarea');area.value=['Original report narrative retained in full.',...Array.from({length:26},(_,i)=>'src/file-'+i+'.rs:3')].join(String.fromCharCode(10));area.dispatchEvent(new Event('input',{bubbles:true}));Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async text=>window.copied=text}});window.copied=undefined;`);
 await browser.evaluate(`document.querySelector('form').dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}))`);
 await browser.observed(`document.body.textContent.includes('Only25 of 26 distinct relative paths were checked')`);
 assert.equal(await browser.evaluate('window.calls.length'),25);await browser.evaluate(`[...document.querySelectorAll('button')].find(el=>el.textContent==='Copy complete guided run JSON').click()`);await browser.observed(`!!window.copied`);
 const copy=JSON.parse(await browser.evaluate('window.copied'));assert.equal(copy.reads.length,25);assert.equal(copy.checks.length,26);assert.equal(copy.inputs.fields.text,['Original report narrative retained in full.',...Array.from({length:26},(_,i)=>'src/file-'+i+'.rs:3')].join(String.fromCharCode(10)));assert.equal(copy.inputs.route,'evidence/checker');assert.equal(copy.checks.at(-1).state,'unchecked');assert.deepEqual(copy.reads.at(-1).raw.unknown_outer,{kept:true});
});

test('source inspection retains the original guided facts and both complete raw calls; changed scope disables historical source actions',{skip:browserAvailable?false:'Chromium unavailable.'},async()=>{
 await browser.evaluate(`document.body.innerHTML='<div id="root"></div>';window.route='code/symbols';window.scope='56'.repeat(32);window.calls=[];window.copied=undefined;window.capture={indexIds:['idx_'+'23'.repeat(16)],checkCurrent(){},async read(tool,args){window.calls.push({tool,args});const logical=tool==='file_info'?{path:args.path,indexed_lines:4,content_hash:'78'.repeat(32),future_file:{all:true}}:{files:[{path:'src/session.rs',content_hash:'78'.repeat(32)}],items:[{file:0,kind:'definition',lines:[3,4],name:'refreshSession',signature:'fn refreshSession()'}]};return {scopeIdentity:'56'.repeat(32),request:{tool,arguments:args},raw:{original:logical,unknown_outer:{all:['preserved']}},logical};}};`);
 await browser.evaluate(bundle.outputFiles[0].text);await browser.observed(`!!document.querySelector('form')`);
 await browser.evaluate(`const input=document.querySelector('form input');input.value='refreshSession';input.dispatchEvent(new Event('input',{bubbles:true}));`);
 await browser.evaluate(`document.querySelector('form').dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}))`);
 await browser.observed(`[...document.querySelectorAll('button')].some(button=>button.textContent==='Inspect indexed source')`);
 await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Inspect indexed source').click()`);
 await browser.observed(`window.calls.length===2&&document.body.textContent.includes('Complete native read 2')`);
 assert.equal(await browser.evaluate(`document.querySelector('[aria-label="Fact result 1"]')?.textContent.includes('refreshSession')`),true);
 await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Copy complete guided run JSON').click()`);await browser.observed(`!!window.copied`);
 const copy=JSON.parse(await browser.evaluate('window.copied'));assert.equal(copy.reads.length,2);assert.deepEqual(copy.reads.map(read=>read.request.tool),['code_facts','file_info']);assert.deepEqual(copy.reads[0].raw.unknown_outer,{all:['preserved']});assert.equal(copy.facts.items[0].name,'refreshSession');assert.equal(copy.inputs.fields.name,'refreshSession');await browser.evaluate(`(()=>{const input=document.querySelector('form input');input.value='changed-after-the-read';input.dispatchEvent(new Event('input',{bubbles:true}));[...document.querySelectorAll('button')].find(button=>button.textContent==='Copy complete guided run JSON').click();})()`);await browser.observed(`JSON.parse(window.copied).inputs.fields.name==='refreshSession'`);
 await browser.evaluate(`window.scope='99'.repeat(32);window.mountGuided()`);await browser.observed(`document.body.textContent.includes('Historical result from another scope')`);
 assert.equal(await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Inspect indexed source').disabled`),true);assert.equal(await browser.evaluate('window.calls.length'),2);
});
