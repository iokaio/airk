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
const bundle=await build({absWorkingDir:root,stdin:{contents:`import {h,render} from 'preact';import {App} from './src/app';import {ManualCard} from './src/agents';import {adoptNativeSession} from './src/session';import {createNativeSessionLifetime} from '../ui/src/nativeSessionLifetime';window.mountShell=()=>render(h(App,{initialGate:{kind:'none'}}),document.getElementById('root'));window.mountManual=()=>render(h(ManualCard,{entries:window.manualEntries,url:'https://127.0.0.1:57421/mcp'}),document.getElementById('root'));window.unmount=()=>render(null,document.getElementById('root'));window.startSession=()=>adoptNativeSession(createNativeSessionLifetime({kind:'native',serviceInstance:'12'.repeat(16),isCurrent:()=>true,call:async(operation,body)=>{window.calls.push({operation,body});const result=operation==='arbiter.status'?{status:'arbiter_device',observation:window.stage}:operation==='service.resources'?{status:'resources',observation:{configuration:{query_concurrency:4},logical_cpus:8,resident_workers:1,resident_limit:4,observed_rss_bytes:2147483648,available_memory_bytes:8589934592,detected_memory_bytes:17179869184,memory_reserved_bytes:3221225472,memory_limit_bytes:6442450944,query_headroom_bytes:1073741824,running_jobs:2,waiting_projects:0,telemetry_complete:true,pressure:false,provider_drain_error:false}}:{};return {kind:'ok',status:200,envelope:{schema:'airk-admin/1',ok:true,result}};},close:async()=>{}}));`,resolveDir:root,sourcefile:'service-setup-dom.tsx'},bundle:true,write:false,format:'iife',platform:'browser',jsx:'automatic',jsxImportSource:'preact'});
const live={skip:browserAvailable?false:'Chromium unavailable.'};
async function reset(){await browser.evaluate(`window.unmount?.();document.body.innerHTML='<style>'+${JSON.stringify(css)}+'</style><div id="root"></div>';window.calls=[];window.stage=${JSON.stringify(stage)};window.manualEntries=[];Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async text=>{window.copied=text;}}});`);await browser.evaluate(bundle.outputFiles[0].text);}
test('actual shell has twelve distinct accessible icons and opens current Service resources and Arbiter without a mutation',live,async()=>{
 await reset();await browser.evaluate('window.startSession();window.mountShell()');
 await browser.observed(`document.querySelectorAll('nav[aria-label="AIRK main navigation"] button').length===12`);
 const labels=await browser.evaluate(`[...document.querySelectorAll('nav[aria-label="AIRK main navigation"] button')].map(button=>button.getAttribute('aria-label'))`);
 assert.deepEqual(labels,['Overview','Search','Indexes','Activity','Agents','Service','Code','Documents','Evidence','Review','Strategies','Help']);
 assert.equal(await browser.evaluate(`new Set([...document.querySelectorAll('nav[aria-label="AIRK main navigation"] svg path')].map(path=>path.getAttribute('d'))).size`),12);
 await browser.evaluate(`document.querySelector('nav button[aria-label="Service"]').click()`);
 await browser.observed(`window.calls.some(row=>row.operation==='service.resources')&&window.calls.some(row=>row.operation==='arbiter.status')&&document.body.textContent.includes('Turned off')`);
 assert.equal(await browser.evaluate(`document.querySelector('main .panel details')?.open`),false,'complete resources stay collapsed while the summary is primary');assert.equal(await browser.evaluate(`document.body.textContent.includes('2.0 GiB')&&document.body.textContent.includes('8 logical CPUs')`),true);
 assert.equal(await browser.evaluate(`window.calls.some(row=>/arbiter\.(?:provision|configure|initialize|remove)/.test(row.operation))`),false);
 for(const width of [1280,800]){await browser.call('Emulation.setDeviceMetricsOverride',{width,height:900,deviceScaleFactor:1,mobile:false});const states=await browser.evaluate(`(()=>{const buttons=[...document.querySelectorAll('nav[aria-label="AIRK main navigation"] button')];return buttons.map(button=>({title:button.title,label:button.getAttribute('aria-label'),display:getComputedStyle(button.querySelector('.nav-label')).display,icon:button.querySelector('svg').getBoundingClientRect().width}));})()`);assert.ok(states.every(row=>row.title===row.label&&row.icon>0));assert.ok(states.every(row=>width===800?row.display==='none':row.display!=='none'));assert.equal(await browser.evaluate(`(()=>{const help=document.querySelector('nav button[aria-label="Help"]'),box=help.getBoundingClientRect();return box.top>=0&&box.bottom<=innerHeight;})()`),true,'Help stays reachable in the viewport with long Service content');if(process.env.AIRK_UI_SCREENSHOT_DIR){const target=resolve(process.env.AIRK_UI_SCREENSHOT_DIR);assert.ok(target.startsWith(resolve(root,'../../artifacts/v190/final-plan')+sep));const capture=await browser.call('Page.captureScreenshot',{format:'png'});await writeFile(join(target,`service-${width}.png`),Buffer.from(capture.data,'base64'),{flag:'wx'});}}
});
test('all native manual variants render and Copy exactly, including changed ports and escaped server names',live,async()=>{
 await reset();for(const entry of manual){await browser.evaluate(`window.manualEntries=[${JSON.stringify(entry)}];window.mountManual()`);await browser.observed(`!!document.getElementById('au-tab-${entry.client}')`);await browser.evaluate(`document.getElementById('au-tab-${entry.client}').click()`);await browser.observed(`document.querySelector('.au-snippet pre')?.textContent===${JSON.stringify(entry.text)}`);await browser.evaluate(`document.querySelector('.au-snippet button').click()`);await browser.observed(`window.copied===${JSON.stringify(entry.text)}`);assert.equal(await browser.evaluate(`document.querySelector('.au-snippet figcaption').textContent.startsWith(${JSON.stringify(entry.file)})`),true);}
});

test('a rejected clipboard write reports failure and preserves the native entry for manual selection',live,async()=>{
 await reset();const entry=manual[0];await browser.evaluate(`window.manualEntries=[${JSON.stringify(entry)}];window.mountManual();navigator.clipboard.writeText=async()=>{throw Error('denied');};`);
 await browser.observed(`!!document.querySelector('.au-snippet pre')`);await browser.evaluate(`document.querySelector('.au-snippet button').click()`);
 await browser.observed(`document.querySelector('.au-copy [role="alert"]')?.textContent.includes('Copy failed')`);
 assert.equal(await browser.evaluate(`document.querySelector('.au-snippet button').textContent.trim()`),'Copy');
 assert.equal(await browser.evaluate(`document.querySelector('.au-snippet pre').textContent`),entry.text);
});
