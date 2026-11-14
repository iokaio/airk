// SPDX-License-Identifier: Apache-2.0
// Real outer Explorer and shared Admin lifecycle; synthetic host, no native parity claim.
import assert from 'node:assert/strict';
import test from 'node:test';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
import {readFile} from 'node:fs/promises';
import {browserAvailable,browserHarness} from './browserHarness.mjs';
const require=createRequire(new URL('../../vscode/package.json',import.meta.url));
const {build}=require('esbuild'),{sharedAdminStylePlugin}=require('./scripts/build-explorer.cjs');
const root=fileURLToPath(new URL('../../vscode/',import.meta.url));
test('actual outer Explorer preserves draft nodes and pending outcomes through shared pages Help and Overview, then resets for a new native epoch',
 {skip:browserAvailable?false:'Chromium unavailable.'},async()=>{
  const manifest=await readFile(new URL('../generated/help-manifest.json',import.meta.url),'utf8');
  const bundle=await build({absWorkingDir:root,stdin:{resolveDir:root,sourcefile:'outer-management-observation.tsx',contents:`import {h,render} from 'preact';import {App} from './src/explorerWebview/app';import {ProgressPanel} from '../admin/src/progress';import {PageVisible} from '../admin/src/pageVisibility';window.mountProgressProbe=visible=>render(h(PageVisible.Provider,{value:visible},h(ProgressPanel,{binding:'a'.repeat(64)})),document.body);render(h(App,{}),document.body);`},
    bundle:true,write:false,outdir:'out/unused-outer-dom',format:'iife',platform:'browser',jsx:'automatic',jsxImportSource:'preact',alias:{preact:`${root}/node_modules/preact`},plugins:[sharedAdminStylePlugin(),{
      name:'actual-help-manifest',setup(plugin){plugin.onResolve({filter:/^airk-explorer-help$/},()=>({path:'help',namespace:'observed-help'}));plugin.onLoad({filter:/.*/,namespace:'observed-help'},()=>({contents:manifest,loader:'json'}));}
    }]});
  const browser=await browserHarness();
  try{
    await browser.evaluate(`window.requests=[];window.epoch=11;window.state={revision:1,extensionVersion:'1.9.0',connection:'ready',trusted:true,workspaceTrusted:true,nativeExecutionEnabled:true,projects:[],workspaceFolders:[],features:{'indexing/library':'implemented','manage/service':'implemented',help:'implemented'},adminManagement:{epoch:11,available:true}};
      window.emit=data=>window.dispatchEvent(new MessageEvent('message',{data:{v:1,...data}}));
      window.reply=(message,data)=>queueMicrotask(()=>window.emit({type:'result',requestId:message.requestId,ok:true,data}));
      window.navigate=route=>window.emit({type:'navigate',route});
      window.acquireVsCodeApi=()=>({getState:()=>({route:'#indexing/library'}),setState:()=>{},postMessage(message){window.requests.push(message);
        if(message.type==='ready'){queueMicrotask(()=>window.emit({type:'state',state:window.state}));return;}
        if(message.type==='admin.session')return window.reply(message,{epoch:message.epoch,serviceInstance:'c'.repeat(32),pending:[]});
        if(message.type==='admin.call'){
          if(message.operation==='files.browse'&&window.holdBrowse){window.heldBrowse=message;return;}
          if(message.operation==='https.trust_status'){window.heldTrust=message;return;}
          const result=message.operation==='catalog.page'?{status:'catalog_page',page:{namespace:'e'.repeat(32),revision:1,items:[],continuation:null}}:message.operation==='indexes.progress'?{pending_job:{state:'running'}}:{};
          return window.reply(message,{epoch:message.epoch,current:true,reply:{httpStatus:200,body:JSON.stringify({schema:'airk-admin/1',ok:true,operation:message.operation,result})}});
        }
        window.reply(message,{});
      }});`);
    await browser.evaluate(bundle.outputFiles.find(file=>file.path.endsWith('.js')).text);
    await browser.observed(`document.querySelector('[aria-label="Index library views"]')`);
    await browser.evaluate(`[...document.querySelectorAll('[aria-label="Index library views"] button')].find(button=>button.textContent==='Create an index').click()`);
    await browser.observed(`[...document.querySelectorAll('h2')].some(h=>h.textContent==='Set up an index'&&h.getClientRects().length)`);
    await browser.evaluate(`{const panel=[...document.querySelectorAll('.panel')].find(p=>p.querySelector('h2')?.textContent==='Set up an index');window.originalDraft=[...panel.querySelectorAll('label')].find(label=>label.textContent.trim()==='Name').querySelector('input');originalDraft.value='Outer navigation retained draft';originalDraft.dispatchEvent(new Event('input',{bubbles:true}));[...panel.querySelectorAll('button')].find(button=>button.textContent==='Browse…').click();}`);
    await browser.observed(`document.querySelector('[role="dialog"][aria-modal="true"]')`);
    await browser.evaluate(`window.navigate('#manage/service')`);
    await browser.observed(`document.querySelector('[aria-label="Service settings views"]')`);
    await browser.observed(`!document.querySelector('[role="dialog"][aria-modal="true"]')&&![...document.querySelectorAll('.rail,.topbar')].some(node=>node.inert)`);
    assert.equal(await browser.evaluate(`window.originalDraft.isConnected&&window.originalDraft.value==='Outer navigation retained draft'&&!window.originalDraft.getClientRects().length`),true);
    await browser.evaluate(`[...document.querySelectorAll('[aria-label="Service settings views"] button')].find(button=>button.textContent==='TLS and trust').click()`);
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Trust status').click()`);
    await browser.observed(`window.heldTrust`);
    await browser.evaluate(`window.navigate('#help')`);
    await browser.observed(`document.querySelector('[aria-label="Service settings views"]').getClientRects().length===0`);
    await browser.evaluate(`window.navigate('#overview')`);
    await browser.observed(`document.querySelector('.rail [aria-current="page"]')?.textContent.includes('Overview')`);
    assert.equal(await browser.evaluate('window.originalDraft.isConnected'),true);
    await browser.evaluate(`window.reply(window.heldTrust,{epoch:11,current:true,reply:{httpStatus:200,body:JSON.stringify({schema:'airk-admin/1',ok:true,operation:'https.trust_status',result:{full_original_native:{future:['retained-through-navigation']}}})}});window.navigate('#manage/service')`);
    await browser.observed(`document.querySelector('[aria-label="Service settings views"]').getClientRects().length>0&&document.body.textContent.includes('retained-through-navigation')`);
    assert.equal(await browser.evaluate(`window.requests.filter(message=>message.operation==='https.trust_status').length`),1,'navigation never repeats the held native read');
    await browser.evaluate(`window.navigate('#indexing/library')`);
    await browser.observed(`window.originalDraft.getClientRects().length>0`);
    assert.equal(await browser.evaluate(`window.originalDraft.value`),'Outer navigation retained draft');
    assert.equal(await browser.evaluate(`window.requests.filter(message=>message.type==='admin.session').length`),1);
    assert.equal(await browser.evaluate(`window.requests.some(message=>message.operation==='catalog.index_setup_draft'||message.operation==='catalog.index_setup_apply')`),false,'navigation never submits an edited form');
    await browser.evaluate(`window.state={...window.state,adminManagement:{epoch:12,available:true}};window.emit({type:'state',state:window.state})`);
    await browser.observed(`window.requests.filter(message=>message.type==='admin.session').length===2&&!window.originalDraft.isConnected`);
    // A nonempty initial path would normally cause another browse after the
    // start-list reply. Hide/unmount and adopt a new session before that reply.
    await browser.evaluate(`[...document.querySelectorAll('[aria-label="Index library views"] button')].find(button=>button.textContent==='Create an index').click()`);
    await browser.observed(`[...document.querySelectorAll('h2')].some(h=>h.textContent==='Set up an index'&&h.getClientRects().length)`);
    await browser.evaluate(`{window.holdBrowse=true;const panel=[...document.querySelectorAll('.panel')].find(p=>p.querySelector('h2')?.textContent==='Set up an index');const primary=[...panel.querySelectorAll('label')].find(label=>label.textContent.includes('Primary root')).querySelector('input');primary.value='C:/original/missing';primary.dispatchEvent(new Event('input',{bubbles:true}));}`);
    await browser.evaluate(`[...document.querySelectorAll('.panel')].find(p=>p.querySelector('h2')?.textContent==='Set up an index').querySelector('button').click()`);
    await browser.observed(`window.heldBrowse&&document.querySelector('[role="dialog"]')`);
    const browseCount=await browser.evaluate(`window.requests.filter(message=>message.operation==='files.browse').length`);
    await browser.evaluate(`window.navigate('#help');window.state={...window.state,adminManagement:{epoch:13,available:true}};window.emit({type:'state',state:window.state})`);
    await browser.observed(`window.requests.filter(message=>message.type==='admin.session').length===3&&!document.querySelector('[role="dialog"]')`);
    await browser.evaluate(`window.reply(window.heldBrowse,{epoch:12,current:false,reply:{httpStatus:200,body:JSON.stringify({schema:'airk-admin/1',ok:true,operation:'files.browse',result:{listing:{schema:1,entries:[],breadcrumb:[]}}})}})`);
    await browser.evaluate(`new Promise(resolve=>requestAnimationFrame(resolve))`);
    assert.equal(await browser.evaluate(`window.requests.filter(message=>message.operation==='files.browse').length`),browseCount,'closed old initialization cannot browse its initial path or parent under the replacement session');
    assert.equal(await browser.evaluate(`document.querySelector('[role="dialog"]')!==null`),false,'late start-list reply cannot restore a closed modal');
    // Exercise the actual recurring native read with an injected timer observer,
    // including a callback already queued before its page became hidden.
    await browser.evaluate(`window.pollTimers=[];const originalTimer=window.setTimeout;window.setTimeout=(callback,delay,...args)=>{if(delay===2000){window.pollTimers.push(callback);return -window.pollTimers.length;}return originalTimer(callback,delay,...args);};window.mountProgressProbe(true)`);
    await browser.observed(`window.pollTimers.length===1`);
    const before=await browser.evaluate(`window.requests.filter(message=>message.operation==='indexes.progress').length`);
    await browser.evaluate(`window.mountProgressProbe(false)`);
    await browser.observed(`document.body.textContent.includes('Refreshing stopped while this tab is hidden.')`);
    await browser.evaluate(`window.pollTimers[0]()`);
    assert.equal(await browser.evaluate(`window.requests.filter(message=>message.operation==='indexes.progress').length`),before,'a stale queued poll cannot dispatch from a hidden retained page');
    await browser.evaluate(`window.mountProgressProbe(true)`);
    await browser.observed(`window.requests.filter(message=>message.operation==='indexes.progress').length===${before+1}`);
  }finally{await browser.close();}
});
