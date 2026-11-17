// SPDX-License-Identifier: Apache-2.0
import {browserHarness} from '../../../packages/ui/test/browserHarness.mjs';
import {writeFile} from 'node:fs/promises';
const browser=await browserHarness(),r={scope:'Synthetic button CDP input fidelity only',cases:[]};
try{
 await browser.evaluate(`document.body.innerHTML='<button id="target">Create an index</button>';window.events=[];target.addEventListener('click',event=>window.events.push({type:event.type,trusted:event.isTrusted}));target.focus();`);
 for(const variant of ['original','with-text']){
  await browser.evaluate('target.focus();window.events=[]');
  await browser.call('Input.dispatchKeyEvent',{type:'keyDown',key:'Enter',code:'Enter',windowsVirtualKeyCode:13,...(variant==='with-text'?{text:'\r',unmodifiedText:'\r'}:{})});
  await browser.call('Input.dispatchKeyEvent',{type:'keyUp',key:'Enter',code:'Enter',windowsVirtualKeyCode:13});
  r.cases.push({variant,events:await browser.evaluate('window.events')});
 }
}finally{await browser.close();r.browser=browser.observation;await writeFile(new URL('./help-enter-proof-r1.json',import.meta.url),JSON.stringify(r,null,2)+'\n');}
console.log(JSON.stringify(r));
