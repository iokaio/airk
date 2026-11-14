// SPDX-License-Identifier: Apache-2.0
// Execute the actual sealed Admin module with its HTML/SRI, not a test re-bundle.
import assert from 'node:assert/strict';
import test from 'node:test';
import {createServer} from 'node:http';
import {once} from 'node:events';
import {readFile} from 'node:fs/promises';
import {browserAvailable,browserHarness} from './browserHarness.mjs';
test('actual generated Admin module keeps offline native Help and complete tool JSON after escaped-string codegen',
 {skip:browserAvailable?false:'Chromium unavailable.'},async()=>{
  const assets=new URL('../../../crates/airk-cli/admin-assets/',import.meta.url);
  const seal=JSON.parse(await readFile(new URL('manifest.json',assets),'utf8'));
  const native=JSON.parse(await readFile(new URL('../../../crates/airk-mcp/src/help/content.json',import.meta.url),'utf8'));
  const files=new Map(await Promise.all(Object.keys(seal.files).map(async name=>[name,await readFile(new URL(name,assets))])));
  const unexpected=[];
  const server=createServer((request,response)=>{
    const path=new URL(request.url,'http://localhost').pathname;
    const name=path==='/admin/'?'index.html':path.startsWith('/admin/assets/')?path.slice('/admin/assets/'.length):null;
    if(name&&files.has(name)){response.setHeader('Content-Type',seal.files[name].content_type);response.end(files.get(name));return;}
    unexpected.push(path);response.statusCode=404;response.end('Not found');
  });
  server.listen(0,'127.0.0.1');await once(server,'listening');
  let browser;
  try{
    browser=await browserHarness();
    await browser.call('Page.navigate',{url:`http://127.0.0.1:${server.address().port}/admin/#help`});
    await browser.observed(`document.querySelector('[aria-label="Offline Help"] input[type="search"]')`);
    await browser.evaluate(`{const details=[...document.querySelectorAll('details')].find(node=>node.querySelector('summary')?.textContent==='Bundled public tool reference');details.open=true;const select=details.querySelector('select');select.value='help';select.dispatchEvent(new Event('change',{bubbles:true}));}`);
    await browser.observed(`document.body.textContent.includes('Complete native tool definition (JSON)')`);
    const raw=await browser.evaluate(`JSON.parse([...document.querySelectorAll('pre')].find(pre=>{try{return JSON.parse(pre.textContent)?.name==='help';}catch{return false;}}).textContent)`);
    assert.deepEqual(raw,native.tools.find(tool=>tool.name==='help'));
    const guide=native.topics.find(topic=>topic.id==='installation');assert.ok(guide);
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent.trim()===${JSON.stringify(`Engine: ${guide.title}`)}).click()`);
    await browser.observed(`document.querySelector('[aria-label="Offline Help"] article')`);
    assert.equal(await browser.evaluate(`document.querySelector('[aria-label="Offline Help"] article h3').textContent`),`Engine: ${guide.title}`);
    assert.deepEqual(unexpected.filter(path=>path!=='/favicon.ico'),[],'offline Help performs no native/API read');
  }finally{try{await browser?.close();}finally{await new Promise(resolve=>server.close(resolve));}}
});

test('actual generated Explorer module keeps offline Help and the whole native tool definition after escaped-string codegen',
 {skip:browserAvailable?false:'Chromium unavailable.'},async()=>{
  const source=await readFile(new URL('../../vscode/out/explorerWebview.js',import.meta.url),'utf8');
  const native=JSON.parse(await readFile(new URL('../../../crates/airk-mcp/src/help/content.json',import.meta.url),'utf8'));
  const browser=await browserHarness();
  try{
    await browser.evaluate(`{const root=document.createElement('div');root.id='root';document.body.append(root);window.requests=[];
      window.acquireVsCodeApi=()=>({getState:()=>({route:'#help'}),setState:()=>{},postMessage(message){window.requests.push(message);
        if(message.type==='ready'){queueMicrotask(()=>window.dispatchEvent(new MessageEvent('message',{data:{v:1,type:'state',state:{revision:1,extensionVersion:'1.9.0',trusted:false,workspaceTrusted:false,nativeExecutionEnabled:false,connection:'disconnected',projects:[],features:{help:'implemented'}}}})));return;}
        queueMicrotask(()=>window.dispatchEvent(new MessageEvent('message',{data:{v:1,type:'result',requestId:message.requestId,ok:true,data:{}}})));
      }});}`);
    await browser.evaluate(source);
    await browser.observed(`document.querySelector('input[aria-label="Search help"]')`);
    await browser.evaluate(`{const details=[...document.querySelectorAll('details')].find(node=>node.querySelector('summary')?.textContent==='Bundled public tool reference');details.open=true;const select=details.querySelector('select');select.value='help';select.dispatchEvent(new Event('change',{bubbles:true}));}`);
    await browser.observed(`document.body.textContent.includes('Complete native tool definition (JSON)')`);
    const raw=await browser.evaluate(`JSON.parse([...document.querySelectorAll('pre')].find(pre=>{try{return JSON.parse(pre.textContent)?.name==='help';}catch{return false;}}).textContent)`);
    assert.deepEqual(raw,native.tools.find(tool=>tool.name==='help'));
    assert.deepEqual(await browser.evaluate(`window.requests.filter(message=>['help.engine','pilot.read','admin.call','admin.session'].includes(message.type))`),[]);
  }finally{await browser.close();}
});
