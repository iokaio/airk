// SPDX-License-Identifier: Apache-2.0
// Installed VSIX qualification using private VS Code profiles and fictional sources.
const fs = require('node:fs/promises');
const path = require('node:path');
const net = require('node:net');
const { spawn } = require('node:child_process');
const { createHash } = require('node:crypto');
const root = path.resolve(__dirname, '..');
const [editor, cli, vsix] = process.argv.slice(2).map(value => path.resolve(value));
if (!vsix) { throw new Error('Expected Code.exe, its resources/app/out/cli.js and a verified VSIX.'); }
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const env = { ...process.env, AIRK_USAGE_MODE: 'disabled', RUST_BACKTRACE: '1' };
delete env.ELECTRON_RUN_AS_NODE;
delete env.VSCODE_IPC_HOOK_CLI;
const active = new Set();
const scenarios = (process.env.AIRK_EDITOR_SCENARIOS || 'same,different,multi,usage,untrusted').split(',');
if (scenarios.some(value => !['same', 'different', 'multi', 'usage', 'untrusted'].includes(value))) { throw new Error('Unknown editor scenario'); }

async function runProcess(args, logfile, extraEnv = {}) {
    const log = await fs.open(logfile, 'w');
    return new Promise((resolve, reject) => {
        const child = spawn(editor, args, { windowsHide: true, env: { ...env, ...extraEnv }, stdio: ['ignore', log.fd, log.fd] });
        active.add(child);
        const timer = setTimeout(() => { child.kill(); reject(new Error('Editor process timed out')); }, 240000);
        child.on('error', error => { active.delete(child); clearTimeout(timer); void log.close(); reject(error); });
        child.on('exit', code => { active.delete(child); clearTimeout(timer); void log.close(); code === 0 ? resolve() : reject(new Error(`Editor exited ${code}; inspect ${logfile}`)); });
    });
}

async function port() {
    const server = net.createServer();
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    const value = server.address().port;
    await new Promise(resolve => server.close(resolve));
    return value;
}

