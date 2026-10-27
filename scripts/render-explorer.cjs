// SPDX-License-Identifier: Apache-2.0
// Render the actual bundled Explorer with fictional bridge observations. No native work.
const fs = require('node:fs/promises');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { pathToFileURL } = require('node:url');
const assert = require('node:assert/strict');
const { FEATURES } = require('../packages/vscode/out/explorerFeatures');
const { CORE_PACK } = require('../packages/vscode/out/explorerStrategies');
const root = path.resolve(__dirname, '..');
const version = require('../packages/vscode/package.json').version;
const output = path.join(root, `artifacts/local/explorer-ui-${version.replaceAll('.', '')}`);
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const project = 'ab'.repeat(32);
const publication = 'airk-r1:fictional-preview:17:0123456789abcdef';
const libraryFiles = {
    plans: { plan: { schema: 'airk.explorer.plan/1', title: 'Preview saved plan', objective: 'Review the cited source.', tasks: [] }, format: 'json', warnings: ['Fictional browser fixture.'] },
    strategies: CORE_PACK[0].strategy,
    recipes: { schema: 'airk.explorer.search-recipe/1', name: 'Preview recipe', query: 'source identity', mode: 'lexical', limit: 20, maxBytes: 16384, fullChunks: false },
};
const projects = Array.from({ length: 8 }, (_, at) => ({ project: at ? at.toString(16).padStart(64, '0') : project,
    name: ['airk', 'bookbuilder', 'design-library'][at % 3] + (at > 2 ? `-${at}` : ''), collection: `sample-${at}`,
    root: `D:\\Projects\\AIRK preview\\${at ? 'supporting-projects' : 'airk'}\\source-${at}`,
    current: at === 0, watching: true, paused: at === 2, indexedFiles: at === 1 ? undefined : 1158 + at, semantic: 'partial', publication, observedAt: Date.now() }));
const state = { revision: 1, extensionVersion: `${version}-preview`, trusted: true, connection: 'ready', selectedProject: project,
    selectionEpoch: 1, features: FEATURES, projects, suggestions: { open: 0, cards: [] },
    status: { indexState: 'available', files: 1158, chunks: 4040, semantic: 'partial', freshness: 'idle', publication, observedAt: Date.now() },
    management: { trusted: true, connected: true, busy: false, authority: 'Local execution host', responseMode: 'terse',
        enabled: { projectActions: true, refresh: true, registerWorkspace: true }, projects: projects.map(summary => ({ summary, rows: [], activity: [], selected: false })),
        inactive: false, morePages: false, pageNote: 'Showing 8 fictional projects. Cached observations.', selectedCount: 0,
        pending: [], events: [], processEvents: [], service: [], runtime: [], resources: [], inference: [] } };
const dark = { foreground: '#cccccc', descriptionForeground: '#a0a0a0', 'editor-background': '#1f1f1f', 'editorWidget-background': '#252526', 'sideBar-background': '#181818',
    'panel-border': '#454545', focusBorder: '#007fd4', 'textLink-foreground': '#4daafc', 'button-background': '#0078d4', 'button-foreground': '#ffffff', 'button-hoverBackground': '#026ec1',
    'input-background': '#313131', 'input-foreground': '#cccccc', 'input-border': '#555555', 'list-hoverBackground': '#2a2d2e', 'list-inactiveSelectionBackground': '#37373d',
    'textBlockQuote-background': '#252526', 'textCodeBlock-background': '#252526', 'editorWarning-foreground': '#cca700', 'testing-iconPassed': '#73c991' };
const light = { ...dark, foreground: '#333333', descriptionForeground: '#616161', 'editor-background': '#ffffff', 'editorWidget-background': '#f3f3f3', 'sideBar-background': '#f3f3f3',
    'panel-border': '#d4d4d4', 'textLink-foreground': '#005fb8', 'input-background': '#ffffff', 'input-foreground': '#333333', 'input-border': '#999999',
    'list-hoverBackground': '#e8e8e8', 'list-inactiveSelectionBackground': '#e4e6f1', 'textBlockQuote-background': '#f2f2f2', 'textCodeBlock-background': '#eeeeee' };
