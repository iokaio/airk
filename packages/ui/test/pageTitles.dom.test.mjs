// SPDX-License-Identifier: Apache-2.0
// Actual Admin components and browser DOM; pending synthetic reads, no native parity claim.
import assert from 'node:assert/strict';
import test from 'node:test';
import {createServer} from 'node:http';
import {once} from 'node:events';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
import {browserAvailable,browserHarness} from './browserHarness.mjs';
const {build}=createRequire(new URL('../../vscode/package.json',import.meta.url))('esbuild');
const root=fileURLToPath(new URL('../../../',import.meta.url));

test('all twelve Admin destinations expose their exact independent rail label as one visible page heading',
 {skip:browserAvailable?false:'Chromium unavailable.'},async t=>{
 const bundle=await build({stdin:{resolveDir:`${root}/packages/admin/src`,sourcefile:'page-titles-observation.tsx',contents:`
  import {h,render} from 'preact';import {App} from './app';import {restoreSession} from './session';
  sessionStorage.setItem('airk-admin-csrf:'+ 'c'.repeat(32),'synthetic-title-view-only');restoreSession();
  window.fetch=()=>new Promise(()=>{});render(h(App,{initialGate:{kind:'none'}}),document.body);
 `},bundle:true,write:false,format:'iife',platform:'browser',jsx:'automatic',jsxImportSource:'preact'});
 const server=createServer((_request,response)=>{response.setHeader('Content-Type','text/html');response.end('<!doctype html><title>Isolated title view test</title><body></body>');});
 server.listen(0,'127.0.0.1');await once(server,'listening');let browser;
 try{
  browser=await browserHarness();await browser.call('Page.navigate',{url:`http://127.0.0.1:${server.address().port}/admin/`});
  await browser.observed(`document.readyState==='complete'`);await browser.evaluate(bundle.outputFiles[0].text);
  await browser.observed(`document.querySelector('nav[aria-label="AIRK main navigation"]')`);
  // Deliberately independent of MAIN_DESTINATIONS and production title lookup.
  const titles=['Overview','Search','Indexes','Activity','Agents','Service','Code','Documents','Evidence','Review','Strategies','Help'];
  for(const title of titles){
   await browser.evaluate(`document.querySelector('nav button[aria-label="${title}"]').click()`);
   await browser.observed(`document.querySelector('nav [aria-current="page"]')?.getAttribute('aria-label')===${JSON.stringify(title)}`);
   const headings=await browser.evaluate(`[...document.querySelectorAll('main h1,main h2')].filter(e=>e.getClientRects().length).map(e=>e.textContent)`);
   assert.equal(headings.filter(text=>text===title).length,1,title+' must have one matching visible heading');
  }
  // Grouped service/activity routes retain the main destination title.
  for(const [route,title]of [['storage','Service'],['recovery','Service'],['logs','Activity'],['usage','Activity']]){
   await browser.evaluate(`location.hash=${JSON.stringify(route)}`);
   await browser.observed(`document.querySelector('main > h1')?.textContent===${JSON.stringify(title)}`);
   assert.equal(await browser.evaluate(`document.querySelectorAll('main > h1').length`),1);
  }
 }finally{try{await browser?.close();}finally{server.closeAllConnections();await new Promise(resolve=>server.close(resolve));t.diagnostic(JSON.stringify({browser_ownership:browser?.observation}));}}
});
