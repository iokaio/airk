// SPDX-License-Identifier: Apache-2.0
// Private read-only acceptance observations over current compiled product views.
import assert from 'node:assert/strict';
import {readFile,mkdir,writeFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {createServer} from 'node:http';
import {once} from 'node:events';
import {browserHarness} from '../../../packages/ui/test/browserHarness.mjs';
const output=new URL('./help-acceptance-r4/',import.meta.url);await mkdir(output);
const root=new URL('../../../',import.meta.url),assetRoot=new URL('crates/airk-cli/admin-assets/',root);
const sha=b=>createHash('sha256').update(b).digest('hex');
const seal=JSON.parse(await readFile(new URL('manifest.json',assetRoot),'utf8'));
const files=new Map();for(const [name,row]of Object.entries(seal.files)){const bytes=await readFile(new URL(name,assetRoot));assert.equal(sha(bytes),row.sha256);files.set(name,bytes);}
const script=await readFile(new URL('packages/vscode/out/explorerWebview.js',root),'utf8'),css=await readFile(new URL('packages/vscode/out/explorerWebview.css',root),'utf8');
const pins={admin_manifest:sha(await readFile(new URL('manifest.json',assetRoot))),editor_js:sha(script),editor_css:sha(css),native_help:sha(await readFile(new URL('crates/airk-mcp/src/help/content.json',root))),script:sha(await readFile(new URL(import.meta.url)))};
const receipt={schema:'airk.private-help-acceptance-observation/1',started_utc:new Date().toISOString(),inputs_before:pins,cases:[],browsers:[],unexpected_requests:[],scope:'Actual sealed Admin HTML/SRI/module; actual compiled Explorer module/CSS with synthetic closed VSCode host. No installed host/native management/NN claim. Mechanical driver observations, not human usability scoring.'};
const persist=()=>writeFile(new URL('receipt.json',output),JSON.stringify(receipt,null,2)+'\n',{encoding:'utf8'});
const server=createServer((request,response)=>{const pathname=new URL(request.url,'http://localhost').pathname;if(pathname==='/editor/'){response.setHeader('Content-Type','text/html');response.end('<!doctype html><html><head></head><body><div id="root"></div></body></html>');return;}const name=pathname==='/admin/'?'index.html':pathname.startsWith('/admin/assets/')?pathname.slice(14):null;if(name&&files.has(name)){response.setHeader('Content-Type',seal.files[name].content_type);response.end(files.get(name));return;}if(pathname!=='/favicon.ico')receipt.unexpected_requests.push(pathname);response.statusCode=404;response.end('Not found');});server.listen(0,'127.0.0.1');await once(server,'listening');
const press=async(browser,key,code,n)=>{await browser.call('Input.dispatchKeyEvent',{type:'keyDown',key,code,windowsVirtualKeyCode:n,...(key==='Enter'?{text:'\r',unmodifiedText:'\r'}:{})});await browser.call('Input.dispatchKeyEvent',{type:'keyUp',key,code,windowsVirtualKeyCode:n});};
try{
for(const host of ['browser','editor']){
 const browser=await browserHarness();
 try{
 for(const viewport of [{name:'desktop-1366',width:1366,height:768},{name:'desktop-1440',width:1440,height:1000},{name:'narrow-390',width:390,height:760},{name:'css-equivalent-200percent',width:683,height:384}]){
  const row={host,viewport,stage:'load',keyboard:[],internal_ids_supplied:[]};receipt.cases.push(row);await persist();
  await browser.call('Emulation.setDeviceMetricsOverride',{width:viewport.width,height:viewport.height,deviceScaleFactor:1,mobile:false});
  const freshURL=`http://127.0.0.1:${server.address().port}/${host==='browser'?'admin':'editor'}/?acceptance=${host}-${viewport.name}${host==='browser'?'#help':''}`;
  await browser.call('Page.navigate',{url:freshURL});
  await browser.observed(`document.readyState==='complete'&&new URL(performance.getEntriesByType('navigation')[0]?.name??'about:blank').search===${JSON.stringify(new URL(freshURL).search)}`);
  if(host==='browser')await browser.observed(`document.querySelector('[aria-label="Offline Help"]')`);
  else{
   await browser.evaluate(`document.head.innerHTML='';{const style=document.createElement('style');style.textContent=${JSON.stringify(css)};document.head.append(style);}document.body.className='vscode-light';document.body.innerHTML='<div id="root"></div>';window.requests=[];window.acquireVsCodeApi=()=>({getState:()=>({route:'#help'}),setState:()=>{},postMessage(message){window.requests.push(message);if(message.type==='ready')queueMicrotask(()=>window.dispatchEvent(new MessageEvent('message',{data:{v:1,type:'state',state:{revision:1,extensionVersion:'1.9.0',trusted:false,workspaceTrusted:false,nativeExecutionEnabled:false,connection:'disconnected',projects:[],features:{help:'implemented'}}}})));}});`);
   await browser.evaluate(script);await browser.observed(`document.querySelector('input[aria-label="Search help"]')`);
  }
  row.stage='initial-observation';
  row.initial=await browser.evaluate(`(()=>{const rect=el=>{if(!el)return null;const r=el.getBoundingClientRect();return {text:el.textContent.trim(),x:r.x,y:r.y,width:r.width,height:r.height,visible_without_scroll:r.top>=0&&r.bottom<=innerHeight&&r.left>=0&&r.right<=innerWidth};};return {body_font_px:getComputedStyle(document.body).fontSize,help_font_px:getComputedStyle(document.querySelector('.bundled-help,.help')).fontSize,viewport:{width:innerWidth,height:innerHeight},scroll:{x:scrollX,y:scrollY,total_height:document.scrollingElement.scrollHeight},horizontal_overflow:document.scrollingElement.scrollWidth>innerWidth,main_scroll_regions:[...document.querySelectorAll('*')].filter(el=>['auto','scroll'].includes(getComputedStyle(el).overflowY)&&el.scrollHeight>el.clientHeight+1).map(el=>({tag:el.tagName,class:el.className,height:el.clientHeight,total:el.scrollHeight})),tasks:['Create an index','Connect an agent','Find missing results'].map(label=>({label,control:rect([...document.querySelectorAll('button')].find(el=>el.textContent.trim()===label))})),warnings:[...document.querySelectorAll('[role="alert"]')].map(rect)};})()`);
  if(viewport.name==='desktop-1366'||viewport.name==='narrow-390'){const shot=await browser.call('Page.captureScreenshot',{format:'png'});await writeFile(new URL(`${host}-${viewport.name}-initial.png`,output),Buffer.from(shot.data,'base64'));}
  if(host==='editor'&&row.initial.tasks.some(task=>task.control===null)){row.product_defect='Required task landing controls absent in compiled editor Help';throw Error(row.product_defect);}
  row.stage='keyboard-task';
  const target='Create an index';
  for(let step=0;step<80;step++){
   await press(browser,'Tab','Tab',9);
   const focus=await browser.evaluate(`({tag:document.activeElement?.tagName,text:document.activeElement?.textContent?.trim(),label:document.activeElement?.getAttribute('aria-label'),type:document.activeElement?.getAttribute('type')})`);row.keyboard.push(focus);
   if(focus.text===target)break;
  }
  const focused=row.keyboard.at(-1);row.keyboard_target_reached=focused.text===target;
  if(row.keyboard_target_reached){
   await press(browser,'Enter','Enter',13);await browser.observed(`document.activeElement?.tagName==='ARTICLE'`);
   row.article=await browser.evaluate(`(()=>{const article=document.activeElement,heading=article.querySelector('h2,h3'),r=heading.getBoundingClientRect();return {heading:heading.textContent,top:r.top,bottom:r.bottom,heading_visible:r.top>=0&&r.bottom<=innerHeight,focus_tag:article.tagName,scroll_y:scrollY,query:document.querySelector('input[type="search"]')?.value};})()`);
   row.stage='return-focus';
   await browser.evaluate(`[...document.querySelectorAll('article button')].find(button=>button.textContent.trim()==='Back to guides').focus()`); // isolated Back control setup; not a claim of keyboard-only traversal through the whole article.
   await press(browser,'Enter','Enter',13);await browser.observed(`document.activeElement?.type==='search'||document.activeElement?.getAttribute('aria-label')==='Search help'`);row.return_focus=await browser.evaluate(`({tag:document.activeElement.tagName,type:document.activeElement.type})`);
  }else if(row.keyboard_target_reached){
   await browser.call('Input.insertText',{text:'source freshness'});await browser.observed(`document.body.textContent.includes('matching topic')`);
   row.search=await browser.evaluate(`({query:document.querySelector('input[aria-label="Search help"]').value,matches:document.querySelector('[role="status"]')?.textContent})`);
  }
  row.stage='completed';await persist();
 }
 }catch(error){receipt.error=String(error.stack??error);try{receipt.failure_dom=await browser.evaluate(`({url:location.href,active:document.activeElement?.outerHTML,articles:[...document.querySelectorAll('article')].map(article=>({heading:article.querySelector('h2,h3')?.textContent,rect:article.getBoundingClientRect().toJSON()})),text:document.body.textContent.slice(0,2000)})`);const shot=await browser.call('Page.captureScreenshot',{format:'png'});await writeFile(new URL('failure.png',output),Buffer.from(shot.data,'base64'));}catch(diagnostic){receipt.failure_diagnostic_error=String(diagnostic);}await persist();throw error;}finally{try{await browser.close();}finally{receipt.browsers.push(browser.observation);await persist();}}
}
assert.equal(receipt.unexpected_requests.length,0,'offline Help must not make API requests');
assert.equal(pins.editor_js,sha(await readFile(new URL('packages/vscode/out/explorerWebview.js',root))));assert.equal(pins.editor_css,sha(await readFile(new URL('packages/vscode/out/explorerWebview.css',root))));for(const [name,bytes]of files)assert.equal(sha(bytes),sha(await readFile(new URL(name,assetRoot))));receipt.inputs_unchanged=true;receipt.finished_utc=new Date().toISOString();await persist();console.log(JSON.stringify({cases:receipt.cases.length,browsers:receipt.browsers,receipt:new URL('receipt.json',output).pathname}));
}finally{server.closeAllConnections();await new Promise(resolve=>server.close(resolve));}
