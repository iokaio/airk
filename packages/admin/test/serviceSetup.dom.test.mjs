// SPDX-License-Identifier: Apache-2.0
// Real rendered components over native structural fixtures and a synthetic session port.
// This proves UI behavior, not model qualification or service transport parity.
import assert from 'node:assert/strict';
import {before,after,test} from 'node:test';
import {build} from 'esbuild';
import {fileURLToPath} from 'node:url';
import {readFile,writeFile} from 'node:fs/promises';
import {join,resolve,sep} from 'node:path';
import {createServer} from 'node:http';
import {once} from 'node:events';
import {browserAvailable,browserHarness} from '../../ui/test/browserHarness.mjs';
const root=fileURLToPath(new URL('../',import.meta.url));
const stage=JSON.parse(await readFile(new URL('../../../fixtures/split/arbiter/device_cases.json',import.meta.url),'utf8')).cases.find(row=>row.id==='both_disabled_auto').status;
const manual=JSON.parse(await readFile(new URL('../../../fixtures/split/i2/agent_manual.json',import.meta.url),'utf8')).entries;
const css=await readFile(new URL('../src/app.css',import.meta.url),'utf8');
let browser,server;
before(async()=>{if(browserAvailable){server=createServer((_,response)=>{response.setHeader('Content-Type','text/html');response.end('<!doctype html><div id="root"></div>');});server.listen(0,'127.0.0.1');await once(server,'listening');browser=await browserHarness();await browser.call('Page.navigate',{url:`http://127.0.0.1:${server.address().port}/`});await browser.observed(`!!document.getElementById('root')`);}});
after(async t=>{try{await browser?.close();}finally{try{if(server)await new Promise((resolve,reject)=>server.close(error=>error?reject(error):resolve()));}finally{t.diagnostic(JSON.stringify({browser_ownership:browser?.observation}));}}});
const bundle=await build({absWorkingDir:root,stdin:{contents:`import {h,render} from 'preact';import {App} from './src/app';import {ManualCard} from './src/agents';import {adoptNativeSession} from './src/session';import {createNativeSessionLifetime} from '../ui/src/nativeSessionLifetime';window.mountShell=()=>render(h(App,{initialGate:{kind:'none'}}),document.getElementById('root'));window.mountManual=()=>render(h(ManualCard,{entries:window.manualEntries,url:'http://127.0.0.1:57421/mcp'}),document.getElementById('root'));window.unmount=()=>render(null,document.getElementById('root'));window.startSession=()=>adoptNativeSession(createNativeSessionLifetime({kind:'native',serviceInstance:'12'.repeat(16),isCurrent:()=>true,call:async(operation,body)=>{window.calls.push({operation,body});if(operation==='service.allow_gpu'){window.allowGpu=body.set_gpu_allowed.allow_gpu;window.gpuOptIn=window.allowGpu?'user_choice':null;}if(operation==='service.configure_resources'){window.embeddingGpu=body.configure_resources.configuration.allow_embedding_gpu;}const result=operation==='arbiter.status'?{status:'arbiter_device',observation:window.stage}:(operation==='service.resources'||operation==='service.allow_gpu'||operation==='service.configure_resources')?{status:'resources',configuration_error:false,observation:{configuration:{query_concurrency:window.resourceConcurrency,preset:'balanced',allow_gpu:window.allowGpu,allow_embedding_gpu:window.embeddingGpu,...(window.gpuOptIn?{gpu_opt_in:window.gpuOptIn}:{})},logical_cpus:8,resident_workers:1,resident_limit:4,observed_rss_bytes:2147483648,available_memory_bytes:8589934592,detected_memory_bytes:17179869184,memory_reserved_bytes:3221225472,memory_limit_bytes:6442450944,query_headroom_bytes:1073741824,running_jobs:2,waiting_projects:0,telemetry_complete:true,pressure:false,provider_drain_error:false}}:{};return {kind:'ok',status:200,envelope:{schema:'airk-admin/1',ok:true,result}};},close:async()=>{}}));`,resolveDir:root,sourcefile:'service-setup-dom.tsx'},bundle:true,write:false,format:'iife',platform:'browser',jsx:'automatic',jsxImportSource:'preact'});
const live={skip:browserAvailable?false:'Chromium unavailable.'};
async function reset(){await browser.evaluate(`window.unmount?.();document.body.innerHTML='<style>'+${JSON.stringify(css)}+'</style><div id="root"></div>';window.calls=[];window.allowGpu=false;window.gpuOptIn=null;window.embeddingGpu=true;window.resourceConcurrency=4;window.stage=${JSON.stringify(stage)};window.manualEntries=[];Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async text=>{window.copied=text;}}});`);await browser.evaluate(bundle.outputFiles[0].text);}
test('actual shell has twelve distinct accessible icons and opens current Service resources and Arbiter without a mutation',live,async()=>{
 await reset();await browser.evaluate('window.startSession();window.mountShell()');
 await browser.observed(`document.querySelectorAll('nav[aria-label="AIRK main navigation"] button').length===12`);
 assert.equal(await browser.evaluate(`document.querySelector('nav').querySelectorAll('h2').length`),3);
 const labels=await browser.evaluate(`[...document.querySelectorAll('nav[aria-label="AIRK main navigation"] button')].map(button=>button.getAttribute('aria-label'))`);
 assert.deepEqual(labels,['Overview','Indexes','Activity','Agents','Service','Search','Code','Documents','Evidence','Review','Strategies','Help']);
 assert.equal(await browser.evaluate(`new Set([...document.querySelectorAll('nav[aria-label="AIRK main navigation"] svg path')].map(path=>path.getAttribute('d'))).size`),12);
 await browser.evaluate(`document.querySelector('nav button[aria-label="Service"]').click()`);
 await browser.observed(`window.calls.some(row=>row.operation==='service.resources')&&window.calls.some(row=>row.operation==='arbiter.status')&&document.body.textContent.includes('Turned off')`);
 await browser.evaluate(`[...document.querySelectorAll('[aria-label="Runtime and models"] button')].find(button=>button.textContent==='Models').click()`);
 await browser.observed(`[...document.querySelectorAll('h2')].some(heading=>heading.textContent==='Optional local models for Answer'&&heading.getClientRects().length>0)`);
 await browser.evaluate(`[...document.querySelectorAll('[aria-label="Runtime and models"] button')].find(button=>button.textContent==='Runtime').click()`);
 assert.equal(await browser.evaluate(`document.querySelector('main .panel details')?.open`),false,'complete resources stay collapsed while the summary is primary');assert.equal(await browser.evaluate(`document.body.textContent.includes('2.0 GiB')&&document.body.textContent.includes('8 logical CPUs')`),true);
 assert.equal(await browser.evaluate(`window.calls.some(row=>/arbiter\.(?:provision|configure|initialize|remove)/.test(row.operation))`),false);
 for(const width of [1280,800]){await browser.call('Emulation.setDeviceMetricsOverride',{width,height:900,deviceScaleFactor:1,mobile:false});const states=await browser.evaluate(`(()=>{const buttons=[...document.querySelectorAll('nav[aria-label="AIRK main navigation"] button')];return buttons.map(button=>({title:button.title,label:button.getAttribute('aria-label'),display:getComputedStyle(button.querySelector('.nav-label')).display,icon:button.querySelector('svg').getBoundingClientRect().width}));})()`);assert.ok(states.every(row=>row.title===row.label&&row.icon>0));assert.ok(states.every(row=>row.display!=='none'));assert.equal(await browser.evaluate(`(()=>{const help=document.querySelector('nav button[aria-label="Help"]'),box=help.getBoundingClientRect();return box.top>=0&&box.bottom<=innerHeight;})()`),true,'Help stays reachable in the viewport with long Service content');if(process.env.AIRK_UI_SCREENSHOT_DIR){const target=resolve(process.env.AIRK_UI_SCREENSHOT_DIR);assert.ok(target.startsWith(resolve(root,'../../artifacts/v190/final-plan')+sep));const capture=await browser.call('Page.captureScreenshot',{format:'png'});await writeFile(join(target,`service-${width}.png`),Buffer.from(capture.data,'base64'),{flag:'wx'});}}
});
test('service GPU permission requires confirmation and changes only the native boolean',live,async()=>{
 await reset();
 await browser.evaluate(`window.startSession();window.mountShell()`);
 await browser.observed(`!!document.querySelector('nav button[aria-label="Service"]')`);
 await browser.evaluate(`document.querySelector('nav button[aria-label="Service"]').click()`);
 const has = label => `[...document.querySelectorAll('button')].some(b=>b.textContent.trim()===${JSON.stringify(label)})`;
 const click = label => browser.evaluate(`[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===${JSON.stringify(label)}).click()`);
 await browser.observed(has('Allow GPU use'));
 await click('Allow GPU use'); await click('Cancel');
 assert.equal(await browser.evaluate(`window.calls.filter(c=>c.operation==='service.allow_gpu').length`),0);
 await click('Allow GPU use'); await click('Confirm change');
 await browser.observed(has('Disable GPU use'));
 assert.deepEqual(await browser.evaluate(`window.calls.filter(c=>c.operation==='service.allow_gpu').map(c=>c.body)`),
                  [{set_gpu_allowed:{allow_gpu:true}}]);
 assert.equal(await browser.evaluate(`window.calls.filter(c=>c.operation==='arbiter.configure').length`),0);
});
// The reviewed I3 operation table says which operations only read. It is not derived from the page under test.
const readOperations=JSON.parse(await readFile(new URL('../../../fixtures/split/i3/admin_operations.json',import.meta.url),'utf8')).operations.filter(row=>row.effect==='read').map(row=>row.operation);
const hasButton=label=>`[...document.querySelectorAll('button')].some(b=>b.textContent.trim()===${JSON.stringify(label)})`;
const clickButton=label=>browser.evaluate(`[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===${JSON.stringify(label)}).click()`);
const gpuCalls=()=>browser.evaluate(`window.calls.filter(c=>c.operation==='service.allow_gpu').map(c=>c.body)`);
const resetNotice=`[...document.querySelectorAll('main p[role="status"]')].some(p=>p.textContent.startsWith('GPU use was reset to off.'))`;
async function openService(record){
 await reset();await browser.evaluate(`window.gpuOptIn=${JSON.stringify(record)};window.startSession();window.mountShell()`);
 await browser.observed(`!!document.querySelector('nav button[aria-label="Service"]')`);
 await browser.evaluate(`document.querySelector('nav button[aria-label="Service"]').click()`);
 await browser.observed(hasButton('Allow GPU use'));
}
test('embedding permission changes independently after refreshing the reviewed configuration',live,async()=>{
 await openService(null);
 await browser.observed(hasButton('Disable embedding GPU use'));
 await clickButton('Disable embedding GPU use');
 await browser.evaluate('window.resourceConcurrency=8');
 await clickButton('Confirm embedding change');
 await browser.observed("document.body.textContent.includes('Resources changed or could not be read')");
 const changes=()=>browser.evaluate("window.calls.filter(c=>c.operation==='service.configure_resources').map(c=>c.body)");
 assert.deepEqual(await changes(),[]);
 await browser.observed(hasButton('Disable embedding GPU use'));
 await clickButton('Disable embedding GPU use');
 await clickButton('Confirm embedding change');
 await browser.observed(hasButton('Allow embedding GPU use'));
 assert.deepEqual(await changes(),[{configure_resources:{configuration:{
   query_concurrency:8,preset:'balanced',allow_gpu:false,allow_embedding_gpu:false,
 }}}]);
 assert.deepEqual(await gpuCalls(),[]);
});

