// SPDX-License-Identifier: Apache-2.0
// Render the actual compiled sidebar with fictional data using a local Chromium browser.
const fs = require('node:fs/promises');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { pathToFileURL } = require('node:url');
const { renderSidebar, sidebarControlGroups } = require('../packages/vscode/out/sidebar');

const root = path.resolve(__dirname, '..');
const output = path.join(root, 'docs/ui/images');
const width = 360;
const browserPath = process.env.AIRK_BROWSER || 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe';
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const coverage = { collection: 'sample-collection', root: 'C:\\Projects\\Sample App', generation: 12,
    indexed: true, inventory_revision: 12, scanned_at_unix_ms: 1, scan_complete: true,
    eligible: 820, current: 812, stale: 5, missing: 3, extraction_failed: 0,
    capacity_blocked: 0, unreadable: 0, excluded: 30, unsupported: 2, pending_removals: 0,
    budget_exhausted: false, percent: 99, eligible_source_bytes: 159383552 };
const base = { trusted: true, paused: false, changingStorage: false, mode: 'debounced', groups: sidebarControlGroups(),
    collections: [{ collection: 'sample-collection', name: 'Sample App', root: 'C:\\Projects\\Sample App',
        dataDir: 'D:\\AIRK Indexes', state: 'indexing', coverage: { report: coverage, scanning: false, pending: false },
        detail: { configured_detail: 'balanced', published_detail: 'balanced', pending_reindex: false },
        diskBytes: 756023296 }] };
const paused = structuredClone(base); paused.paused = true; paused.collections[0].state = 'paused';
const first = structuredClone(base); first.collections[0].state = 'queued'; delete first.collections[0].coverage; delete first.collections[0].diskBytes;
first.collections[0].detail = { configured_detail: 'balanced', published_detail: null, pending_reindex: true };
const failed = structuredClone(base); failed.collections[0].state = 'failed';
failed.collections[0].error = 'Corpus exceeds configured byte limit; previous generation retained.';
failed.collections[0].coverage.pending = true;
const multi = structuredClone(base); multi.collections[0].state = 'current';
multi.collections.push({ collection: 'sample-docs', name: 'Sample Docs', root: 'C:\\Projects\\Sample Docs',
    dataDir: 'D:\\AIRK Indexes', state: 'queued', detail: { configured_detail: 'essential', published_detail: 'essential', pending_reindex: false } });
const verified = structuredClone(base); verified.collections[0].state = 'current';
verified.collections[0].coverage.report = { ...coverage, current: coverage.eligible, stale: 0, missing: 0, percent: 100 };
verified.collections[0].mcp = { phase: 'verified', checkedAt: Date.parse('2026-10-17T16:30:00Z'),
    tools: Array.from({ length: 27 }, (_, i) => `fixture_tool_${i}`) };
const checking = structuredClone(verified); checking.collections[0].mcp = { phase: 'checking' };
const mcpFailed = structuredClone(verified); mcpFailed.collections[0].mcp = { phase: 'failed',
    checkedAt: Date.parse('2026-10-17T16:32:00Z'), error: 'The helper did not complete the MCP handshake within the allowed time.' };
const off = structuredClone(verified); off.registrationEnabled = false;
const storageBlocked = structuredClone(base); storageBlocked.collections[0].state = 'capacity-limited';
storageBlocked.collections[0].error = 'Storage quota reached. Automatic indexing is stopped. Review storage limits and retention, reclaim eligible space, then Resume or Index now.';
const detailPending = structuredClone(verified); detailPending.collections[0].detail = { configured_detail: 'balanced', published_detail: 'detailed', pending_reindex: true };
const scenes = [['dashboard-indexing', base], ['dashboard-paused', paused], ['dashboard-first-run', first],
    ['dashboard-error', failed], ['dashboard-multi-root', multi], ['dashboard-mcp-verified', verified],
    ['dashboard-mcp-checking', checking], ['dashboard-mcp-failed', mcpFailed], ['dashboard-mcp-off', off], ['dashboard-storage-blocked', storageBlocked], ['dashboard-detail-pending', detailPending]];

