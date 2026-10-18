// SPDX-License-Identifier: Apache-2.0
// Real installed webview/dialog acceptance; used only by the private editor harness.
const fs = require('node:fs/promises');
const path = require('node:path');
const assert = require('node:assert/strict');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));

async function until(action, label, timeout = 30000) {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
        const result = await action();
        if (result) { return result; }
        await delay(150);
    }
    throw new Error(`Timed out: ${label}`);
}
async function exists(file) { return fs.access(file).then(() => true, () => false); }
async function connection(target) {
    const socket = new WebSocket(target.webSocketDebuggerUrl), contexts = new Set(), pending = new Map();
    let sequence = 0;
    await new Promise((resolve, reject) => { socket.addEventListener('open', resolve, { once: true }); socket.addEventListener('error', reject, { once: true }); });
    socket.addEventListener('message', event => {
        const value = JSON.parse(event.data);
        if (value.method === 'Runtime.executionContextCreated') { contexts.add(value.params.context.id); }
        if (value.method === 'Runtime.executionContextDestroyed') { contexts.delete(value.params.executionContextId); }
        if (value.method === 'Runtime.executionContextsCleared') { contexts.clear(); }
        const request = pending.get(value.id);
        if (request) { pending.delete(value.id); clearTimeout(request.timer); value.error ? request.reject(new Error(JSON.stringify(value.error))) : request.resolve(value.result); }
    });
    const call = (method, params = {}) => new Promise((resolve, reject) => {
        const id = ++sequence, timer = setTimeout(() => { pending.delete(id); reject(new Error(`CDP timeout: ${method}`)); }, method === 'Runtime.evaluate' ? 1000 : 10000);
        pending.set(id, { resolve, reject, timer }); socket.send(JSON.stringify({ id, method, params }));
    });
    await call('Runtime.enable');
    return { call, contexts, close: () => socket.close(), evaluate: async (contextId, expression) => {
        const response = await call('Runtime.evaluate', { contextId, expression, returnByValue: true });
        if (response.exceptionDetails) { return undefined; }
        return response.result?.value;
    } };
}

