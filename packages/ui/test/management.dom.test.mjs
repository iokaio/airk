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
// One explicit post-emission extension exercises forward-compatible retention.
const stageStatus = {...JSON.parse(await readFile(new URL('../../../fixtures/split/arbiter/device_cases.json', import.meta.url), 'utf8')).cases.find(entry => entry.id === 'both_disabled_auto').status,future_native:{all:['retained']}};
const usage = nativeReads.cases.find(entry => entry.name === 'usage_unknown_fields');
const usageFailure = nativeReads.cases.find(entry => entry.name === 'usage_native_failure');
let browser;
before(async () => { if (browserAvailable) browser = await browserHarness(); });
after(async () => { await browser?.close(); });
test('real shared editor Service uses native port, explicit Arbiter enrollment/CAS and early401 without browser credentials',
  { skip: browserAvailable ? false : 'Chromium unavailable.' }, async () => {
    const bundle = await build({ absWorkingDir: root, stdin: { resolveDir: root, sourcefile: 'management-observation.tsx', contents: `
      import {h,render} from 'preact';import {SharedManagement} from './src/explorerWebview/sharedManagement';
      const root=document.createElement('div');document.body.append(root);let epoch=9,page='service',target;
      window.unmountManagement=()=>render(null,root);
      const show=()=>render(h(SharedManagement,{state:{adminManagement:{epoch,available:true}},page,target,onPageChange:next=>{page=next;show();}}),root);
      window.navigateManagement=(next,prefill)=>{page=next;target=prefill;show();};
      window.replaceManagementEpoch=next=>{epoch=next;window.dispatchEvent(new MessageEvent('message',{data:{v:1,type:'state',state:{adminManagement:{epoch,available:true}}}}));show();};show();
    ` }, bundle: true, write: false, outdir: 'out/unused-dom-check', format: 'iife', platform: 'browser', jsx: 'automatic', jsxImportSource: 'preact',
      alias: { preact: `${root}/node_modules/preact` }, plugins: [sharedAdminStylePlugin()] });
    await browser.evaluate(`window.requests=[];window.browserAuthTouches=0;window.holdArbiter=false;
      window.nativeUsage=${JSON.stringify(usage.i3)};window.nativeUsageFailure=${JSON.stringify(usageFailure.i3)};
      window.fetch=()=>{throw Error('Native management attempted HTTP');};
      Object.defineProperty(window,'sessionStorage',{get(){window.browserAuthTouches++;throw Error('Native management touched browser credentials');},configurable:true});
      history.replaceState=()=>{throw Error('Native embed attempted browser routing');};
      window.arbiterStatus=${JSON.stringify(stageStatus)};
      window.hostReply=(message,data)=>queueMicrotask(()=>window.dispatchEvent(new MessageEvent('message',{data:{v:1,type:'result',requestId:message.requestId,ok:true,data}})));
      window.acquireVsCodeApi=()=>({getState:()=>undefined,setState:()=>{throw Error('Native capability persisted');},postMessage(message){
        window.requests.push(message);
        if(message.type==='admin.session')return window.hostReply(message,{epoch:message.epoch,serviceInstance:'c'.repeat(32),pending:[]});
        if(message.type==='admin.close'){window.hostReply(message,{epoch:message.epoch,closed:true});queueMicrotask(()=>window.replaceManagementEpoch(message.epoch+1));return;}
        if(message.type==='admin.call'){
          if(window.holdArbiter){window.heldMessage=message;return;}
          if(message.operation==='logs.recent'){
            const page={schema:1,records:[
              {schema:1,instance:'c'.repeat(32),job:'a'.repeat(64),project:'b'.repeat(64),kind:'job_failed',code:'selected_job',observed_unix_ms:1,count:1},
              {schema:1,instance:'c'.repeat(32),job:'d'.repeat(64),project:'b'.repeat(64),kind:'job_failed',code:'other_job',observed_unix_ms:2,count:1},
            ],stored_bytes:100,malformed_records:0,tail_truncated:true};
            return window.hostReply(message,{epoch:message.epoch,current:true,reply:{httpStatus:200,body:JSON.stringify({schema:'airk-admin/1',ok:true,operation:message.operation,result:{status:'logs',page}})}});
          }
          if(message.operation==='usage.document'){const reply=message.payload.body.usage_document.read.report?window.nativeUsage:window.nativeUsageFailure;
            return window.hostReply(message,{epoch:message.epoch,current:true,reply:{httpStatus:reply.status,body:JSON.stringify(reply.body)}});}
          if(message.operation==='arbiter.configure'){const request=message.payload.body.arbiter.request;const {stage,configuration}=request;window.arbiterStatus.configuration={...window.arbiterStatus.configuration,[stage]:structuredClone(configuration),revision:2};window.arbiterStatus[stage]={...window.arbiterStatus[stage],selected_model:configuration.selected_model,device_policy:configuration.device_policy,gpu_precision:configuration.gpu_precision};}
          const result={schema:'airk-admin/1',ok:true,operation:message.operation,result:message.operation==='catalog.page'
            ?{status:'catalog_page',page:{namespace:'e'.repeat(32),revision:1,items:[],continuation:null}}
            :{status:'arbiter_device',observation:window.arbiterStatus}};
          return window.hostReply(message,{epoch:message.epoch,current:true,reply:{httpStatus:200,body:JSON.stringify(result)}});
        }
      }});`);
    const css = bundle.outputFiles.find(file => file.path.endsWith('.css')).text;
    await browser.evaluate(`{const style=document.createElement('style');style.textContent=${JSON.stringify(css)};document.head.append(style);}`);
    await browser.evaluate(bundle.outputFiles.find(file => file.path.endsWith('.js')).text);
    await browser.observed(`document.body.textContent.includes('Optional local models for Answer')`);
    await browser.observed(`window.requests.some(message=>message.operation==='arbiter.status')&&window.requests.some(message=>message.operation==='service.resources')`);
    assert.equal(await browser.evaluate(`window.requests.filter(message=>message.type==='admin.session').length`),1);
    assert.deepEqual(await browser.evaluate(`window.requests.filter(message=>message.type==='admin.call').map(message=>message.operation).sort()`),['arbiter.status','service.resources'],'opening the view reads current status without any mutation');
    assert.equal(await browser.evaluate(`document.querySelector('.airk-shared-management > details').open`),false);
    assert.equal(await browser.evaluate(`document.querySelector('.airk-shared-management > details code').textContent`),'c'.repeat(32));
    assert.equal(await browser.evaluate('document.querySelectorAll("nav").length'), 0, 'native consumer uses the owning Explorer navigation, without a nested browser shell');
    await browser.evaluate(`[...document.querySelectorAll('[aria-label="Runtime and models"] button')].find(button=>button.textContent==='Models').click()`);
    await browser.observed(`[...document.querySelectorAll('button')].some(button=>button.textContent==='Refresh Arbiter status'&&!button.disabled)`);
    await browser.observed(`document.body.textContent.includes(${JSON.stringify(stageStatus.focus.reason)})`);
    assert.deepEqual(await browser.evaluate(`JSON.parse([...document.querySelectorAll('details')].find(detail=>detail.querySelector('summary')?.textContent==='Details and complete management outcome').querySelector('pre').textContent)`),{status:'arbiter_device',observation:stageStatus},'complete native status retains its explicit future-field control');
    assert.equal(await browser.evaluate(`[...document.querySelectorAll('button')].filter(button=>['Focus','Arbiter'].some(stage=>button.textContent.startsWith('Download '+stage+' '))).length`),stageStatus.focus.models.concat(stageStatus.context.models).reduce((total,model)=>total+model.variants.length,0),'all native variants, including a genuinely empty roster, have exact download control coverage');
    assert.equal(await browser.evaluate(`[...document.querySelectorAll('button')].filter(button=>['Focus','Arbiter'].some(stage=>button.textContent.startsWith('Download '+stage+' '))).every(button=>button.disabled)`),true);
    assert.equal(await browser.evaluate(`document.querySelectorAll('input[type=radio][name="arbiter-focus-model"]').length`),2);
    assert.equal(await browser.evaluate(`document.querySelectorAll('input[type=radio][name="arbiter-context-model"]').length`),2);
    assert.deepEqual(await browser.evaluate(`[...document.querySelector('[aria-label="Focus device policy"]').options].map(option=>option.value)`),['auto','cpu']);
    assert.deepEqual(await browser.evaluate(`[...document.querySelector('[aria-label="Focus GPU precision"]').options].map(option=>option.value)`),['auto']);
    const nativeFocus=JSON.parse(await readFile(new URL('../../../fixtures/split/arbiter/device_cases.json',import.meta.url),'utf8')).cases.find(row=>row.id==='focus_cpu_local_candidate').status.focus;
    const fp16Only=nativeFocus.models[0].variants.filter(variant=>variant.precision==='float16');
    assert.equal(fp16Only.length,1);
    await browser.evaluate(`window.arbiterStatus.focus.models[0].variants=${JSON.stringify(fp16Only)};
      [...document.querySelectorAll('button')].find(button=>button.textContent==='Refresh Arbiter status').click();`);
    await browser.observed(`document.querySelector('[aria-label="Focus GPU precision"]').options.length===2`);
    assert.deepEqual(await browser.evaluate(`[...document.querySelector('[aria-label="Focus GPU precision"]').options].map(option=>option.value)`),['auto','fp16']);
    assert.equal(await browser.evaluate(`document.querySelector('[aria-label="Focus"] input[type=checkbox]').checked`), false);
    await browser.evaluate(`{const stage=document.querySelector('[aria-label="Focus"]');stage.querySelector('input[type=checkbox]').click();}`);
    assert.equal(await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Apply Focus choices').disabled`),true,'unqualified model cannot be enabled');
    await browser.evaluate(`{const stage=document.querySelector('[aria-label="Focus"]');stage.querySelector('input[type=checkbox]').click();
      const field=[...stage.querySelectorAll('label')].find(label=>label.textContent.includes('immutable index ID')).querySelector('input');field.value='idx_'+'d'.repeat(32);field.dispatchEvent(new Event('input',{bubbles:true}));
      const task=[...stage.querySelectorAll('label')].find(label=>label.textContent.includes('Rank retrieved evidence')).querySelector('input');task.click();}`);
    await browser.observed(`[...document.querySelectorAll('button')].some(button=>button.textContent==='Add explicit Focus enrollment to draft'&&!button.disabled)`);
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Add explicit Focus enrollment to draft').click()`);
    await browser.observed(`document.querySelector('[aria-label="Focus"]').textContent.includes('idx_'+'d'.repeat(32))`);
    await browser.evaluate(`{const stage=document.querySelector('[aria-label="Focus"]');[...stage.querySelectorAll('label')].find(label=>label.textContent.trim()==='Select Focus').querySelector('input').click();const device=document.querySelector('[aria-label="Focus device policy"]');device.value='cpu';device.dispatchEvent(new Event('change',{bubbles:true}));const precision=document.querySelector('[aria-label="Focus GPU precision"]');precision.value='fp16';precision.dispatchEvent(new Event('change',{bubbles:true}));}`);
    await browser.observed(`document.querySelector('[aria-label="Focus device policy"]').value==='cpu'&&document.querySelector('[aria-label="Focus GPU precision"]').value==='fp16'`);
    await browser.evaluate(`document.querySelector('[aria-label="Focus"] input[type=checkbox]').click()`);
    assert.equal(await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Apply Focus choices').disabled`),true,'off-stage model intent does not promote an unavailable model to enable admission');
    await browser.evaluate(`document.querySelector('[aria-label="Focus"] input[type=checkbox]').click()`);
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Apply Focus choices').click()`);
    await browser.observed(`window.requests.some(message=>message.operation==='arbiter.configure')`);
    const saved = await browser.evaluate(`window.requests.find(message=>message.operation==='arbiter.configure').payload.body`);
    assert.deepEqual(saved, { arbiter: { request: { operation: 'configure_device', stage:'focus', expected_revision:stageStatus.configuration.revision,
      configuration:{...stageStatus.configuration.focus,enabled:false,selected_model:'focus',device_policy:'cpu',gpu_precision:'fp16',indexes:[{index_id:'idx_'+'d'.repeat(32),tasks:['airk.focus_rank.v1']}]} } } });
    assert.deepEqual(await browser.evaluate('window.arbiterStatus.configuration.context'),stageStatus.configuration.context,'Focus changes preserve Context independently');
    assert.equal(await browser.evaluate('window.browserAuthTouches'),0);
    await browser.observed(`document.body.textContent.includes('"revision": 2')`);
    assert.deepEqual(await browser.evaluate(`JSON.parse([...document.querySelectorAll('details')].find(detail=>detail.querySelector('summary')?.textContent==='Details and complete management outcome').querySelector('pre').textContent)`),{status:'arbiter_device',observation:await browser.evaluate('window.arbiterStatus')},'complete CAS response retains every original status field including future_native');
    await browser.evaluate(`window.navigateManagement('index-library',{link:{page:'index-library',folder:'/work/selected'},serial:1})`);
    await browser.observed(`document.querySelector('[aria-label="Index library views"]')`);
    await browser.observed(`[...document.querySelectorAll('label')].some(label=>label.textContent.includes('Primary root (the folder to index)')&&label.querySelector('input')?.value==='/work/selected')`);
    await browser.evaluate(`[...document.querySelectorAll('[aria-label="Index library views"] button')].find(button=>button.textContent==='Create an index').click()`);
    await browser.observed(`[...document.querySelectorAll('h2')].some(heading=>heading.textContent==='Set up an index'&&heading.getClientRects().length)`);
    await browser.evaluate(`{const panel=[...document.querySelectorAll('.panel')].find(panel=>panel.querySelector('h2')?.textContent==='Set up an index');const input=[...panel.querySelectorAll('label')].find(label=>label.textContent.trim()==='Name').querySelector('input');input.value='Retained explicit draft';input.dispatchEvent(new Event('input',{bubbles:true}));}`);
    await browser.evaluate(`[...document.querySelectorAll('[aria-label="Index library views"] button')].find(button=>button.textContent==='Indexes').click()`);
    await browser.observed(`![...document.querySelectorAll('h2')].some(heading=>heading.textContent==='Set up an index'&&heading.getClientRects().length)`);
    await browser.evaluate(`[...document.querySelectorAll('[aria-label="Index library views"] button')].find(button=>button.textContent==='Create an index').click()`);
    assert.equal(await browser.evaluate(`{const panel=[...document.querySelectorAll('.panel')].find(panel=>panel.querySelector('h2')?.textContent==='Set up an index');[...panel.querySelectorAll('label')].find(label=>label.textContent.trim()==='Name').querySelector('input').value}`), 'Retained explicit draft');
    assert.equal(await browser.evaluate(`window.requests.some(message=>message.operation==='catalog.index_setup_draft'||message.operation==='catalog.index_setup_apply')`), false);
    await browser.evaluate(`window.navigateManagement('activity')`);
    await browser.evaluate(`window.navigateManagement('usage')`);
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
    await browser.evaluate(`window.navigateManagement('logs',{link:{page:'logs',logTarget:{job:'a'.repeat(64)}},serial:2})`);
    await browser.observed(`[...document.querySelectorAll('button')].some(button=>button.textContent==='Load recent logs'&&button.getClientRects().length)`);
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Load recent logs').click()`);
    await browser.observed(`[...document.querySelectorAll('td')].some(cell=>cell.textContent==='selected_job'&&cell.getClientRects().length)`);
    assert.equal(await browser.evaluate(`[...document.querySelectorAll('td')].some(cell=>cell.textContent==='other_job')`),false);
    assert.equal(await browser.evaluate(`[...document.querySelectorAll('pre')].some(pre=>pre.textContent.includes('other_job'))`),true,'the complete log result retains unmatched records');

    await browser.evaluate(`window.navigateManagement('service')`);
    await browser.evaluate(`[...document.querySelectorAll('[aria-label="Runtime and models"] button')].find(button=>button.textContent==='Runtime').click()`);
    await browser.observed(`[...document.querySelectorAll('summary')].find(summary=>summary.textContent==='Start AIRK when I sign in')`);
    await browser.evaluate(`[...document.querySelectorAll('summary')].find(summary=>summary.textContent==='Start AIRK when I sign in').click()`);
    await browser.observed(`[...document.querySelectorAll('h2')].some(heading=>heading.textContent==='Start at sign-in'&&heading.getClientRects().length)`);
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Close management session').click()`);
    await browser.observed(`document.body.textContent.includes('management connection ended')`);
    assert.equal(await browser.evaluate(`window.requests.filter(message=>message.type==='admin.session').length`), 1, 'close never automatically recaptures a replacement');
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Open native management').click()`);
    await browser.observed(`window.requests.filter(message=>message.type==='admin.session').length===2&&document.body.textContent.includes('Optional local models for Answer')`);
    await browser.observed(`[...document.querySelectorAll('button')].some(button=>button.textContent==='Refresh Arbiter status'&&!button.disabled)`);
    await browser.evaluate(`window.holdArbiter=true;[...document.querySelectorAll('button')].find(button=>button.textContent==='Refresh Arbiter status').click();`);
    await browser.observed('window.heldMessage');
    await browser.evaluate(`window.dispatchEvent(new MessageEvent('message',{data:{v:1,type:'admin.ended',epoch:9,status:401}}))`);
    assert.equal(await browser.evaluate(`document.body.textContent.includes('management connection ended')`), false, 'old epoch401 cannot end the explicitly reopened context');
    await browser.evaluate(`window.dispatchEvent(new MessageEvent('message',{data:{v:1,type:'admin.ended',epoch:10,status:401}}))`);
    await browser.observed(`document.body.textContent.includes('management connection ended')`);
    await browser.evaluate(`window.hostReply(window.heldMessage,{epoch:10,current:false,reply:{transport:true,httpStatus:401,message:'Original body unavailable'}})`);
    await browser.observed(`document.body.textContent.includes('management connection ended')`);
    assert.equal(await browser.evaluate('window.browserAuthTouches'), 0);
    await browser.evaluate('window.unmountManagement()');
  });
