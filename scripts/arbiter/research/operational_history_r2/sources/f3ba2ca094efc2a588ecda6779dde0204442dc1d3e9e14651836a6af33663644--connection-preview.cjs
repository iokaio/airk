// SPDX-License-Identifier: Apache-2.0
// Render the actual compiled sidebar with fictional data using a local Chromium browser.
const fs = require('node:fs/promises');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { pathToFileURL } = require('node:url');
const { renderConnectionGuide } = require('../packages/vscode/out/agentConnections');

const root = path.resolve(__dirname, '..');
const output = path.join(root, 'artifacts/connection-preview', process.env.AIRK_PREVIEW_WIDTH || '900');
const width = Number(process.env.AIRK_PREVIEW_WIDTH || '900');
const browserPath = process.env.AIRK_BROWSER || 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe';
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const spec = { command: 'C:/AIRK/airk.exe', root: 'C:/Projects/Sample', dataDir: 'D:/Indexes', collection: 'project-sample', name: 'Sample' };
const scenes = [['codex', 'codex'], ['copilot', 'vscode'], ['claude', 'claude'], ['gemini', 'gemini'], ['antigravity', 'antigravity']];
function preview(state) {
    let html = renderConnectionGuide(state, spec, state === 'codex' ? 'Codex uses its own project or user configuration.' : state === 'vscode' ? 'Enable the registered AIRK collection in VS Code.' : 'Install project configuration, then reload the selected agent and complete its trust prompts.');
    // The browser fixture supplies the editor's theme variables and webview bridge.
    html = html.replace(/(<style nonce="[^"]+">)/, `$1
      :root{--vscode-font-family:"Segoe UI",sans-serif;--vscode-font-size:13px;--vscode-foreground:#cccccc;
      --vscode-editor-background:#181818;--vscode-textCodeBlock-background:#252526;--vscode-sideBar-background:#181818;--vscode-descriptionForeground:#a0a0a0;--vscode-panel-border:#343434;
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
                const ready = await call('Runtime.evaluate', { expression: `location.href === ${JSON.stringify(pathToFileURL(source).href)} && document.readyState === 'complete' && !!document.querySelector('h1')` });
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
            const action = state === 'codex' ? 'createCodex' : state === 'vscode' ? 'enableCopilot' : 'installJson';
            const interaction = await call('Runtime.evaluate', { expression: `document.querySelector('[data-action="${action}"]').click();JSON.stringify(window.lastAction)` });
            if (interaction.result.value !== JSON.stringify({ action })) { throw new Error('Guide action did not reach the bridge'); }
            const busy = await call('Runtime.evaluate', { expression: 'Array.from(document.querySelectorAll("button")).every(button => button.disabled)' });
            if (!busy.result.value) { throw new Error('Guide did not prevent repeated actions'); }
            console.log(`${name}.png: ${width} CSS pixels wide, ${size.result.value} high (2x rendering)`);
        }
        await call('Browser.close');
    } finally {
        socket?.close();
        browser.kill();
    }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
