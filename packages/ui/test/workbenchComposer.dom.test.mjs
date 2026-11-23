// SPDX-License-Identifier: Apache-2.0
// Actual Workbench DOM with a synthetic host; no native transport or model qualification.
import assert from 'node:assert/strict';
import test from 'node:test';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
import {readFile,writeFile} from 'node:fs/promises';
import {browserAvailable,browserHarness} from './browserHarness.mjs';
const {build}=createRequire(new URL('../../vscode/package.json',import.meta.url))('esbuild');
const root=fileURLToPath(new URL('../../vscode/',import.meta.url));
// Answer /3 defaults: the Rust-emitted loop cases.
const native=JSON.parse(await readFile(new URL('../../../fixtures/split/arbiter/answer_loop_cases.json',import.meta.url),'utf8'));
async function fixture(t,{width=420,project=true}={}){
 const bundle=await build({absWorkingDir:root,entryPoints:['src/workbenchWebview/main.tsx'],bundle:true,write:false,outdir:'out/unused-composer-dom',format:'iife',platform:'browser',jsx:'automatic',jsxImportSource:'preact'});
 const browser=await browserHarness();t.after(async()=>{try{await browser.close();}finally{t.diagnostic(JSON.stringify({browser_ownership:browser.observation}));}});
 await browser.call('Emulation.setDeviceMetricsOverride',{width,height:720,deviceScaleFactor:1,mobile:false});
 await browser.evaluate(`document.body.replaceChildren();{const root=document.createElement('div');root.id='root';document.body.append(root);const style=document.createElement('style');style.textContent=${JSON.stringify(bundle.outputFiles.find(file=>file.path.endsWith('.css')).text)};document.head.append(style);}window.requests=[];window.acquireVsCodeApi=()=>({postMessage:message=>window.requests.push(message)});`);
 await browser.evaluate(bundle.outputFiles.find(file=>file.path.endsWith('.js')).text);await browser.observed(`window.requests.some(message=>message.type==='ready')`);
 const id='c'.repeat(64);let state={projects:project?[{id,name:'Synthetic project'}]:[],...(project?{project:id,session:{schema:1,id:'composer-fixture',project:id,title:'Synthetic composer state',createdAt:1,updatedAt:1,revision:1,omitted:0,turns:[],formDraft:{revision:0,activeTool:'search',scope:'',recipe:'',values:{}}}}:{}),sessionEpoch:1,recent:[],cards:{},busy:false,save:'',persistence:false};
 const deliver=async patch=>{state={...state,...patch};await browser.evaluate(`window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:${JSON.stringify(state)}}}))`);};await deliver({});
 const change=async(selector,value)=>browser.evaluate(`{const input=document.querySelector(${JSON.stringify(selector)});input.value=${JSON.stringify(value)};input.dispatchEvent(new Event('input',{bubbles:true}));}`);
 const plus=async()=>{await browser.evaluate(`document.querySelector('button[aria-controls="evidence-options"]').click()`);await browser.observed(`document.querySelector('#evidence-options[role="menu"]')`);};
 const choose=async name=>browser.evaluate(`[...document.querySelectorAll('[role="menuitem"]')].find(button=>button.textContent===${JSON.stringify(name)}).click()`);
 const key=async value=>{const code={Escape:27,Enter:13,ArrowDown:40,ArrowUp:38,Home:36,End:35,Tab:9}[value];await browser.call('Input.dispatchKeyEvent',{type:'keyDown',key:value,code:value,windowsVirtualKeyCode:code,...(value==='Enter'?{text:String.fromCharCode(13),unmodifiedText:String.fromCharCode(13)}:{})});await browser.call('Input.dispatchKeyEvent',{type:'keyUp',key:value,code:value,windowsVirtualKeyCode:code});};
 return {browser,deliver,change,plus,choose,key};
}

