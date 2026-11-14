// SPDX-License-Identifier: Apache-2.0
// Isolated Chromium profile and real DOM events; no product/service state.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { watch } from 'node:fs';
import { access, mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve, sep } from 'node:path';
export const browserPath = process.env.AIRK_BROWSER ?? 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe';
export const browserAvailable = await access(browserPath).then(() => true, () => false);
export async function browserHarness() {
  const directory = await mkdtemp(join(tmpdir(), 'airk-shared-ui-dom-'));
  const browser = spawn(browserPath, ['--headless', '--disable-gpu', '--no-first-run', '--no-default-browser-check',
    '--remote-debugging-port=0', '--remote-debugging-address=127.0.0.1', `--user-data-dir=${directory}`, 'about:blank'],
    { windowsHide: true, stdio: 'ignore' });
  const observation = { pid: browser.pid, profile: directory, executable: browserPath,
    spawned_at_utc: new Date().toISOString(), browser_close_acknowledged: false,
    exit_observed: false, profile_removed: false };
  browser.once('exit', (code, signal) => Object.assign(observation,
    { exit_observed: true, exit_code: code, exit_signal: signal, exited_at_utc: new Date().toISOString() }));
  let socket, call;
  const close = async () => {
    try { if (call) { await call('Browser.close'); observation.browser_close_acknowledged = true; } } finally {
      socket?.close();
      // Browser.close drains its subprocesses. Killing the parent immediately
      // after its CDP reply can leave Windows profile databases locked.
      if (browser.exitCode === null && browser.signalCode === null) {
        const exited=once(browser,'exit',{signal:AbortSignal.timeout(10000)});
        if(!call)browser.kill();
        await exited;
      }
      assert.ok(resolve(directory).startsWith(`${resolve(tmpdir())}${sep}`));
      await rm(directory, { recursive: true, force: true });
      observation.profile_removed = true;
    }
  };
  try {
    const portFile = join(directory, 'DevToolsActivePort'), watcher = watch(directory);
    try {
      await new Promise((ready, reject) => {
        const deadline = setTimeout(() => reject(Error('Browser endpoint deadline')), 15000);
        const observe = async () => { if (await access(portFile).then(() => true, () => false)) { clearTimeout(deadline); ready(); } };
        watcher.on('change', observe); browser.once('error', error => { clearTimeout(deadline); reject(error); }); void observe();
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
      pending.delete(reply.id); clearTimeout(request.deadline);
      if (reply.error) request.reject(Error(JSON.stringify(reply.error))); else request.ready(reply.result);
    });
    call = (method, params = {}) => new Promise((ready, reject) => {
      const id = ++sequence;
      Object.assign(observation, { last_cdp_method: method, last_cdp_id: id, last_cdp_at_utc: new Date().toISOString() });
      const deadline = setTimeout(() => { pending.delete(id); reject(Error(`CDP deadline: ${method}`)); }, 10000);
      pending.set(id, { ready, reject, deadline }); socket.send(JSON.stringify({ id, method, params }));
    });
    const evaluate = async expression => {
      const result = await call('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
      assert.equal(result.exceptionDetails, undefined, result.exceptionDetails?.text);
      return result.result.value;
    };
    const observed = condition => evaluate(`new Promise((resolve,reject)=>{const deadline=performance.now()+5000;const observe=()=>{
      if(${condition})return resolve(true);if(performance.now()>deadline)return reject(Error('DOM observation deadline'));requestAnimationFrame(observe);};observe();})`);
    return { close, call, evaluate, observed, observation };
  } catch (error) { await close(); throw error; }
}
