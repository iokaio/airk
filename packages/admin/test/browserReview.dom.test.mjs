// SPDX-License-Identifier: Apache-2.0
// Actual shared Review/browser callbacks over synthetic capture replies; no Git/native qualification claim.
import assert from 'node:assert/strict';
import {before,after,test} from 'node:test';
import {createServer} from 'node:http';
import {once} from 'node:events';
import {build} from 'esbuild';
import {fileURLToPath} from 'node:url';
import {browserAvailable,browserHarness} from '../../ui/test/browserHarness.mjs';
const root=fileURLToPath(new URL('../',import.meta.url));let browser,server;
before(async()=>{if(!browserAvailable)return;server=createServer((request,response)=>{response.setHeader('Content-Type','text/html');response.end('<!doctype html><html><body><div id="root"></div></body></html>');});server.listen(0,'127.0.0.1');await once(server,'listening');browser=await browserHarness();await browser.call('Page.navigate',{url:`http://127.0.0.1:${server.address().port}/review`});await browser.observed(`document.readyState==='complete'&&location.pathname==='/review'`);});
after(async t=>{try{await browser?.close();}finally{t.diagnostic(JSON.stringify({browser_ownership:browser?.observation}));if(server){server.closeAllConnections();await new Promise(resolve=>server.close(resolve));}}});
const bundle=await build({absWorkingDir:root,stdin:{contents:"import {h,render} from 'preact';import {BrowserReviewScreen} from './src/ReviewScreen';render(h(BrowserReviewScreen,{route:window.route,scopeIdentity:'56'.repeat(32),disabled:false,capture:()=>window.capture,active:true}),document.getElementById('root'));",resolveDir:root,sourcefile:'browser-review-dom.tsx'},bundle:true,write:false,format:'iife',platform:'browser',jsx:'automatic',jsxImportSource:'preact',metafile:true});
assert.equal(Object.keys(bundle.metafile.inputs).some(path=>path.endsWith('/explorerReview.ts')||path.includes('mcpStdioSession')||path.includes('airkctl/runner')),false);
for(const route of ['review/changes','review/impact','review/tests'])test(`browser ${route} uses explicit inputs shared native-read algorithm and full exports`,{skip:browserAvailable?false:'Chromium unavailable.'},async()=>{
 await browser.evaluate(`document.body.innerHTML='<div id="root"></div>';window.route=${JSON.stringify(route)};window.calls=[];window.copied=undefined;window.capture={indexIds:['idx_'+'23'.repeat(16)],backingBinding:'ab'.repeat(32),checkCurrent(){},async read(tool,args){window.calls.push({tool,args});const publication={collection:'fixture',token:'airk-r1:'+'11'.repeat(16)+':1:'+'22'.repeat(32)};let logical={publication,future_native:{kept:['all','fields']}};if(tool==='file_info')logical={...logical,content_hash:'78'.repeat(32),indexed_lines:4,source:{root:'synthetic',relative_path:args.path}};if(tool==='symbols')logical={...logical,files:[{path:args.path_prefix,content_hash:'78'.repeat(32)}],items:[{file:0,kind:'function',lines:[1,1],name:'refreshSession',signature:'export function refreshSession()'}]};if(tool==='references')logical={...logical,evidence:{hits:[{citation:{path:'tests/session.test.ts',start_line:3,end_line:3,content_hash:'78'.repeat(32)},text:'refreshSession()'}]}};if(tool==='list_files')logical={...logical,paths:['tests/session.test.ts']};return {scopeIdentity:'56'.repeat(32),request:{tool,arguments:args},logical,raw:{original:logical,future_outer:{preserved:true}}};}};Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async text=>window.copied=text}});`);
 await browser.evaluate(bundle.outputFiles[0].text);await browser.observed(`!!document.querySelector('form textarea')`);
 await browser.evaluate(`(()=>{const area=document.querySelector('form textarea');area.value=['src/session.rs','tests/session.test.ts'].join(String.fromCharCode(10));area.dispatchEvent(new Event('input',{bubbles:true}));})()`);
 await browser.evaluate(`document.querySelector('form').dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}))`);
 await browser.observed(`!![...document.querySelectorAll('summary')].find(node=>node.textContent==='Complete Review result, original inputs and all native read receipts')`);
 await browser.evaluate(`[...document.querySelectorAll('button')].find(node=>node.textContent==='Copy complete Review JSON').click()`);await browser.observed(`!!window.copied`);
 const copy=JSON.parse(await browser.evaluate('window.copied'));assert.equal(copy.format,'paths');assert.equal(copy.input,'src/session.rs\ntests/session.test.ts');assert.ok(copy.reads.length>0);assert.deepEqual(copy.reads[0].raw.future_outer,{preserved:true});assert.ok(copy.result.notes.some(note=>note.includes('supplied')||note.includes('Manual path')));assert.equal(copy.pendingReads,0);
 if(route==='review/tests')assert.ok(copy.result.candidates.some(candidate=>candidate.path==='tests/session.test.ts'));
 assert.equal(await browser.evaluate(`document.body.textContent.includes('Open source')`),false);assert.equal(await browser.evaluate(`document.body.textContent.includes('matches_index')`),false);
});