test('compact Plus menu owns on-demand context and keyboard navigation, outside dismissal and focus return',{skip:browserAvailable?false:'Chromium unavailable.'},async t=>{
 const {browser:b,change,plus,choose,key}=await fixture(t);await change('textarea[aria-label="Request indexed evidence"]','Current composer question');
 assert.equal(await b.evaluate(`document.querySelector('.composer [aria-label="Context question"]')===null&&document.querySelector('.composer details')===null&&!document.querySelector('.composer').textContent.includes('Cited context uses')`),true);
 assert.equal(await b.evaluate(`document.querySelector('.compose-actions small').textContent`),'Sources read-only');
 for(const width of [1000,320]){await b.call('Emulation.setDeviceMetricsOverride',{width,height:720,deviceScaleFactor:1,mobile:false});const size=await b.evaluate(`({composer:document.querySelector('.composer').getBoundingClientRect().height,transcript:document.querySelector('.transcript').getBoundingClientRect().height})`);assert.ok(size.composer<160&&size.transcript>400,'transcript retains vertical space at '+width);await plus();
  assert.deepEqual(await b.evaluate(`[...document.querySelectorAll('#evidence-options [role="group"]')].map(e=>e.getAttribute('aria-label'))`),['Collect','Request options','Help']);
  assert.deepEqual(await b.evaluate(`[...document.querySelectorAll('#evidence-options [role="menuitem"]')].map(e=>e.textContent)`),['Gather cited context…','Tool forms…','Scope and recipe…','Prepare plan before running','Commands and help']);
  assert.equal(await b.evaluate(`document.querySelector('#evidence-options input, #evidence-options textarea, #evidence-options select')===null`),true,'root menu stays a compact action chooser');
  const rect=await b.evaluate(`JSON.parse(JSON.stringify(document.querySelector('#evidence-options').getBoundingClientRect()))`);assert.ok(rect.height<320&&rect.left>=0&&rect.right<=width&&rect.top>=0);
  if(process.env.AIRK_WORKBENCH_DOM_SCREENSHOTS){const shot=await b.call('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});await writeFile(fileURLToPath(new URL(`composer-plus-${width}.png`,process.env.AIRK_WORKBENCH_DOM_SCREENSHOTS)),Buffer.from(shot.data,'base64'));}
  await key('Escape');await b.observed(`!document.querySelector('#evidence-options')&&document.activeElement?.getAttribute('aria-controls')==='evidence-options'`);
 }
 await plus();await b.observed(`document.activeElement?.textContent==='Gather cited context…'`);await key('ArrowDown');assert.equal(await b.evaluate('document.activeElement.textContent'),'Tool forms…');await key('End');assert.equal(await b.evaluate('document.activeElement.textContent'),'Commands and help');await key('Home');await key('Enter');await b.observed(`document.activeElement?.getAttribute('aria-label')==='Context question'`);
 assert.equal(await b.evaluate(`document.querySelector('[aria-label="Context question"]').value`),'Current composer question');assert.equal(await b.evaluate(`document.querySelector('#evidence-options details').open`),false);assert.equal(await b.evaluate(`(()=>{const panel=document.querySelector('#evidence-options');return panel.scrollWidth<=panel.clientWidth;})()`),true,'on-demand context panel must not scroll horizontally');
 await change('[aria-label="Context question"]','Retained context draft');await key('Escape');await b.observed(`!document.querySelector('#evidence-options')&&document.activeElement?.getAttribute('aria-controls')==='evidence-options'`);
 await plus();await choose('Gather cited context…');await b.observed(`document.querySelector('[aria-label="Context question"]')?.value==='Retained context draft'`);
 const point=await b.evaluate(`(()=>{const node=document.querySelector('.toolbar .project'),r=node.getBoundingClientRect(),point={x:r.x+20,y:r.y+15};if(document.elementFromPoint(point.x,point.y)!==node)throw Error('Outside-click control is covered');return point;})()`);await b.call('Input.dispatchMouseEvent',{type:'mousePressed',...point,button:'left',clickCount:1});await b.call('Input.dispatchMouseEvent',{type:'mouseReleased',...point,button:'left',clickCount:1});await b.observed(`!document.querySelector('#evidence-options')&&document.activeElement?.getAttribute('aria-label')==='Project'`);
 assert.deepEqual(await b.evaluate(`window.requests.filter(e=>!['ready'].includes(e.type))`),[],'opening/dismissing options never dispatches evidence or changes scope');
});

test('tool forms load only on request and scope/recipe choices preserve existing session draft messages',{skip:browserAvailable?false:'Chromium unavailable.'},async t=>{
 const {browser:b,plus,choose,deliver,change}=await fixture(t);await plus();await choose('Tool forms…');await b.observed(`document.querySelector('#evidence-options [role="status"]')?.textContent==='Loading tool forms…'`);assert.deepEqual(await b.evaluate(`window.requests.filter(e=>e.type==='forms')`),[{type:'forms'}]);
 await deliver({operations:{search:{type:'object',required:['query'],properties:{query:{type:'string',minLength:1,maxLength:100}}}}});await b.observed(`document.querySelector('#evidence-options [aria-label="Operation"]')`);
 await b.evaluate(`document.querySelector('[aria-label="Close evidence options"]').click()`);await b.observed(`!document.querySelector('#evidence-options')&&document.activeElement?.getAttribute('aria-controls')==='evidence-options'`);
 assert.equal(await b.evaluate(`document.querySelector('.tool-form')===null`),true);
 await plus();await choose('Scope and recipe…');await b.observed(`document.activeElement?.getAttribute('aria-label')==='Literal collection-relative path prefix'`);await change('[aria-label="Literal collection-relative path prefix"]','src/owned/');
 assert.deepEqual(await b.evaluate(`window.requests.filter(e=>e.type==='formDraft').at(-1)`),{type:'formDraft',sessionId:'composer-fixture',project:'c'.repeat(64),epoch:1,revision:1,tool:'search',scope:'src/owned/'});
 await b.evaluate(`{const select=document.querySelector('[aria-label="Recipe"]');select.value='definition';select.dispatchEvent(new Event('change',{bubbles:true}));}`);
 assert.deepEqual(await b.evaluate(`window.requests.filter(e=>e.type==='formDraft').at(-1)`),{type:'formDraft',sessionId:'composer-fixture',project:'c'.repeat(64),epoch:1,revision:2,tool:'search',recipe:'definition'});
 assert.equal(await b.evaluate(`window.requests.some(e=>e.type==='run'||e.type==='answer')`),false,'settings never execute a read');
});

test('context submission retains native budget validation and rejects empty/oversize/busy requests',{skip:browserAvailable?false:'Chromium unavailable.'},async t=>{
 const {browser:b,plus,choose,change,deliver}=await fixture(t);await change('textarea[aria-label="Request indexed evidence"]','Unrelated request still being edited');await plus();await choose('Gather cited context…');await b.observed(`document.querySelector('[aria-label="Context question"]')?.value==='Unrelated request still being edited'`);await change('[aria-label="Context question"]','');assert.equal(await b.evaluate(`document.querySelector('.context-submit').disabled`),true);
 await change('[aria-label="Context question"]','Question within the native limit');await b.evaluate(`document.querySelector('#evidence-options summary').click()`);assert.deepEqual(JSON.parse(await b.evaluate(`document.querySelector('[aria-label="Native context budget"]').value`)),native.default_budget);
 await change('[aria-label="Native context budget"]','invalid JSON');await b.evaluate(`[...document.querySelectorAll('#evidence-options button')].find(e=>e.textContent==='Gather with this budget').click()`);await b.observed(`document.querySelector('#evidence-options [role="alert"]')`);assert.equal(await b.evaluate(`window.requests.some(e=>e.type==='answer')`),false);
 await change('[aria-label="Native context budget"]',JSON.stringify(native.default_budget));await change('[aria-label="Context question"]','é'.repeat(8193));await b.evaluate(`document.querySelector('.context-submit').click()`);await b.observed(`document.querySelector('#evidence-options [role="alert"]')?.textContent.includes('16,384 UTF-8 bytes')`);assert.equal(await b.evaluate(`window.requests.some(e=>e.type==='answer')`),false);
 await change('[aria-label="Context question"]','Original complete question');await deliver({busy:true});await b.observed(`document.querySelector('.context-submit')?.disabled&&document.querySelector('.context-submit')?.textContent==='Collecting evidence…'`);assert.equal(await b.evaluate(`document.querySelector('[aria-label="Stop collecting evidence"]')!==null`),true);
 await deliver({busy:false});await b.evaluate(`[...document.querySelectorAll('#evidence-options button')].find(e=>e.textContent==='Gather with this budget').click()`);assert.deepEqual(await b.evaluate(`window.requests.filter(e=>e.type==='answer')`),[{type:'answer',question:'Original complete question',budget:native.default_budget}]);await b.observed(`!document.querySelector('#evidence-options')&&document.activeElement?.getAttribute('aria-controls')==='evidence-options'`);assert.equal(await b.evaluate(`document.querySelector('textarea[aria-label="Request indexed evidence"]').value`),'Unrelated request still being edited','a separate context question preserves unrelated composer text');
});

test('without a project the Plus menu disables read actions and keeps help keyboard reachable',{skip:browserAvailable?false:'Chromium unavailable.'},async t=>{
 const {browser:b,plus,key}=await fixture(t,{project:false});await plus();assert.equal(await b.evaluate(`[...document.querySelectorAll('[role="menuitem"]')].filter(e=>e.textContent!=='Commands and help').every(e=>e.disabled)`),true);await b.observed(`document.activeElement?.textContent==='Commands and help'`);await key('ArrowDown');assert.equal(await b.evaluate('document.activeElement.textContent'),'Commands and help');await key('Escape');await b.observed(`document.activeElement?.getAttribute('aria-controls')==='evidence-options'`);assert.equal(await b.evaluate(`window.requests.some(e=>e.type==='answer'||e.type==='run'||e.type==='forms')`),false);
});
