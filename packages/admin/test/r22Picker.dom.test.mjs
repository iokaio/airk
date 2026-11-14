// SPDX-License-Identifier: Apache-2.0
// Real Chromium DOM and keyboard behavior; no service, shell or user browser profile.
import assert from 'node:assert/strict';
import { after, before, beforeEach, test } from 'node:test';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { watch } from 'node:fs';
import { access, mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';

const src = resolve(dirname(fileURLToPath(import.meta.url)), '../src');
const browserPath = process.env.AIRK_BROWSER ?? 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe';
const available = await access(browserPath).then(() => true, () => false);
const browserTest = available ? test : (name, run) => test(name, { skip: 'Set AIRK_BROWSER to a Chromium executable for real DOM qualification.' }, run);
let browser, socket, scratch, call;
async function evaluate(expression) {
  const reply = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
  assert.equal(reply.exceptionDetails, undefined, reply.exceptionDetails?.text);
  return reply.result.value;
}
async function key(key, modifiers = 0) {
  await call('Input.dispatchKeyEvent', { type: 'keyDown', key, code: key, modifiers,
    windowsVirtualKeyCode: key === 'Tab' ? 9 : 27 });
  await call('Input.dispatchKeyEvent', { type: 'keyUp', key, code: key, modifiers,
    windowsVirtualKeyCode: key === 'Tab' ? 9 : 27 });
}
before(async () => {
  if (!available) return;
  const bundle = await build({
    stdin: { resolveDir: src, sourcefile: 'r22-dom.tsx', contents: `
      import {h,render} from 'preact'; import {FolderPicker} from './folderPicker';
      window.mountPicker=()=>{
        document.body.replaceChildren();
        const app=document.createElement('main'), background=document.createElement('section'), host=document.createElement('div');
        const opener=document.createElement('button'), outside=document.createElement('button'), preserved=document.createElement('aside');
        app.id='app'; background.id='background'; opener.id='opener'; opener.textContent='Browse';
        outside.id='outside'; outside.textContent='Background action'; preserved.id='preserved'; preserved.inert=true;
        background.append(opener); app.append(background,host);document.body.append(app,outside,preserved);
        window.closedPicker=0; window.detachedFocusCalls=0;
        window.closePicker=()=>render(null,host);
        opener.focus();
        render(h(FolderPicker,{title:'Choose folder',initial:'',onPick(){},onClose(){window.closedPicker++;render(null,host);}}),host);
      };
    ` }, bundle: true, write: false, format: 'iife', platform: 'browser', jsx: 'automatic', jsxImportSource: 'preact',
    plugins: [{ name: 'folder-service', setup(plugin) {
      plugin.onResolve({ filter: /^\.\/api$/ }, args => args.importer.endsWith('folderPicker.tsx')
        ? { path: 'api', namespace: 'r22' } : undefined);
      plugin.onLoad({ filter: /.*/, namespace: 'r22' }, () => ({ contents: `export async function call(){return {kind:'ok',envelope:{result:{listing:{schema:1,
        breadcrumb:[],entries:[{name:'Home',path:'/home/fixture',kind:'folder',start:'home'}],truncated:false,marks_complete:true}}}};}` }));
    } }],
  });
  scratch = await mkdtemp(join(tmpdir(), 'airk-r22-dom-'));
  browser = spawn(browserPath, ['--headless', '--disable-gpu', '--no-first-run', '--no-default-browser-check',
    '--remote-debugging-port=0', '--remote-debugging-address=127.0.0.1', `--user-data-dir=${scratch}`, 'about:blank'],
    { windowsHide: true, stdio: 'ignore' });
  const portFile = join(scratch, 'DevToolsActivePort');
  const watcher = watch(scratch);
  try {
    await new Promise((resolvePort, reject) => {
      const timeout = setTimeout(() => reject(new Error('Browser debugging endpoint deadline')), 15000);
      const observe = async () => {
        if (await access(portFile).then(() => true, () => false)) {
          clearTimeout(timeout); resolvePort();
        }
      };
      watcher.on('change', observe);
      browser.once('error', error => { clearTimeout(timeout); reject(error); });
      void observe();
    });
  } finally { watcher.close(); }
  const port = (await readFile(portFile, 'utf8')).split('\n')[0];
  const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
  socket = new WebSocket(targets.find(target => target.type === 'page').webSocketDebuggerUrl);
  await once(socket, 'open');
  let sequence = 0;
  const pending = new Map();
  socket.addEventListener('message', event => {
    const reply = JSON.parse(event.data), request = pending.get(reply.id);
    if (!request) return;
    pending.delete(reply.id); clearTimeout(request.timeout);
    if (reply.error) request.reject(new Error(JSON.stringify(reply.error)));
    else request.resolve(reply.result);
  });
  call = (method, params = {}) => new Promise((resolveCall, reject) => {
    const id = ++sequence, timeout = setTimeout(() => { pending.delete(id); reject(new Error(`CDP deadline: ${method}`)); }, 10000);
    pending.set(id, { resolve: resolveCall, reject, timeout }); socket.send(JSON.stringify({ id, method, params }));
  });
  await evaluate(bundle.outputFiles[0].text);
});
beforeEach(async () => {
  if (!available) return;
  await evaluate('window.closePicker?.(); window.mountPicker();');
  await evaluate(`new Promise((resolve,reject)=>{const deadline=performance.now()+5000;const observe=()=>{
    if(document.querySelector('[role="dialog"] button')?.textContent.includes('Home'))return resolve(true);
    if(performance.now()>deadline)return reject(Error('Picker listing deadline'));requestAnimationFrame(observe);};observe();})`);
});
after(async () => {
  if (!available) return;
  try { if (call) await call('Browser.close'); } finally {
    socket?.close();
    if (browser && browser.exitCode === null && browser.signalCode === null) {
      const exited = once(browser, 'exit', { signal: AbortSignal.timeout(10000) });
      if (!call) browser.kill();
      await exited;
    }
    if (scratch && resolve(scratch).startsWith(`${resolve(tmpdir())}${sep}`)) await rm(scratch, { recursive: true, force: true });
  }
});

browserTest('R22 initial keyboard focus enters the Path input', async () => {
  assert.equal(await evaluate(`document.activeElement===document.querySelector('[role="dialog"] input')`), true);
});
browserTest('R22 real Tab and Shift-Tab stay inside the modal', async () => {
  await evaluate(`window.enabledPickerButtons=()=>[...document.querySelectorAll('[role="dialog"] button:not([disabled]),[role="dialog"] input:not([disabled])')];enabledPickerButtons().at(-1).focus()`);
  await key('Tab');
  assert.equal(await evaluate('document.activeElement===enabledPickerButtons()[0]'), true);
  await key('Tab', 8);
  assert.equal(await evaluate('document.activeElement===enabledPickerButtons().at(-1)'), true);
});
browserTest('R22 background branches are inert and restored after Escape', async () => {
  assert.deepEqual(await evaluate(`[document.getElementById('background').inert,document.getElementById('outside').inert,document.getElementById('preserved').inert]`), [true, true, true]);
  assert.equal(await evaluate(`document.getElementById('outside').focus();document.activeElement===document.getElementById('outside')`), false);
  await key('Escape');
  assert.equal(await evaluate('window.closedPicker'), 1);
  assert.equal(await evaluate(`document.activeElement===document.getElementById('opener')`), true);
  assert.deepEqual(await evaluate(`[document.getElementById('background').inert,document.getElementById('outside').inert,document.getElementById('preserved').inert]`), [false, false, true]);
});
browserTest('R22 cleanup never focuses a removed opener', async () => {
  await evaluate(`const opener=document.getElementById('opener');opener.focus=()=>window.detachedFocusCalls++;opener.remove();document.querySelector('[role="dialog"] input').focus();`);
  await key('Escape');
  assert.equal(await evaluate('window.closedPicker'), 1);
  assert.equal(await evaluate('window.detachedFocusCalls'), 0);
});
