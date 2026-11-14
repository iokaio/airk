// SPDX-License-Identifier: Apache-2.0
// Actual shared editor UI over a synthetic host response. This qualifies DOM
// wiring/admission only, not native model readiness or installed I2 transport.
import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { readFile } from 'node:fs/promises';
import { browserAvailable, browserHarness } from './browserHarness.mjs';
const require = createRequire(new URL('../../vscode/package.json', import.meta.url));
const { build } = require('esbuild');
const { sharedAdminStylePlugin } = require('./scripts/build-explorer.cjs');
const root = fileURLToPath(new URL('../../vscode/', import.meta.url));
const nativeReads = JSON.parse(await readFile(new URL('../../../fixtures/split/ui_pilot/native_reads.json', import.meta.url), 'utf8'));
const usage = nativeReads.cases.find(entry => entry.name === 'usage_unknown_fields');
const usageFailure = nativeReads.cases.find(entry => entry.name === 'usage_native_failure');
let browser;
before(async () => { if (browserAvailable) browser = await browserHarness(); });
after(async () => { await browser?.close(); });
test('real shared editor Service uses native port, explicit Arbiter enrollment/CAS and early401 without browser credentials',
  { skip: browserAvailable ? false : 'Chromium unavailable.' }, async () => {
    const bundle = await build({ absWorkingDir: root, stdin: { resolveDir: root, sourcefile: 'management-observation.tsx', contents: `
      import {h,render} from 'preact';import {SharedManagement} from './src/explorerWebview/sharedManagement';
      const root=document.createElement('div');document.body.append(root);let epoch=9,page='service';
      window.unmountManagement=()=>render(null,root);
      const show=()=>render(h(SharedManagement,{state:{adminManagement:{epoch,available:true}},page,onPageChange:next=>{page=next;show();}}),root);
      window.navigateManagement=next=>{page=next;show();};
      window.replaceManagementEpoch=next=>{epoch=next;window.dispatchEvent(new MessageEvent('message',{data:{v:1,type:'state',state:{adminManagement:{epoch,available:true}}}}));show();};show();
    ` }, bundle: true, write: false, outdir: 'out/unused-dom-check', format: 'iife', platform: 'browser', jsx: 'automatic', jsxImportSource: 'preact',
      alias: { preact: `${root}/node_modules/preact` }, plugins: [sharedAdminStylePlugin()] });
    await browser.evaluate(`window.requests=[];window.browserAuthTouches=0;window.holdArbiter=false;
      window.nativeUsage=${JSON.stringify(usage.i3)};window.nativeUsageFailure=${JSON.stringify(usageFailure.i3)};
      window.fetch=()=>{throw Error('Native management attempted HTTP');};
      Object.defineProperty(window,'sessionStorage',{get(){window.browserAuthTouches++;throw Error('Native management touched browser credentials');},configurable:true});
      history.replaceState=()=>{throw Error('Native embed attempted browser routing');};
      window.arbiterStatus={schema:'airk.arbiter.status/1',availability:'disabled',reason:'Explicitly off',queued:0,running:0,
        configuration:{schema:'airk.arbiter.configuration/1',installation:'a'.repeat(32),catalog:'b'.repeat(32),revision:7,enabled:false,indexes:[],selected_profile:null},
        selected_profile:null,active_profile:null,profiles:['arbiter-small-intern-08b','arbiter-small-strands-2b','arbiter-large-intern-4b','arbiter-large-kev-4b'].map(model=>({model,availability:'unqualified',profile:null,reason:'No qualified native profile',tasks:['airk.evidence_role.v1']})),future_native:{all:['retained']}};
      window.hostReply=(message,data)=>queueMicrotask(()=>window.dispatchEvent(new MessageEvent('message',{data:{v:1,type:'result',requestId:message.requestId,ok:true,data}})));
      window.acquireVsCodeApi=()=>({getState:()=>undefined,setState:()=>{throw Error('Native capability persisted');},postMessage(message){
        window.requests.push(message);
        if(message.type==='admin.session')return window.hostReply(message,{epoch:message.epoch,serviceInstance:'c'.repeat(32),pending:[]});
        if(message.type==='admin.close'){window.hostReply(message,{epoch:message.epoch,closed:true});queueMicrotask(()=>window.replaceManagementEpoch(message.epoch+1));return;}
        if(message.type==='admin.call'){
          if(window.holdArbiter){window.heldMessage=message;return;}
          if(message.operation==='usage.document'){const reply=message.payload.body.usage_document.read.report?window.nativeUsage:window.nativeUsageFailure;
            return window.hostReply(message,{epoch:message.epoch,current:true,reply:{httpStatus:reply.status,body:JSON.stringify(reply.body)}});}
          if(message.operation==='arbiter.configure'){const request=message.payload.body.arbiter.request;window.arbiterStatus.configuration={...window.arbiterStatus.configuration,...request,revision:8};}
          const result={schema:'airk-admin/1',ok:true,operation:message.operation,result:message.operation==='catalog.page'
            ?{status:'catalog_page',page:{namespace:'e'.repeat(32),revision:1,items:[],continuation:null}}
            :{status:'arbiter',observation:window.arbiterStatus}};
          return window.hostReply(message,{epoch:message.epoch,current:true,reply:{httpStatus:200,body:JSON.stringify(result)}});
        }
      }});`);
    const css = bundle.outputFiles.find(file => file.path.endsWith('.css')).text;
    await browser.evaluate(`{const style=document.createElement('style');style.textContent=${JSON.stringify(css)};document.head.append(style);}`);
    await browser.evaluate(bundle.outputFiles.find(file => file.path.endsWith('.js')).text);
    await browser.observed(`document.body.textContent.includes('Optional local Arbiter')`);
    assert.deepEqual(await browser.evaluate('window.requests.map(message=>message.type)'), ['admin.session']);
    assert.equal(await browser.evaluate('document.querySelectorAll("nav").length'), 0, 'native consumer uses the owning Explorer navigation, without a nested browser shell');
    await browser.evaluate(`[...document.querySelectorAll('summary')].find(summary=>summary.textContent==='Optional local Arbiter').click()`);
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Read Arbiter status').click()`);
    await browser.observed(`document.body.textContent.includes('No qualified native profile')`);
    assert.equal(await browser.evaluate(`[...document.querySelectorAll('button')].filter(button=>button.textContent==='Provision verified profile').every(button=>button.disabled)`), true);
    assert.equal(await browser.evaluate(`document.querySelector('input[type=checkbox]').checked`), false);
    await browser.evaluate(`{const enable=document.querySelector('input[type=checkbox]');enable.click();
      const field=[...document.querySelectorAll('label')].find(label=>label.textContent.includes('Immutable index ID')).querySelector('input');field.value='idx_'+'d'.repeat(32);field.dispatchEvent(new Event('input',{bubbles:true}));
      const task=[...document.querySelectorAll('label')].find(label=>label.textContent.includes('airk.evidence_role.v1')).querySelector('input');task.click();}`);
    await browser.observed(`[...document.querySelectorAll('button')].some(button=>button.textContent==='Add explicit index enrollment'&&!button.disabled)`);
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Add explicit index enrollment').click()`);
    await browser.observed(`document.body.textContent.includes('idx_'+'d'.repeat(32))`);
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Save Arbiter configuration').click()`);
    await browser.observed(`window.requests.some(message=>message.operation==='arbiter.configure')`);
    const saved = await browser.evaluate(`window.requests.find(message=>message.operation==='arbiter.configure').payload.body`);
    assert.deepEqual(saved, { arbiter: { request: { operation: 'configure', expected_revision: 7, enabled: true,
      indexes: [{ index_id: 'idx_' + 'd'.repeat(32), tasks: ['airk.evidence_role.v1'] }], selected_profile: null } } });
    assert.equal(await browser.evaluate('window.browserAuthTouches'), 0);
    await browser.observed(`!document.querySelector('button[disabled]')||document.body.textContent.includes('native revision 8')`);
    await browser.evaluate(`window.navigateManagement('index-library')`);
    await browser.observed(`document.querySelector('[aria-label="Index library views"]')`);
    await browser.evaluate(`[...document.querySelectorAll('[aria-label="Index library views"] button')].find(button=>button.textContent==='Create an index').click()`);
    await browser.observed(`[...document.querySelectorAll('h2')].some(heading=>heading.textContent==='Set up an index'&&heading.getClientRects().length)`);
    await browser.evaluate(`{const panel=[...document.querySelectorAll('.panel')].find(panel=>panel.querySelector('h2')?.textContent==='Set up an index');const input=[...panel.querySelectorAll('label')].find(label=>label.textContent.trim()==='Name').querySelector('input');input.value='Retained explicit draft';input.dispatchEvent(new Event('input',{bubbles:true}));}`);
    await browser.evaluate(`[...document.querySelectorAll('[aria-label="Index library views"] button')].find(button=>button.textContent==='Indexes').click()`);
    await browser.observed(`![...document.querySelectorAll('h2')].some(heading=>heading.textContent==='Set up an index'&&heading.getClientRects().length)`);
    await browser.evaluate(`[...document.querySelectorAll('[aria-label="Index library views"] button')].find(button=>button.textContent==='Create an index').click()`);
    assert.equal(await browser.evaluate(`{const panel=[...document.querySelectorAll('.panel')].find(panel=>panel.querySelector('h2')?.textContent==='Set up an index');[...panel.querySelectorAll('label')].find(label=>label.textContent.trim()==='Name').querySelector('input').value}`), 'Retained explicit draft');
    assert.equal(await browser.evaluate(`window.requests.some(message=>message.operation==='catalog.index_setup_draft'||message.operation==='catalog.index_setup_apply')`), false);
    await browser.evaluate(`window.navigateManagement('activity')`);
    await browser.observed(`document.querySelector('[aria-label="Activity views"]')`);
    await browser.evaluate(`[...document.querySelectorAll('[aria-label="Activity views"] button')].find(button=>button.textContent==='Usage').click()`);
    await browser.observed(`document.querySelector('[aria-label="Usage views"]')`);
    await browser.evaluate(`{const details=[...document.querySelectorAll('details')].find(details=>details.querySelector('summary')?.textContent==='Use a project binding instead');details.open=true;
      const input=details.querySelector('input');input.value=${JSON.stringify(usage.request.admin_body.usage_document.project)};input.dispatchEvent(new Event('input',{bubbles:true}));}`);
    await browser.observed(`[...document.querySelectorAll('button')].some(button=>button.textContent==='Show usage'&&!button.disabled)`);
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Show usage').click()`);
    await browser.observed(`document.body.textContent.includes('Complete native Usage document (JSON)')`);
    for (const tab of ['Summary', 'Breakdown', 'Recording']) {
      await browser.evaluate(`[...document.querySelectorAll('[aria-label="Usage views"] button')].find(button=>button.textContent===${JSON.stringify(tab)}).click()`);
      await browser.observed(`[...document.querySelectorAll('[aria-label="Usage views"] button')].some(button=>button.textContent===${JSON.stringify(tab)}&&button.getAttribute('aria-pressed')==='true')`);
      const complete = await browser.evaluate(`JSON.parse([...document.querySelectorAll('details')].find(details=>details.querySelector('summary')?.textContent==='Complete native Usage document (JSON)'&&details.getClientRects().length).querySelector('pre').textContent)`);
      assert.deepEqual(complete, usage.i3.body.result.document);
    }
    await browser.evaluate(`window.navigateManagement('service')`);
    await browser.observed(`document.querySelector('[aria-label="Service settings views"]')`);
    await browser.evaluate(`[...document.querySelectorAll('[aria-label="Service settings views"] button')].find(button=>button.textContent==='TLS and trust').click()`);
    await browser.observed(`[...document.querySelectorAll('h2')].some(heading=>heading.textContent==='HTTPS issuer'&&heading.getClientRects().length)`);
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Close management session').click()`);
    await browser.observed(`document.body.textContent.includes('captured management session is closed')`);
    assert.equal(await browser.evaluate(`window.requests.filter(message=>message.type==='admin.session').length`), 1, 'close never automatically recaptures a replacement');
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Open native management').click()`);
    await browser.observed(`window.requests.filter(message=>message.type==='admin.session').length===2&&document.body.textContent.includes('Optional local Arbiter')`);
    await browser.evaluate(`[...document.querySelectorAll('summary')].find(summary=>summary.textContent==='Optional local Arbiter').click()`);
    await browser.evaluate(`window.holdArbiter=true;[...document.querySelectorAll('button')].find(button=>button.textContent==='Read Arbiter status').click();`);
    await browser.observed('window.heldMessage');
    await browser.evaluate(`window.dispatchEvent(new MessageEvent('message',{data:{v:1,type:'admin.ended',epoch:9,status:401}}))`);
    assert.equal(await browser.evaluate(`document.body.textContent.includes('captured native management session ended')`), false, 'old epoch401 cannot end the explicitly reopened context');
    await browser.evaluate(`window.dispatchEvent(new MessageEvent('message',{data:{v:1,type:'admin.ended',epoch:10,status:401}}))`);
    await browser.observed(`document.body.textContent.includes('captured native management session ended')`);
    await browser.evaluate(`window.hostReply(window.heldMessage,{epoch:10,current:false,reply:{transport:true,httpStatus:401,message:'Original body unavailable'}})`);
    await browser.observed(`document.body.textContent.includes('captured native management session ended')`);
    assert.equal(await browser.evaluate('window.browserAuthTouches'), 0);
    await browser.evaluate('window.unmountManagement()');
  });