test('a pending GPU reset shows the notice with both answers, and rendering it sends no setting and only reads',live,async()=>{
 await openService(null);
 assert.equal(await browser.evaluate(resetNotice),false,'no record, no notice');
 assert.equal(await browser.evaluate(hasButton('Keep GPU off')),false);
 await openService('default_reset');
 await browser.observed(resetNotice);
 assert.equal(await browser.evaluate(hasButton('Allow GPU use')),true);
 assert.equal(await browser.evaluate(hasButton('Keep GPU off')),true);
 assert.equal(await browser.evaluate(`document.querySelector('main').textContent.includes('Service-wide GPU use: Disabled')`),true);
 assert.deepEqual(await gpuCalls(),[]);
 const called=await browser.evaluate(`window.calls.map(c=>c.operation)`);
 assert.ok(called.includes('service.resources'));
 assert.deepEqual(called.filter(operation=>!readOperations.includes(operation)),[],'rendering the notice calls only read operations');
 assert.equal(await browser.evaluate(`JSON.stringify(window.calls).includes('set_gpu_allowed')`),false);
});
test('Keep GPU off sends exactly allow_gpu false after confirmation and the notice clears',live,async()=>{
 await openService('default_reset');
 await browser.observed(hasButton('Keep GPU off'));
 await clickButton('Keep GPU off');
 await browser.observed(hasButton('Confirm change'));
 assert.equal(await browser.evaluate(`document.querySelector('main').textContent.includes('Keep GPU use off for this service?')`),true);
 await clickButton('Cancel');
 await browser.observed(hasButton('Keep GPU off'));
 assert.deepEqual(await gpuCalls(),[],'opening and cancelling the review sends nothing');
 await clickButton('Keep GPU off');await browser.observed(hasButton('Confirm change'));await clickButton('Confirm change');
 await browser.observed(`!(${resetNotice})&&!(${hasButton('Keep GPU off')})&&${hasButton('Allow GPU use')}`);
 assert.deepEqual(await gpuCalls(),[{set_gpu_allowed:{allow_gpu:false}}]);
 assert.equal(await browser.evaluate(`document.querySelector('main').textContent.includes('Service-wide GPU use: Disabled')`),true);
 assert.equal(await browser.evaluate(`window.calls.some(row=>/arbiter\\.(?:provision|configure|initialize|remove)/.test(row.operation))`),false);
});
test('Allow GPU use from the reset notice sends exactly allow_gpu true after confirmation',live,async()=>{
 await openService('default_reset');
 await browser.observed(resetNotice);
 await clickButton('Allow GPU use');
 await browser.observed(hasButton('Confirm change'));
 assert.equal(await browser.evaluate(`document.querySelector('main').textContent.includes('Allow this service to use an eligible GPU?')`),true);
 assert.deepEqual(await gpuCalls(),[]);
 await clickButton('Confirm change');
 await browser.observed(`!(${resetNotice})&&${hasButton('Disable GPU use')}`);
 assert.deepEqual(await gpuCalls(),[{set_gpu_allowed:{allow_gpu:true}}]);
 assert.equal(await browser.evaluate(hasButton('Keep GPU off')),false);
});
test('all native manual variants render and Copy exactly, including changed ports and escaped server names',live,async()=>{
 await reset();for(const entry of manual){await browser.evaluate(`window.manualEntries=[${JSON.stringify(entry)}];window.mountManual()`);await browser.observed(`!!document.getElementById('au-tab-${entry.client}')`);await browser.evaluate(`document.getElementById('au-tab-${entry.client}').click()`);await browser.observed(`document.querySelector('.au-snippet pre')?.textContent===${JSON.stringify(entry.text)}`);await browser.evaluate(`document.querySelector('.au-snippet button').click()`);await browser.observed(`window.copied===${JSON.stringify(entry.text)}`);assert.equal(await browser.evaluate(`document.querySelector('.au-snippet figcaption').textContent.startsWith(${JSON.stringify(entry.file)})`),true);
  // Local trust plan C: no client tab carries a certificate or trust step.
  const shown=await browser.evaluate(`document.getElementById('root').textContent`);for(const forbidden of ['NODE_EXTRA_CA_CERTS','CODEX_CA_CERTIFICATE','airkctl service trust','CA certificate','https://'])assert.equal(shown.includes(forbidden),false,entry.client+': '+forbidden);}
});