exports.checkUsage = async (port, folder) => {
    await until(async () => {
        const error = await fs.readFile(path.join(folder, 'a-error.json'), 'utf8').catch(() => undefined);
        if (error) { throw new Error(error); }
        return exists(path.join(folder, 'a-usage-ready.json'));
    }, 'installed usage driver ready', 120000);
    const { specs } = JSON.parse(await fs.readFile(path.join(folder, 'a-usage-ready.json'), 'utf8'));
    const clients = new Map(), checks = [];
    const document = async (predicate, expression, type = 'iframe') => {
        const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`, { signal: AbortSignal.timeout(10000) })).json();
        for (const target of targets.filter(t => t.type === type && (type === 'page' ? t.url.includes('workbench') : t.url.includes('extensionId=iokaio.airk')))) {
            let client = clients.get(target.id);
            if (!client) { client = await connection(target); clients.set(target.id, client); }
            for (const context of client.contexts) {
                try {
                    if (await client.evaluate(context, predicate)) { return { value: await client.evaluate(context, expression), client }; }
                } catch (error) {
                    // VS Code freezes hidden webviews and replaces contexts on tab switches.
                    if (!/CDP timeout: Runtime.evaluate|Cannot find context|Inspected target navigated or closed/.test(String(error))) { throw error; }
                }
            }
        }
        return undefined;
    };
    const workbench = expression => document('!!document.querySelector(".monaco-workbench")', expression, 'page');
    let selected = specs[0];
    const report = expression => document(`!!document.querySelector('[data-usage=refresh]') && document.body.innerText.includes(${JSON.stringify(selected.collection)})`, expression);
    const reportContains = text => until(async () => (await report(`document.body.innerText.includes(${JSON.stringify(text)})`))?.value, text);
    const click = async action => {
        await until(async () => (await report(`document.querySelector('[data-usage=${action}]').click();true`))?.value, `report ${action}`);
    };
    const input = async (value, title) => {
        const result = await until(async () => {
            const response = await workbench(`(() => {const w=document.querySelector('.quick-input-widget');const i=w?.querySelector('input');
                if(!w || !i || !w.getBoundingClientRect().height || !w.innerText.includes(${JSON.stringify(title)}))return false;i.focus();i.select();return true;})()`);
            return response?.value ? response : undefined;
        }, `input ${title}`);
        await delay(350);
        await workbench("(() => {const i=document.querySelector('.quick-input-widget input');i.focus();i.select();return true;})()");
        await result.client.call('Input.dispatchKeyEvent', { type: 'keyDown', key: 'a', code: 'KeyA', windowsVirtualKeyCode: 65, modifiers: 2 });
        await result.client.call('Input.dispatchKeyEvent', { type: 'keyUp', key: 'a', code: 'KeyA', windowsVirtualKeyCode: 65, modifiers: 2 });
        await result.client.call('Input.insertText', { text: value });
        // File pickers resolve the typed directory asynchronously before accepting it.
        await delay(title ? 100 : 700);
        await result.client.call('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13 });
        await result.client.call('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13 });
    };
    const button = (label, dialog = false) => until(async () => (await workbench(`(() => {
        const nodes=[...document.querySelectorAll(${JSON.stringify(dialog ? '.monaco-dialog-box .monaco-button' : '.quick-input-widget .monaco-list-row')} )];
        const node=nodes.find(n=>n.getBoundingClientRect().height && n.innerText.includes(${JSON.stringify(label)}));if(!node)return false;node.click();return true;
    })()`))?.value, `choose ${label}`);
    const activateReport = async () => {
        const response = await until(async () => {
            const result = await workbench(`(() => {const t=[...document.querySelectorAll('.tab')].find(t=>t.textContent.includes(${JSON.stringify('AIRK Usage & Savings: ' + path.basename(selected.root))}));if(!t)return false;
                const r=t.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};})()`);
            return result?.value ? result : undefined;
        }, 'activate report tab');
        await response.client.call('Input.dispatchMouseEvent', { type: 'mousePressed', ...response.value, button: 'left', clickCount: 1 });
        await response.client.call('Input.dispatchMouseEvent', { type: 'mouseReleased', ...response.value, button: 'left', clickCount: 1 });
    };
    try {
        await reportContains('Recorded AIRK calls');
        await report("document.getElementById('usage-days').value='7';document.getElementById('usage-days').dispatchEvent(new Event('change',{bubbles:true}));true");
        await delay(400);
        await until(async () => (await report("document.getElementById('usage-days').value==='7'"))?.value, '7 day report');
        for (const [action, name] of [['csv', 'report.csv'], ['markdown', 'report.md']]) {
            await click(action);
            await input(path.join(folder, name), '');
            await until(() => exists(path.join(folder, name)), `export ${name}`);
            const content = await fs.readFile(path.join(folder, name), 'utf8');
            assert.ok(content.includes(selected.collection));
            if (action === 'csv') {
                const row = content.split(/\r?\n/).find(line => line.startsWith('report_metadata,'));
                assert.ok(row);
                const metadata = JSON.parse(row.slice(row.indexOf('"') + 1, -1).replace(/""/g, '"'));
                assert.equal(metadata.end_unix_ms_exclusive - metadata.start_unix_ms, 7 * 86400000);
                assert.equal(metadata.scope.collection, selected.collection);
            }
        }
        checks.push('real 7-day selector', 'CSV and Markdown save dialogs and exported scope');
        await click('import');
        await input(path.join(folder, 'measurement.json'), 'Select an AIRK session or paired-task measurement');
        await button('Import measurement', true);
        await activateReport();
        await reportContains('12,000');
        checks.push('real file picker, import preview and confirmation, session counters displayed');
        await click('prices');
        for (const [value, title] of [['fixture-model', 'Cost assumptions'], ['USD', 'Currency'], ['2026-10-18', 'Price date'], ['2', 'Input price'], ['8', 'Output price'], ['0.5', 'Cached input price']]) { await input(value, title); }
        await button('Show session costs only');
        await reportContains('USD 0.02');
        await reportContains('USD 0.001');
        await reportContains('USD 0.008');
        await click('removePrices');
        await reportContains('No cost assumptions selected.');
        checks.push('actual price wizard, input/cache/output equivalents and removal');
        await click('recording'); await button('Turn recording off'); await reportContains('Recording is off.');
        await click('recording'); await button('Record daily totals'); await reportContains('Recording: Daily totals.');
        checks.push('recording off/on through actual quick picks');
        // Open the other collection using its real sidebar button.
        selected = specs[1];
        await until(async () => (await document("!!document.getElementById('dashboard')", `document.getElementById(${JSON.stringify(selected.collection + '-usage')}).click();true`))?.value, 'second collection sidebar report');
        await reportContains(selected.collection);
        assert.equal((await report("document.body.innerText.includes('12,000')"))?.value, false);
        selected = specs[0]; await activateReport(); await reportContains('12,000');
        await click('clear'); await button('Cancel', true); await reportContains('12,000');
        await click('clear'); await button('Clear history', true); await reportContains('No usage recorded yet.');
        checks.push('two collection reports stay separate', 'clear cancellation preserves evidence', 'confirmed clear resets selected report');
        await fs.writeFile(path.join(folder, 'usage-ui-complete.json'), '{}');
        await until(() => exists(path.join(folder, 'a-usage-storage-changed.json')), 'driver storage switch');
        await click('refresh');
        await until(async () => (await workbench("document.body.innerText.includes('This collection or storage selection changed.')"))?.value, 'stale report error');
        checks.push('open report rejects refresh after actual storage switch');
        await fs.writeFile(path.join(folder, 'usage-stale-checked.json'), '{}');
        await fs.writeFile(path.join(folder, 'usage-ui-checks.json'), JSON.stringify(checks, null, 2));
        return checks;
    } catch (error) {
        const state = await workbench("document.body.innerText+'\\nINPUTS: '+JSON.stringify([...document.querySelectorAll('input')].map(i=>({value:i.value,placeholder:i.placeholder,label:i.getAttribute('aria-label')})))");
        await fs.writeFile(path.join(folder, 'usage-ui-failure.txt'), String(error) + '\n' + (state?.value || ''));
        throw error;
    } finally { for (const client of clients.values()) { client.close(); } }
};