function preview(state) {
    let html = renderSidebar(state);
    // The browser fixture supplies the editor's theme variables and webview bridge.
    html = html.replace(/(<style nonce="[^"]+">)/, `$1
      :root{--vscode-font-family:"Segoe UI",sans-serif;--vscode-font-size:13px;--vscode-foreground:#cccccc;
      --vscode-sideBar-background:#181818;--vscode-descriptionForeground:#a0a0a0;--vscode-panel-border:#343434;
      --vscode-button-background:#0078d4;--vscode-button-foreground:#ffffff;--vscode-button-secondaryBackground:#313131;
      --vscode-button-secondaryForeground:#cccccc;--vscode-dropdown-background:#313131;--vscode-dropdown-foreground:#cccccc;
      --vscode-dropdown-border:#454545;--vscode-focusBorder:#007fd4;--vscode-progressBar-background:#0e70c0;
      --vscode-textBlockQuote-background:#252526;--vscode-errorForeground:#f48771;}
      body{width:${width}px}`);
    html = html.replace('const vscode=acquireVsCodeApi()', `let saved; const acquireVsCodeApi=()=>({getState:()=>saved,setState:value=>{saved=value;},postMessage:value=>{window.lastAction=value;}}); const vscode=acquireVsCodeApi()`);
    return html;
}

async function main() {
    await fs.mkdir(output, { recursive: true });
    const scratch = await fs.mkdtemp(path.join(root, 'artifacts/sidebar-render-'));
    const browser = spawn(browserPath, ['--headless', '--disable-gpu', '--no-first-run', '--no-default-browser-check',
        '--remote-debugging-port=0', '--remote-debugging-address=127.0.0.1', `--user-data-dir=${path.join(scratch, 'profile')}`, 'about:blank'],
        { windowsHide: true, stdio: 'ignore' });
    let socket;
    let startError;
    browser.on('error', error => { startError = error; });
    try {
        let port;
        for (let i = 0; i < 100; i++) {
            if (startError) { throw startError; }
            try { port = (await fs.readFile(path.join(scratch, 'profile/DevToolsActivePort'), 'utf8')).split('\n')[0]; break; }
            catch { await delay(100); }
        }
        if (!port) { throw new Error('Browser did not expose its local rendering endpoint. Set AIRK_BROWSER to Chromium or Edge.'); }
        const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
        socket = new WebSocket(targets.find(target => target.type === 'page').webSocketDebuggerUrl);
        await new Promise((resolve, reject) => { socket.addEventListener('open', resolve, { once: true }); socket.addEventListener('error', reject, { once: true }); });
        let sequence = 0;
        const pending = new Map();
        socket.addEventListener('message', event => {
            const value = JSON.parse(event.data);
            const request = pending.get(value.id);
            if (request) { pending.delete(value.id); clearTimeout(request.timer); value.error ? request.reject(new Error(JSON.stringify(value.error))) : request.resolve(value.result); }
        });
        const call = (method, params = {}) => new Promise((resolve, reject) => {
            const id = ++sequence;
            const timer = setTimeout(() => { pending.delete(id); reject(new Error(`Timed out: ${method}`)); }, 15000);
            pending.set(id, { resolve, reject, timer }); socket.send(JSON.stringify({ id, method, params }));
        });
        await call('Page.enable');
        await call('Emulation.setDeviceMetricsOverride', { width, height: 1000, deviceScaleFactor: 2, mobile: false });
        for (const [name, state] of scenes) {
            const source = path.join(scratch, name + '.html');
            await fs.writeFile(source, preview(state));
            await call('Page.navigate', { url: pathToFileURL(source).href });
            for (let i = 0; i < 100; i++) {
                const ready = await call('Runtime.evaluate', { expression: `location.href === ${JSON.stringify(pathToFileURL(source).href)} && document.readyState === 'complete' && !!document.getElementById('dashboard')` });
                if (ready.result.value) { break; }
                await delay(50);
            }
            await call('Runtime.evaluate', { expression: 'document.fonts.ready', awaitPromise: true });
            const fits = await call('Runtime.evaluate', { expression: 'document.documentElement.scrollWidth <= window.innerWidth' });
            if (!fits.result.value) { throw new Error(`Sidebar overflows horizontally: ${name}`); }
            const size = await call('Runtime.evaluate', { expression: 'Math.ceil(document.documentElement.scrollHeight)' });
            const png = await call('Page.captureScreenshot', { format: 'png', captureBeyondViewport: true,
                clip: { x: 0, y: 0, width, height: size.result.value, scale: 1 } });
            await fs.writeFile(path.join(output, name + '.png'), Buffer.from(png.data, 'base64'));
            // Exercise real DOM events in addition to the controller's mocked integration tests.
            const interaction = await call('Runtime.evaluate', { expression: "document.getElementById('mode').value='manual';document.getElementById('mode').dispatchEvent(new Event('change',{bubbles:true}));JSON.stringify(window.lastAction)" });
            if (interaction.result.value !== '{"type":"mode","value":"manual"}') { throw new Error('Sidebar mode event did not reach the bridge'); }
            const toggle = await call('Runtime.evaluate', { expression: "document.getElementById('registration').checked=false;document.getElementById('registration').dispatchEvent(new Event('change',{bubbles:true}));JSON.stringify(window.lastAction)" });
            if (toggle.result.value !== '{"type":"registration","enabled":false}') { throw new Error('Registration switch did not reach the bridge'); }
            console.log(`${name}.png: ${width} CSS pixels wide, ${size.result.value} high (2x rendering)`);
        }
        await call('Browser.close');
    } finally {
        socket?.close();
        browser.kill();
    }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