test('a rejected clipboard write reports failure and preserves the native entry for manual selection',live,async()=>{
 await reset();const entry=manual[0];await browser.evaluate(`window.manualEntries=[${JSON.stringify(entry)}];window.mountManual();navigator.clipboard.writeText=async()=>{throw Error('denied');};`);
 await browser.observed(`!!document.querySelector('.au-snippet pre')`);await browser.evaluate(`document.querySelector('.au-snippet button').click()`);
 await browser.observed(`document.querySelector('.au-copy [role="alert"]')?.textContent.includes('Copy failed')`);
 assert.equal(await browser.evaluate(`document.querySelector('.au-snippet button').textContent.trim()`),'Copy');
 assert.equal(await browser.evaluate(`document.querySelector('.au-snippet pre').textContent`),entry.text);
});

test('compact Admin menu opens a guided link and browser Back restores the prior view',live,async()=>{
 await reset();
 await browser.call('Emulation.setDeviceMetricsOverride',{width:390,height:780,deviceScaleFactor:1,mobile:false});
 try {
  await browser.evaluate('window.startSession();window.mountShell()');
  await browser.observed(`document.querySelector('.console-menu')?.getAttribute('aria-expanded')==='false'`);
  assert.equal(await browser.evaluate(`getComputedStyle(document.querySelector('#console-navigation')).display`),'none');
  await browser.evaluate(`document.querySelector('.console-menu').click();document.querySelector('nav button[aria-label="Service"]').click()`);
  await browser.observed(`location.hash==='#service'&&document.querySelector('main h1')?.textContent==='Service'`);
  await browser.evaluate(`document.querySelector('.console-menu').click();document.querySelector('nav button[aria-label="Code"]').click()`);
  await browser.observed(`location.hash==='#code/symbols'&&document.querySelector('main h1')?.textContent==='Code'`);
  assert.equal(await browser.evaluate(`document.activeElement===document.querySelector('main')`),true);
  await browser.evaluate('history.back()');
  await browser.observed(`location.hash==='#service'&&document.querySelector('main h1')?.textContent==='Service'`);
 } finally {
  await browser.call('Emulation.clearDeviceMetricsOverride');
 }
});