const scenes = [
    ['airk-dark-wide', 1440, 900, 'vscode-dark', true], ['airk-light-wide', 1440, 900, 'vscode-light', true],
    ['vscode-dark-wide', 1440, 900, 'vscode-dark', false], ['vscode-light-wide', 1440, 900, 'vscode-light', false],
    ['airk-dark-split', 820, 780, 'vscode-dark', true], ['vscode-dark-split', 820, 780, 'vscode-dark', false],
    ['airk-dark-narrow', 360, 720, 'vscode-dark', true], ['vscode-light-narrow', 360, 720, 'vscode-light', false],
    ['vscode-short', 900, 480, 'vscode-dark', false], ['high-contrast', 820, 780, 'vscode-high-contrast', true],
];
async function html(scene) {
    const [name, , , theme, themed] = scene;
    const tokens = theme === 'vscode-light' ? light : dark;
    const css = `:root{--vscode-font-family:'Segoe UI',sans-serif;--vscode-font-size:13px;--vscode-editor-font-family:Consolas,monospace;${Object.entries(tokens).map(([key, value]) => `--vscode-${key}:${value};`).join('')}}`;
    const script = pathToFileURL(path.join(root, 'packages/vscode/out/explorerWebview.js')).href;
    const style = pathToFileURL(path.join(root, 'packages/vscode/out/explorerWebview.css')).href;
    return `<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><style>${css}</style><link rel="stylesheet" href="${style}"></head>
<body class="${theme}"><div id="root"></div><script>
const initial=${JSON.stringify(state)}, key=${JSON.stringify(name)}, defaultState=${JSON.stringify({ route: '#manage/projects', themed })};
let saved=JSON.parse(localStorage.getItem(key)||'null')||defaultState;window.bridgeMessages=[];let recipes=[];const libraryFiles=${JSON.stringify(libraryFiles)};
window.send=(message)=>window.dispatchEvent(new MessageEvent('message',{data:message}));
window.acquireVsCodeApi=()=>({getState:()=>saved,setState:value=>{saved=value;localStorage.setItem(key,JSON.stringify(value));},postMessage:message=>{
window.bridgeMessages.push(message);if(message.type==='ready'){setTimeout(()=>window.send({v:1,type:'state',state:initial}),0);}
else if(!message.requestId.startsWith('n')){
let data=message.type==='manage.action'?{message:null}:null;
if(message.type==='files.list'){data={kind:message.kind,workspace:'file:///preview',roots:[{id:'file:///preview',name:'Preview'}],base:'.airk/'+message.kind,directory:message.directory,missing:false,
entries:message.directory?[{name:'saved.json',path:'releases/saved.json',directory:false}]:[{name:'releases',path:'releases',directory:true},{name:'top.json',path:'top.json',directory:false}]};}
if(message.type==='files.load'){data=libraryFiles[message.kind];}
if(message.type==='recipes.list'){data=recipes;}
if(message.type==='recipes.save'){recipes=[message.recipe];data=recipes;}
if(message.type==='help.choose'){data=window.helpChoice||'screen';}
if(message.type==='source.choose'){initial.sourceFile='src/index.ts';setTimeout(()=>window.send({v:1,type:'state',state:initial}),0);}
if(message.type==='docs.facts'){data={tool:'airk_document_facts',items:[{ref:'h-preview',path:'docs/architecture.md',startLine:12,endLine:12,evidenceClass:'E1',kind:'architecture',name:'Source files remain authoritative. The index retains bounded evidence and original locations; retrieval does not execute these instructions.'}],returned:1,limit:50,truncated:false,notes:[],provenance:[],publication:initial.status.publication};}
if(message.type==='evidence.inspect'){data={ref:message.ref||'h-source',path:message.ref?'docs/architecture.md':initial.sourceFile,startLine:message.ref?12:1,endLine:message.ref?12:1,contentHash:'ab'.repeat(32),live:'matches_index',liveMessage:'Fictional saved-byte comparison.',publication:initial.status.publication};}
if(message.type==='review.tests'){data={candidates:[{path:'tests/contract.test.ts',mentions:'',text:'',reason:'changed_test_file'},{path:'tests/contract.test.ts',line:7,mentions:'ChangedContract',text:'assert.equal(ChangedContract(1), 2)',reason:'identifier_reference'}],declarations:1,pattern:'tests/',truncated:false,notes:['Fictional browser observations; tests were not executed.']};}
setTimeout(()=>window.send({v:1,type:'result',requestId:message.requestId,ok:true,data}),0);}}});
</script><script src="${script}"></script></body></html>`;
}
async function main() {
    await fs.mkdir(output, { recursive: true });
    const scratch = await fs.mkdtemp(path.join(output, 'browser-'));
    const browser = spawn(process.env.AIRK_BROWSER || 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
        ['--headless', '--disable-gpu', '--no-first-run', '--no-default-browser-check', '--remote-debugging-port=0', `--user-data-dir=${path.join(scratch, 'profile')}`, 'about:blank'],
        { windowsHide: true, stdio: 'ignore' });
    let startupError, socket;
    browser.on('error', error => { startupError = error; });
    const pending = new Map(); let sequence = 0;
    try {
        let port;
        for (let at = 0; at < 100; at++) {
            if (startupError) { throw startupError; }
            try { port = (await fs.readFile(path.join(scratch, 'profile/DevToolsActivePort'), 'utf8')).split('\n')[0]; break; } catch { await delay(100); }
        }
        if (!port) { throw new Error('Browser did not start. Set AIRK_BROWSER to a local Chromium executable.'); }
        const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
        socket = new WebSocket(targets.find(target => target.type === 'page').webSocketDebuggerUrl);
        await new Promise((resolve, reject) => { socket.addEventListener('open', resolve, { once: true }); socket.addEventListener('error', reject, { once: true }); });
        socket.addEventListener('message', event => { const value = JSON.parse(event.data), request = pending.get(value.id);
            if (request) { pending.delete(value.id); clearTimeout(request.timer); value.error ? request.reject(new Error(JSON.stringify(value.error))) : request.resolve(value.result); } });
        const call = (method, params = {}) => new Promise((resolve, reject) => { const id = ++sequence;
            const timer = setTimeout(() => { pending.delete(id); reject(new Error(`CDP timeout: ${method}`)); }, 15000);
            pending.set(id, { resolve, reject, timer }); socket.send(JSON.stringify({ id, method, params })); });
        const evaluate = async expression => { const value = await call('Runtime.evaluate', { expression, returnByValue: true });
            if (value.exceptionDetails) { throw new Error(JSON.stringify(value.exceptionDetails)); } return value.result.value; };
        const until = async expression => { for (let at = 0; at < 100; at++) { if (await evaluate(expression)) { return; } await delay(50); } throw new Error(`UI did not become ready: ${expression}`); };
        const screenshot = async name => { const png = await call('Page.captureScreenshot', { format: 'png', captureBeyondViewport: false }); await fs.writeFile(path.join(output, `${name}.png`), Buffer.from(png.data, 'base64')); };
        const checks = [];
        await call('Page.enable');
        for (const scene of scenes) {
            const [name, width, height, theme, themed] = scene;
            const source = path.join(scratch, `${name}.html`); await fs.writeFile(source, await html(scene));
            await call('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false });
            await call('Page.navigate', { url: pathToFileURL(source).href });
            await until(`document.querySelectorAll('.project-card').length===8`);
            const fits = await evaluate(`(()=>{const body=document.documentElement,content=document.querySelector('.content'),management=document.querySelector('.rail button[aria-label="Management"]');
                const before=management.getBoundingClientRect();content.scrollTop=content.scrollHeight;
                const after=management.getBoundingClientRect();return {page:body.scrollWidth<=innerWidth&&body.scrollHeight<=innerHeight,
                content:content.scrollWidth<=content.clientWidth+1,nav:after.top>=0&&after.bottom<=innerHeight&&Math.abs(before.top-after.top)<1,
                path:document.querySelector('.project-path').getBoundingClientRect().width>200};})()`);
            assert.deepEqual(fits, { page: true, content: true, nav: true, path: true }, name);
            await evaluate(`document.querySelector('.content').scrollTop=0`);
            await screenshot(name);
            await evaluate(`document.querySelector('.source-context button').click()`);
            await until(`document.querySelector('.source-context code').textContent==='src/index.ts'`);
            await evaluate(`[...document.querySelectorAll('.topbar-actions button')].find(button=>button.textContent.startsWith('Help')).click()`);
            await until(`document.querySelector('.help-topic')?.textContent.includes('Registered projects')`);
            assert.equal(await evaluate(`window.bridgeMessages.find(m=>m.type==='help.choose').route`), 'manage/projects');
            await evaluate(`window.helpChoice='general';[...document.querySelectorAll('.topbar-actions button')].find(button=>button.textContent.startsWith('Help')).click()`);
            await until(`document.querySelector('.help-topic')?.textContent.includes('Help')`);
            await evaluate(`window.send({v:1,type:'navigate',route:'#manage/projects'})`); await until(`document.querySelectorAll('.project-card').length===8`);
            // Actual click, save-state and reload; the presentation toggle never reaches native work.
            await evaluate(`document.querySelector('.theme-toggle').click()`);
            await until(`document.querySelector('.theme-toggle').getAttribute('aria-pressed')==='${!themed}'`);
            await call('Page.reload'); await until(`document.querySelectorAll('.project-card').length===8`);
            assert.equal(await evaluate(`document.querySelector('.theme-toggle').getAttribute('aria-pressed')`), String(!themed), `${name} persists theme`);
            if (theme === 'vscode-high-contrast') {
                await evaluate(`document.querySelector('.theme-toggle').click()`); await delay(30);
                assert.equal(await evaluate(`getComputedStyle(document.querySelector('.project-card')).backgroundColor`), 'rgb(37, 37, 38)', 'high contrast retains editor surface');
            }
            await evaluate(`document.querySelector('.project-footer summary').click()`);
            assert.equal(await evaluate(`document.querySelector('.project-footer details').open`), true);
            await evaluate(`document.querySelector('.project-card .actions button').click()`); await delay(30);
            assert.equal(await evaluate(`window.bridgeMessages.some(m=>m.type==='manage.action'&&m.action==='index'&&m.project===${JSON.stringify(project)})`), true, 'Index keeps exact project identity');
            const tabs = await evaluate(`(()=>{const tabs=document.querySelector('.tabs');tabs.scrollLeft=tabs.scrollWidth;const last=tabs.lastElementChild.getBoundingClientRect(),box=tabs.getBoundingClientRect();return last.right<=box.right+1;})()`);
            assert.equal(tabs, true, `${name} last tab remains reachable`);
            await evaluate(`document.querySelector('.rail button[aria-label="Overview"]').click()`); await until(`!!document.querySelector('.hero-block')`);
            if (name === 'airk-dark-wide') { await screenshot('overview-after-toggle'); }
            await evaluate(`document.querySelector('.rail button[aria-label="Help"]')?.click();window.send({v:1,type:'navigate',route:'#help?focus=role-developer'})`);
            await until(`!!document.querySelector('.help-topic')`);
            if (name === 'airk-light-wide') { await screenshot('help-after-toggle'); }
            for (const [kind, route] of [['plans', '#strategies/planner'], ['strategies', '#strategies/studio'], ['recipes', '#search/recipes']]) {
                await evaluate(`window.send({v:1,type:'navigate',route:${JSON.stringify(route)}})`);
                await until(`!!document.querySelector('.file-list .folder button')`);
                await evaluate(`document.querySelector('.file-list .folder button').click()`);
                await until(`document.querySelector('.file-breadcrumbs').textContent.includes('releases')`);
                if (kind === 'plans' && name === 'airk-dark-wide') { await screenshot('planner-file-library'); }
                await evaluate(`document.querySelector('.file-list .file .file-name').dispatchEvent(new MouseEvent('dblclick',{bubbles:true}))`);
                await until(`window.bridgeMessages.some(m=>m.type==='files.load'&&m.kind===${JSON.stringify(kind)}&&m.path==='releases/saved.json')`);
                if (kind === 'plans') { await until(`!![...document.querySelectorAll('input')].find(input=>input.value==='Preview saved plan')`); }
                if (kind === 'strategies') { await until(`!!document.querySelector('.studio-head')`); }
                if (kind === 'recipes') {
                    await until(`!!document.querySelector('[role="alertdialog"]')`);
                    assert.equal(await evaluate(`window.bridgeMessages.some(m=>m.type==='search.run')`), false, 'loading a recipe never searches');
                    await evaluate(`[...document.querySelectorAll('[role="alertdialog"] button')].find(button=>button.textContent==='Add search').click()`);
                    await until(`window.bridgeMessages.some(m=>m.type==='recipes.save'&&m.expected==='null')`);
                    if (name === 'vscode-dark-split') { await screenshot('recipes-file-library'); }
                }
                const fitsLibrary = await evaluate(`document.querySelector('.content').scrollWidth<=document.querySelector('.content').clientWidth+1`);
                if (!fitsLibrary) {
                    await screenshot(`${name}-${kind}-overflow`);
                    console.log(await evaluate(`(()=>{const area=document.querySelector('.content').getBoundingClientRect();return [...document.querySelectorAll('.content *')].filter(e=>e.getBoundingClientRect().right>area.right+1).slice(0,12).map(e=>({tag:e.tagName,class:e.className,right:e.getBoundingClientRect().right,html:e.outerHTML.slice(0,220)}));})()`));
                }
                assert.equal(fitsLibrary, true, `${name} ${kind} layout`);
            }
            await evaluate(`document.querySelector('.rail button[aria-label="Documents"]').click()`);
            await until(`!!document.querySelector('.search-form')`);
            await evaluate(`document.querySelector('.search-form').dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}))`);
            await until(`!!document.querySelector('.source-text')`);
            assert.deepEqual(await evaluate(`(()=>{const e=document.querySelector('.source-text'),s=getComputedStyle(e);return {weight:s.fontWeight,border:s.borderTopWidth,strong:!!e.querySelector('strong')};})()`), { weight: '400', border: '1px', strong: false }, `${name} plain source results`);
            if (name === 'airk-dark-wide') { await screenshot('source-results-outline'); }
            await evaluate(`document.querySelector('.source-context button').click()`);
            await until(`document.querySelector('.source-context code').textContent==='src/index.ts'`);
            await evaluate(`[...document.querySelectorAll('.fact-list button')].find(button=>button.textContent==='Inspect').click()`);
            await until(`document.querySelector('.hit-head .path')?.textContent==='docs/architecture.md:12'`);
            assert.equal(await evaluate(`document.body.textContent.includes('Inspecting a citation selected from a result.')`), true);
            await evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Inspect selected file').click()`);
            await until(`document.querySelector('.hit-head .path')?.textContent==='src/index.ts:1'`);
            assert.equal(await evaluate(`window.bridgeMessages.filter(m=>m.type==='evidence.inspect').at(-1).ref===undefined`), true);
            if (name === 'airk-dark-wide') { await screenshot('inspector-guidance'); }
            await evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Search for evidence').click()`);
            await until(`!!document.querySelector('.search input[placeholder="Words that must all appear"]')`);
            assert.equal(await evaluate(`document.body.textContent.includes('The source file selected in the header does not limit this search.')`), true);
            await evaluate(`document.querySelector('.rail button[aria-label="Evidence"]').click()`);
            await until(`document.querySelector('.hit-head .path')?.textContent==='docs/architecture.md:12'`);
            await evaluate(`window.send({v:1,type:'state',state:{...initial,selectionEpoch:2}})`);
            await until(`document.querySelector('.hit-head .path')?.textContent==='src/index.ts:1'`);
            await evaluate(`window.send({v:1,type:'navigate',route:'#review/tests'})`);
            await until(`document.querySelectorAll('.review-screen .hit').length===2`);
            assert.equal(await evaluate(`document.querySelector('.review-screen .hit .path').textContent`), 'tests/contract.test.ts', 'Git-only test candidate has no fabricated line');
            if (name === 'airk-dark-wide') { await screenshot('review-test-candidates'); }
            checks.push({ name, width, height, theme, themed, fits, interaction: 'theme persists; project action bound; navigation fixed; templates, plans and recipes browse nested folders and double-click loads; recipe import never searches' });
            console.log(`${name}: layout and interactions passed`);
        }
        await fs.writeFile(path.join(output, 'summary.json'), JSON.stringify({ basis: 'actual bundle with fictional host bridge; no installed-editor/native qualification', checks }, null, 2) + '\n');
        await call('Browser.close');
    } finally {
        for (const request of pending.values()) { clearTimeout(request.timer); }
        socket?.close(); browser.kill();
    }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
