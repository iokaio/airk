// SPDX-License-Identifier: Apache-2.0
// Actual component callbacks over synthetic scope replies; no native transport claim.
import assert from 'node:assert/strict';
import {before,after,test} from 'node:test';
import {build} from 'esbuild';
import {fileURLToPath} from 'node:url';
import {browserAvailable,browserHarness} from '../../ui/test/browserHarness.mjs';
const root=fileURLToPath(new URL('../',import.meta.url));let browser;
before(async()=>{if(browserAvailable)browser=await browserHarness();});
after(async t=>{try{await browser?.close();}finally{t.diagnostic(JSON.stringify({browser_ownership:browser?.observation}));}});
const bundle=await build({absWorkingDir:root,stdin:{contents:`import {h,render} from 'preact';import {BrowserStrategiesScreen} from './src/StrategiesScreen';import {adoptNativeSession,currentSession} from './src/session';import {createNativeSessionLifetime} from '../ui/src/nativeSessionLifetime';import {strategyWorkspaceFor} from './src/strategyWorkspace';import {CORE_PACK} from '../ui/src/strategies';
window.newSession=()=>{const port={kind:'native',serviceInstance:'12'.repeat(16),isCurrent:()=>true,call:async()=>({kind:'no_session'}),close:async()=>{}};adoptNativeSession(createNativeSessionLifetime(port));};window.owner=()=>strategyWorkspaceFor(currentSession());window.templates=CORE_PACK;window.unmount=()=>render(null,document.getElementById('root'));window.mount=()=>render(h(BrowserStrategiesScreen,{route:window.route,scopeIdentity:window.scope,active:window.active,disabled:false,capture:()=>window.capture,onNavigate:route=>{window.route=route;window.mount();}}),document.getElementById('root'));`,resolveDir:root,sourcefile:'strategies-dom.tsx'},bundle:true,write:false,format:'iife',platform:'browser',jsx:'automatic',jsxImportSource:'preact'});
const live={skip:browserAvailable?false:'Chromium unavailable.'};
async function reset(route='strategies/library') {
 await browser.evaluate(`document.body.innerHTML='<div id="root"></div>';window.route=${JSON.stringify(route)};window.scope='34'.repeat(32);window.active=true;window.calls=[];window.copied=undefined;Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async text=>{window.copied=text;}}});`);
 console.info('Strategies DOM stage: bootstrap',bundle.outputFiles[0].contents.length);
 await browser.evaluate(bundle.outputFiles[0].text);console.info('Strategies DOM stage: session');await browser.evaluate('window.newSession()');console.info('Strategies DOM stage: mount');await browser.evaluate('window.mount()');await browser.observed(`!!document.querySelector('[aria-label="Strategies workspace"]')`);
}
const click=text=>browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent===${JSON.stringify(text)}).click()`);
const input=(label,value)=>browser.evaluate(`(()=>{const area=[...document.querySelectorAll('label')].find(label=>label.textContent.startsWith(${JSON.stringify(label)})).querySelector('input,textarea,select');area.value=${JSON.stringify(value)};area.dispatchEvent(new Event(area.tagName==='SELECT'?'change':'input',{bubbles:true}));})()`);

test('Library filters all25 native-independent templates and accepted choice opens the actual Studio',live,async()=>{
 await reset();assert.equal(await browser.evaluate('document.querySelectorAll("article.card").length'),25);
 await input('Category','Testing');await browser.observed(`document.querySelectorAll('article.card').length>0&&document.querySelectorAll('article.card').length<25`);
 const titles=await browser.evaluate(`[...document.querySelectorAll('article.card h3')].map(el=>el.textContent)`);assert.deepEqual(titles,await browser.evaluate(`window.templates.filter(entry=>entry.strategy.category==='Testing').map(entry=>entry.strategy.title)`));
 await click('Load in Studio');await browser.observed(`document.body.textContent.includes('Strategy Studio')`);assert.equal(await browser.evaluate('window.route'),'strategies/studio');
 await input('Objective','Keep my objective');await browser.evaluate(`window.route='strategies/library';window.mount()`);await browser.observed(`document.querySelectorAll('article.card').length>0`);
 await click('Load in Studio');await browser.observed(`!!document.querySelector('[data-strategy-confirmation]')`);await click('Keep current draft');assert.equal(await browser.evaluate('window.owner().view().studio.objective'),'Keep my objective');
 await click('Load in Studio');await browser.observed(`!!document.querySelector('[data-strategy-confirmation]')`);await click('Replace draft');await browser.observed(`document.body.textContent.includes('Strategy Studio')`);assert.equal(await browser.evaluate('window.owner().view().studio.objective'),'Keep my objective');
});

test('Studio prompt edits survive actual unmount/remount within exact session, new session with same instance clears them',live,async()=>{
 await input('Compiled prompt','Hand edited original prompt');await browser.evaluate(`window.active=false;window.mount()`);await browser.observed(`window.owner().view().studio.prompt==='Hand edited original prompt'`);
 // Actually destroy the component tree, then remount using the retained exact session owner.
 await browser.evaluate(`window.unmount();window.route='strategies/planner';window.active=true;window.mount();`);await browser.observed(`document.body.textContent.includes('Planner')`);await browser.evaluate(`window.route='strategies/studio';window.mount()`);await browser.observed(`document.querySelector('textarea')&&[...document.querySelectorAll('textarea')].some(area=>area.value==='Hand edited original prompt')`);
 await browser.evaluate(`window.newSession();window.mount()`);await browser.observed(`document.body.textContent.includes('Choose a Library template')`);assert.equal(await browser.evaluate('window.owner().view().studio.prompt'),undefined);
});

test('overwrite confirmation checks current draft revision and never discards newer edits',live,async()=>{
 await reset();await click('Load in Studio');await browser.observed(`document.body.textContent.includes('Strategy Studio')`);await input('Objective','Original');await browser.evaluate(`window.route='strategies/library';window.mount()`);await browser.observed(`!!document.querySelector('article.card')`);await click('Load in Studio');await browser.observed(`!!document.querySelector('[data-strategy-confirmation]')`);
 await browser.evaluate(`window.owner().editStudio(draft=>({...draft,objective:'Newer independent edit'}))`);await click('Replace draft');await browser.observed(`document.querySelector('[role="alert"]')?.textContent.includes('changed')`);assert.equal(await browser.evaluate('window.owner().view().studio.objective'),'Newer independent edit');
});

test('Planner uses real derived dependency state, completion confirmation and inert clipboard failure recovery',live,async()=>{
 await reset('strategies/planner');await click('New plan');await browser.observed(`document.body.textContent.includes('Add manual task')`);await click('Add manual task');await browser.observed(`document.querySelectorAll('article.card').length===1`);await click('Add manual task');await browser.observed(`document.querySelectorAll('article.card').length===2`);
 await browser.evaluate(`const cards=document.querySelectorAll('article.card'),deps=[...cards[1].querySelectorAll('label')].find(el=>el.textContent.startsWith('Dependencies')).querySelector('input');deps.value='T1';deps.dispatchEvent(new Event('change',{bubbles:true}));`);await browser.observed(`document.querySelectorAll('article.card h3')[1].textContent.includes('blocked')`);
 await browser.evaluate(`const state=document.querySelector('article.card select');state.value='completed';state.dispatchEvent(new Event('change',{bubbles:true}));`);await browser.observed(`!!document.querySelector('[aria-label="Confirm task completion"]')`);assert.equal(await browser.evaluate('window.owner().view().planner.plan.tasks[0].state'),'draft');await click('Mark completed');await browser.observed(`!document.querySelectorAll('article.card h3')[1].textContent.includes('blocked')`);
 await browser.evaluate(`Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async()=>{throw Error('browser_denied');}}})`);await click('Copy task instructions');await browser.observed(`document.querySelector('[role="alert"]')?.textContent.includes('browser_denied')`);assert.equal(await browser.evaluate('window.owner().view().planner.plan.tasks.length'),2);
});

test('hidden view releases confirmation without overwriting retained drafts',live,async()=>{
 await click('New plan');await browser.observed(`!!document.querySelector('[data-strategy-confirmation]')`);await browser.evaluate(`window.active=false;window.mount()`);await browser.observed(`!document.querySelector('[data-strategy-confirmation]')`);assert.equal(await browser.evaluate('window.owner().view().planner.plan.tasks.length'),2);await browser.evaluate(`window.active=true;window.mount()`);assert.equal(await browser.evaluate('window.owner().view().planner.plan.tasks.length'),2);
});

test('task completion confirmation refuses a changed plan with the same task identity',live,async()=>{
 await browser.evaluate(`(()=>{const state=document.querySelectorAll('article.card select')[1];state.value='completed';state.dispatchEvent(new Event('change',{bubbles:true}));})()`);await browser.observed(`!!document.querySelector('[aria-label="Confirm task completion"]')`);
 await browser.evaluate(`window.owner().editPlan(plan=>({...plan,tasks:plan.tasks.map(task=>task.id==='T2'?{...task,instructions:'New unreviewed instructions'}:task)}))`);
 await click('Mark completed');await browser.observed(`document.querySelector('[role="alert"]')?.textContent.includes('Planner changed')`);assert.equal(await browser.evaluate('window.owner().view().planner.plan.tasks[1].state'),'draft');assert.equal(await browser.evaluate('window.owner().view().planner.plan.tasks[1].instructions'),'New unreviewed instructions');
});

for(const transition of ['hidden','another strategy route','new session'])test(`actual pending run callback cannot republish after ${transition}`,live,async()=>{
 await reset('strategies/studio');
 await browser.evaluate(`window.owner().editStudio(()=>({strategy:window.templates.find(entry=>entry.runnable).strategy,params:{query:'needle',symbol:'native'},pathPrefix:'',objective:'Original pending evidence',profile:'portable',promptEdited:false}));window.waiting=false;window.settled=false;window.capture={indexIds:['idx_'+'23'.repeat(16)],backingBinding:'45'.repeat(32),checkCurrent(){},async read(tool,args){window.calls.push({tool,args});window.waiting=true;await new Promise(resolve=>{window.release=resolve});window.settled=true;return {scopeIdentity:window.scope,request:{tool,arguments:args},logical:{original:'late reply'},raw:{original_native:{kept:true}}};}};`);
 await browser.observed(`!![...document.querySelectorAll('button')].find(button=>button.textContent==='Assemble evidence')`);await click('Assemble evidence');await browser.observed('window.waiting');
 if(transition==='hidden')await browser.evaluate(`window.active=false;window.mount()`);
 else if(transition==='another strategy route')await browser.evaluate(`window.route='strategies/planner';window.mount()`);
 else await browser.evaluate(`window.newSession();window.mount()`);
 await browser.evaluate(`window.release();new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))`);await browser.observed('window.settled');
 assert.equal(await browser.evaluate('window.calls.length'),1);assert.equal(await browser.evaluate('window.owner().view().proof'),undefined);assert.equal(await browser.evaluate(`document.body.textContent.includes('complete native read receipt(s) retained')`),false);
 if(transition!=='new session')assert.equal(await browser.evaluate('window.owner().view().studio.objective'),'Original pending evidence');else assert.equal(await browser.evaluate('window.owner().view().studio.strategy'),undefined);
});