async function capture(port, output) {
    const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`, { signal: AbortSignal.timeout(10000) })).json();
    const sidebar = targets.find(value => value.type === 'iframe' && value.url.includes('extensionId=iokaio.airk'));
    if (!sidebar) { throw new Error('Installed AIRK dashboard webview missing'); }
    const uiChecks = await checkSidebar(sidebar);
    const target = targets.find(value => value.type === 'page' && value.url.includes('workbench'));
    if (!target) { throw new Error('Editor workbench CDP target missing'); }
    const socket = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise((resolve, reject) => { socket.addEventListener('open', resolve, { once: true }); socket.addEventListener('error', reject, { once: true }); });
    let seq = 0;
    const pending = new Map();
    socket.addEventListener('message', event => {
        const value = JSON.parse(event.data), request = pending.get(value.id);
        if (request) { pending.delete(value.id); clearTimeout(request.timer); value.error ? request.reject(new Error(JSON.stringify(value.error))) : request.resolve(value.result); }
    });
    const call = (method, params = {}) => new Promise((resolve, reject) => {
        const id = ++seq, timer = setTimeout(() => { pending.delete(id); reject(new Error(`CDP timeout: ${method}`)); }, 15000);
        pending.set(id, { resolve, reject, timer }); socket.send(JSON.stringify({ id, method, params }));
    });
    try {
        await call('Page.enable');
        const text = await call('Runtime.evaluate', { expression: 'document.body.innerText' });
        if (!text.result.value.includes('AIRK')) { throw new Error('AIRK workbench view not visible'); }
        await fs.writeFile(output + '.txt', text.result.value);
        const screenshot = await call('Page.captureScreenshot', { format: 'png' });
        await fs.writeFile(output + '.png', Buffer.from(screenshot.data, 'base64'));
        await fs.writeFile(output + '-targets.json', JSON.stringify(targets.map(({ type, url, title }) => ({ type, url, title })), null, 2));
        return uiChecks;
    } finally { socket.close(); }
}

async function checkSidebar(target) {
    const socket = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise((resolve, reject) => { socket.addEventListener('open', resolve, { once: true }); socket.addEventListener('error', reject, { once: true }); });
    let seq = 0;
    const pending = new Map(), contexts = new Set();
    socket.addEventListener('message', event => {
        const value = JSON.parse(event.data);
        if (value.method === 'Runtime.executionContextCreated') { contexts.add(value.params.context.id); }
        if (value.method === 'Runtime.executionContextDestroyed') { contexts.delete(value.params.executionContextId); }
        const request = pending.get(value.id);
        if (request) { pending.delete(value.id); clearTimeout(request.timer); value.error ? request.reject(new Error(JSON.stringify(value.error))) : request.resolve(value.result); }
    });
    const call = (method, params = {}) => new Promise((resolve, reject) => {
        const id = ++seq, timer = setTimeout(() => { pending.delete(id); reject(new Error(`Sidebar CDP timeout: ${method}`)); }, 10000);
        pending.set(id, { resolve, reject, timer }); socket.send(JSON.stringify({ id, method, params }));
    });
    try {
        await call('Runtime.enable');
        let contextId;
        const contentDeadline = Date.now() + 15000;
        while (!contextId && Date.now() < contentDeadline) {
            for (const candidate of contexts) {
                try {
                    const result = await call('Runtime.evaluate', { contextId: candidate, expression: "!!document.getElementById('dashboard')" });
                    if (result.result?.value) { contextId = candidate; break; }
                } catch (error) {
                    // A newly opened webview can replace its bootstrap context.
                    if (!String(error).includes('Cannot find context')) { throw error; }
                }
            }
            if (!contextId) { await delay(100); }
        }
        if (!contextId) { throw new Error('AIRK sidebar content context missing'); }
        const evaluate = async expression => {
            const result = await call('Runtime.evaluate', { contextId, expression });
            if (result.exceptionDetails) { throw new Error(JSON.stringify(result.exceptionDetails)); }
            return result.result.value;
        };
        const until = async expression => {
            const deadline = Date.now() + 30000;
            while (Date.now() < deadline) { if (await evaluate(expression)) { return; } await delay(100); }
            throw new Error(`Sidebar did not reach expected state: ${expression}`);
        };
        const disabled = await evaluate("document.getElementById('registration').disabled");
        if (disabled) {
            if (!await evaluate("document.body.innerText.includes('Trust this workspace')")) { throw new Error('Untrusted notice missing'); }
            return ['real dashboard DOM', 'untrusted controls disabled'];
        }
        await evaluate("document.getElementById('registration').click()");
        await until("document.body.innerText.includes('VS Code registration: Off')");
        await evaluate("document.getElementById('registration').click()");
        await until("document.body.innerText.includes('VS Code registration: On')");
        await until("!document.querySelector('[data-action=verify]').disabled");
        await evaluate("document.querySelector('[data-action=verify]').click()");
        await until("document.body.innerText.includes('Verified · 16 tools available')");
        await evaluate("document.querySelector('[data-action=verify]').scrollIntoView({block:'center'})");
        return ['real dashboard DOM', 'registration switch clicked off/on', 'Verify connection button completed real handshake'];
    } finally { socket.close(); }
}

async function main() {
    const base = path.join(root, 'artifacts/editor-qualification');
    await fs.mkdir(base, { recursive: true });
    const run = await fs.mkdtemp(path.join(base, 'installed-'));
    console.log(`Qualification output: ${run}`);
    const extensions = path.join(run, 'extensions');
    await runProcess([cli, '--user-data-dir', path.join(run, 'install-profile'), '--extensions-dir', extensions,
        '--shared-data-dir', path.join(run, 'shared-editor-data'),
        '--install-extension', vsix, '--force'], path.join(run, 'install.log'), { ELECTRON_RUN_AS_NODE: '1' });
    // Install the tiny driver in this private directory only. Ordinary windows
    // retain real workspace-trust behavior, unlike a development test host.
    const driver = path.join(run, 'driver');
    await fs.mkdir(driver);
    await fs.writeFile(path.join(driver, 'package.json'), JSON.stringify({ name: 'airk-local-test-driver', publisher: 'local-test', version: '0.0.1',
        description: 'Private AIRK installed-editor qualification driver', license: 'Apache-2.0',
        files: ['driver.js', 'README.md', 'LICENSE'],
        repository: { type: 'git', url: 'https://github.com/iokaio/airk.git' },
        engines: { vscode: '^1.140.0' }, main: './driver.js', activationEvents: ['onStartupFinished'],
        capabilities: { untrustedWorkspaces: { supported: true } } }));
    await fs.writeFile(path.join(driver, 'driver.js'), `const vscode=require('vscode'),fs=require('node:fs/promises'),path=require('node:path');
exports.activate=()=>{if(!process.env.AIRK_EDITOR_RUN)return;void require(${JSON.stringify(path.join(root, 'packages/vscode/out/test/installedEditor.js'))}).run()
.catch(error=>fs.writeFile(path.join(process.env.AIRK_EDITOR_RUN,process.env.AIRK_EDITOR_SCENARIO,process.env.AIRK_EDITOR_ROLE+'-error.json'),JSON.stringify({error:String(error),stack:error.stack})))
.finally(()=>vscode.commands.executeCommand('workbench.action.quit'));};\n`);
    await fs.copyFile(path.join(root, 'LICENSE'), path.join(driver, 'LICENSE'));
    await fs.writeFile(path.join(driver, 'README.md'), 'Private local test driver. Not for publication.\n');
    const driverVsix = path.join(run, 'driver.vsix');
    await require(path.join(root, 'packages/vscode/node_modules/@vscode/vsce')).createVSIX({ cwd: driver,
        packagePath: driverVsix, dependencies: false, followSymlinks: false, rewriteRelativeLinks: false });
    await runProcess([cli, '--user-data-dir', path.join(run, 'install-profile'), '--extensions-dir', extensions,
        '--shared-data-dir', path.join(run, 'shared-editor-data'), '--install-extension', driverVsix, '--force'],
        path.join(run, 'install-driver.log'), { ELECTRON_RUN_AS_NODE: '1' });
    const summary = { vsix_sha256: createHash('sha256').update(await fs.readFile(vsix)).digest('hex'), scenarios: [] };
    for (const scenario of scenarios) {
        const folder = path.join(run, scenario); await fs.mkdir(folder);
        const storage = path.join(folder, 'storage'); await fs.mkdir(storage);
        for (const [name, symbol] of [['workspace-a', 'WindowAlphaEvidence'], ['workspace-b', 'WindowBetaEvidence']]) {
            const workspace = path.join(folder, name); await fs.mkdir(workspace);
            await fs.writeFile(path.join(workspace, 'fixture.ts'), `export function ${symbol}() { return 42; }\n`);
        }
        const roles = ['same', 'different'].includes(scenario) ? ['a', 'b'] : ['a'];
        const children = [], ports = {}, uiChecks = {};
        for (const role of roles) {
            const profile = path.join(folder, `profile-${role}`); await fs.mkdir(path.join(profile, 'User'), { recursive: true });
            await fs.writeFile(path.join(profile, 'User/settings.json'), JSON.stringify({ 'airk.index.mode': 'manual', 'airk.storage.path': storage,
                'workbench.startupEditor': 'none', 'window.restoreWindows': 'none', 'security.workspace.trust.startupPrompt': 'never',
                'extensions.autoUpdate': false, 'extensions.autoCheckUpdates': false, 'telemetry.telemetryLevel': 'off', 'update.mode': 'none',
                'files.simpleDialog.enable': true, 'window.dialogStyle': 'custom' }));
            let workspace = path.join(folder, scenario === 'different' && role === 'b' ? 'workspace-b' : 'workspace-a');
            if (scenario === 'multi' || scenario === 'usage') {
                workspace = path.join(folder, 'two-roots.code-workspace');
                await fs.writeFile(workspace, JSON.stringify({ folders: [{ path: './workspace-a' }, { path: './workspace-b' }] }));
            }
            ports[role] = await port();
            const args = [workspace, '--new-window', '--user-data-dir', profile, '--extensions-dir', extensions,
                '--shared-data-dir', path.join(run, 'shared-editor-data'),
                `--remote-debugging-port=${ports[role]}`, '--remote-debugging-address=127.0.0.1',
                '--skip-welcome', '--skip-release-notes', '--disable-updates', '--disable-telemetry', '--disable-gpu'];
            if (scenario !== 'untrusted') { args.push('--disable-workspace-trust'); }
            children.push(runProcess(args, path.join(folder, `${role}-editor.log`), { AIRK_EDITOR_RUN: run, AIRK_EDITOR_ROLE: role, AIRK_EDITOR_SCENARIO: scenario,
                AIRK_USAGE_MODE: scenario === 'usage' ? 'aggregates-only' : 'disabled' }));
        }
        // Observe process failure immediately while waiting for extension-host results.
        let failure;
        const exited = Promise.all(children).catch(error => { failure = error; });
        try {
            if (scenario === 'usage') { uiChecks.a = await require('./check-usage-editor.cjs').checkUsage(ports.a, folder); }
            const end = Date.now() + 180000;
            while (true) {
                if (failure) { throw failure; }
                for (const role of roles) {
                    const error = await fs.readFile(path.join(folder, `${role}-error.json`), 'utf8').catch(() => undefined);
                    if (error) { throw new Error(error); }
                }
                const ready = await Promise.all(roles.map(role => fs.access(path.join(folder, `${role}-capture.json`)).then(() => true, () => false)));
                if (ready.every(Boolean)) { break; }
                if (Date.now() > end) { throw new Error(`Scenario timed out: ${scenario}`); }
                await delay(250);
            }
            for (const role of roles) { uiChecks[role] = [...(uiChecks[role] || []), ...await capture(ports[role], path.join(folder, `${role}-workbench`))]; }
        } finally { await fs.writeFile(path.join(folder, 'capture-complete.json'), '{}'); }
        await exited;
        if (failure) { throw failure; }
        const results = await Promise.all(roles.map(async role => ({ ...JSON.parse(await fs.readFile(path.join(folder, `${role}-result.json`), 'utf8')), uiChecks: uiChecks[role] })));
        if (roles.length === 2 && results[0].extensionHostPid === results[1].extensionHostPid) { throw new Error('Expected two concurrent extension hosts'); }
        summary.scenarios.push(...results);
        await fs.writeFile(path.join(run, 'summary.json'), JSON.stringify(summary, null, 2));
        console.log(`${scenario}: passed (${roles.length} installed editor window(s))`);
    }
    console.log(`Installed-editor qualification passed: ${run}`);
}
main().catch(error => { for (const child of active) { child.kill(); } console.error(error); process.exitCode = 1; });
