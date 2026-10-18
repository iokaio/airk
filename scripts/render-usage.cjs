// SPDX-License-Identifier: Apache-2.0
// Render the shipped webview markup with fictional, content-free measurements.
const fs = require('node:fs/promises');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { pathToFileURL } = require('node:url');
const { renderReport } = require('../packages/vscode/out/report');
const { renderSidebar, sidebarControlGroups } = require('../packages/vscode/out/sidebar');
process.env.TZ = 'UTC';

const root = path.resolve(__dirname, '..');
const output = path.join(root, 'docs/ui/images');
const browserPath = process.env.AIRK_BROWSER || 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe';
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const start = Date.parse('2026-09-19T00:00:00Z');
const end = Date.parse('2026-10-19T00:00:00Z');
const demoNote = 'DEMO — Fictional measurements and illustrative prices, not actual AIRK savings. AIRK measures its own activity; estimates are not a record of your agent’s bill.';
const comparison = (calls, baseline, returned) => ({ full_returned_sources: {
    compared_calls: calls, baseline_utf8_bytes: String(baseline), returned_utf8_bytes: String(returned),
    token_estimate: { nominal_reduction: String((baseline - returned) / 4),
        assumption_low: String(Math.floor(baseline / 6 - returned / 2)),
        assumption_high: String(Math.ceil(baseline / 2 - returned / 6)) }
} });
const session = (input, cached, outputTokens, elapsed, rounds) => ({ sessions: 4,
    input_tokens: String(input), input_measured_sessions: 4,
    cached_input_tokens: String(cached), cached_input_measured_sessions: 4,
    output_tokens: String(outputTokens), output_measured_sessions: 4,
    elapsed_ms: String(elapsed), elapsed_measured_sessions: 4,
    tool_round_trips: String(rounds), round_trip_measured_sessions: 4 });
const stratum = { client: 'codex', provenance: 'client_reported', session_model_revision_sha256: 'a'.repeat(64) };
const report = { scope: { collection: 'Sample App (demo)' }, start_unix_ms: start, end_unix_ms_exclusive: end,
    calls: 240, eligible_evidence_calls: 200, comparisons: comparison(180, 3200000, 512000),
    missing_baseline_calls: 20, mismatched_baseline_calls: 0, excluded_from_estimation_calls: 40,
    dropped_observations: null, clients: { codex: 200, vscode: 40 }, content_kinds: { code: 160, document: 40 },
    operations: {
        search: { calls: 120, successful: 115, empty: 5, failed: 0, truncated: 3, returned_utf8_bytes: '320000', total_elapsed_us: '3600000' },
        read: { calls: 80, successful: 80, empty: 0, failed: 0, truncated: 0, returned_utf8_bytes: '192000', total_elapsed_us: '800000' },
        status: { calls: 32, successful: 32, empty: 0, failed: 0, truncated: 0, returned_utf8_bytes: '16000', total_elapsed_us: '96000' },
        index: { calls: 8, successful: 8, empty: 0, failed: 0, truncated: 0, returned_utf8_bytes: '8000', total_elapsed_us: '3200000',
            index_work: { indexed: 24, reused: 776, removed: 2, processed_source_bytes: '240000', scan_extract_us: '1800000', publish_us: '1400000', disk_growth_bytes: '1048576' } }
    }
};
const sample = { explanation: demoNote, status: 'partial', days: 30, report,
    policy: { mode: 'aggregates_only', aggregate_retention_days: 30 },
    history: [0, 1, 2].map(i => ({ start_unix_ms: end - (3 - i) * 86400000, calls: 80, comparisons: comparison(60, 1066668, 170668) })),
    lifetime: { ...report, start_unix_ms: start - 86400000 * 30, calls: 720, comparisons: comparison(540, 9600000, 1536000) },
    supplemental: [{ day: end / 86400000 - 1, summaries: [{ stratum, sessions: session(96000, 32000, 12000, 420000, 24),
        linked_airk_requests: 18, paired_with_airk: session(96000, 32000, 12000, 420000, 24),
        paired_without_airk: session(144000, 40000, 14000, 540000, 36), paired_quality: [3, 0, 1, 0, 0] }] }],
    price_scenario: {
        scenario: { model: 'Illustrative model (demo)', currency: 'USD', rate_date: '2026-10-18',
            input_price_per_million: '2.00', output_price_per_million: '8.00',
            cached_input_treatment: 'separate_rate', cached_input_price_per_million: '0.50', context_assumes_uncached: true },
        context_equivalents: [{ baseline_kind: 'full_returned_sources', compared_pairs: 180,
            nominal_input_cost_equivalent: '1.344000', assumption_low: '0.554666', assumption_high: '3.029334' }],
        session_input_equivalents: [{ stratum, kind: 'sessions', sessions: 4, cache_classified_sessions: 4,
            uncached_input_equivalent: '0.128000', cached_input_equivalent: '0.016000', output_equivalent: '0.096000',
            output_measured_sessions: 4, unpriced_input_tokens_unknown_cache_split: '0' }],
        paired_cost_equivalents: [{ pairs: 4, uncached_input_reduction: '0.080000', cached_input_reduction: '0.004000',
            output_reduction: '0.016000', quality: [3, 0, 1, 0, 0] }]
    },
    resources: { peak_rss_bytes: '67108864', overlapped_intervals: 2,
        counters: { user_cpu_us: { value: '1200000', samples: 238 }, system_cpu_us: { value: '400000', samples: 238 } } },
    process_dropped_observations: 0,
    health: { database_bytes: 118784, duplicate_observations: 2, useful_feedback: 12, partly_useful_feedback: 3, not_useful_feedback: 1 }
};
const empty = { explanation: demoNote, status: 'empty', days: 30,
    report: { scope: report.scope, start_unix_ms: start, end_unix_ms_exclusive: end, calls: 0, eligible_evidence_calls: 0 },
    policy: sample.policy, health: {} };
const sidebar = { trusted: true, paused: false, changingStorage: false, mode: 'debounced', groups: sidebarControlGroups(),
    collections: [{ collection: 'demo', name: 'Sample App (fictional demo)', root: 'C:\\Projects\\Sample App',
        dataDir: 'D:\\AIRK Indexes', state: 'current', diskBytes: 756023296,
        detail: { configured_detail: 'balanced', published_detail: 'balanced', pending_reindex: false },
        usage: { calls: 240, reduction: '672,000', compared: 180, checkedAt: end - 3600000, disabled: false } }] };
const scenes = [
    { name: 'usage-overview-dark', width: 1200, height: 900 },
    { name: 'usage-overview-light', width: 1200, height: 900, light: true },
    { name: 'usage-full-report', width: 1200, full: true },
    { name: 'usage-task-and-cost', width: 1200, sections: ['Agent sessions and task results', 'What might this be worth?'] },
    { name: 'usage-developer-details', width: 1200, details: true },
    { name: 'usage-narrow', width: 420, height: 1200 },
    { name: 'usage-first-use', width: 1200, height: 900, empty: true },
    { name: 'usage-sidebar', width: 360, full: true, sidebar: true }
];

function preview(scene) {
    let html = scene.sidebar ? renderSidebar(sidebar) : renderReport('Usage & Savings', 'usage', scene.empty ? empty : sample);
    const bg = scene.light ? '#ffffff' : '#1f1f1f', fg = scene.light ? '#333333' : '#cccccc';
    // Only provide VS Code theme variables and its bridge; keep product markup/styles intact.
    html = html.replace(/(<style nonce="[^"]+">)/, `$1
        :root{--vscode-font-family:"Segoe UI",sans-serif;--vscode-font-size:13px;
        --vscode-foreground:${fg};--vscode-editor-background:${bg};--vscode-sideBar-background:${bg};
        --vscode-descriptionForeground:${scene.light ? '#616161' : '#a0a0a0'};
        --vscode-panel-border:${scene.light ? '#cecece' : '#3c3c3c'};
        --vscode-button-background:#0078d4;--vscode-button-foreground:#ffffff;--vscode-button-hoverBackground:#026ec1;
        --vscode-button-secondaryBackground:#313131;--vscode-button-secondaryForeground:#cccccc;
        --vscode-dropdown-background:#313131;--vscode-dropdown-foreground:#cccccc;--vscode-dropdown-border:#454545;
        --vscode-focusBorder:#007fd4;--vscode-progressBar-background:#0e70c0;--vscode-textBlockQuote-background:#252526;
        --vscode-errorForeground:#f48771;}`);
    html = html.replace(/(<script nonce="[^"]+">)/, '$1let saved; const acquireVsCodeApi=()=>({getState:()=>saved,setState:value=>{saved=value;},postMessage:value=>{window.lastAction=value;}});');
    return html;
}

async function main() {
    await fs.mkdir(output, { recursive: true });
    await fs.mkdir(path.join(root, 'artifacts'), { recursive: true });
    const scratch = await fs.mkdtemp(path.join(root, 'artifacts/usage-render-'));
    const browser = spawn(browserPath, ['--headless', '--disable-gpu', '--no-first-run', '--no-default-browser-check',
        '--remote-debugging-port=0', '--remote-debugging-address=127.0.0.1', `--user-data-dir=${path.join(scratch, 'profile')}`, 'about:blank'],
        { windowsHide: true, stdio: 'ignore' });
    let socket, startError;
    browser.on('error', error => { startError = error; });
    try {
        let port;
        for (let i = 0; i < 100; i++) {
            if (startError) { throw startError; }
            try { port = (await fs.readFile(path.join(scratch, 'profile/DevToolsActivePort'), 'utf8')).split('\n')[0]; break; }
            catch { await delay(100); }
        }
        if (!port) { throw new Error('Browser did not expose its local endpoint. Set AIRK_BROWSER to Chromium or Edge.'); }
        const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
        socket = new WebSocket(targets.find(target => target.type === 'page').webSocketDebuggerUrl);
        await new Promise((resolve, reject) => { socket.addEventListener('open', resolve, { once: true }); socket.addEventListener('error', reject, { once: true }); });
        let sequence = 0;
        const pending = new Map();
        socket.addEventListener('message', event => {
            const value = JSON.parse(event.data), request = pending.get(value.id);
            if (request) { pending.delete(value.id); clearTimeout(request.timer); value.error ? request.reject(new Error(JSON.stringify(value.error))) : request.resolve(value.result); }
        });
        const call = (method, params = {}) => new Promise((resolve, reject) => {
            const id = ++sequence;
            const timer = setTimeout(() => { pending.delete(id); reject(new Error(`Timed out: ${method}`)); }, 15000);
            pending.set(id, { resolve, reject, timer }); socket.send(JSON.stringify({ id, method, params }));
        });
        const evaluate = async expression => {
            const result = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
            if (result.exceptionDetails) { throw new Error(JSON.stringify(result.exceptionDetails)); }
            return result.result.value;
        };
        await call('Page.enable');
        // Fixed demo time zone keeps the sidebar timestamp reproducible across machines.
        await call('Emulation.setTimezoneOverride', { timezoneId: 'UTC' });
        for (const scene of scenes) {
            await call('Emulation.setDeviceMetricsOverride', { width: scene.width, height: scene.height || 900, deviceScaleFactor: 2, mobile: false });
            const source = path.join(scratch, scene.name + '.html'), url = pathToFileURL(source).href;
            await fs.writeFile(source, preview(scene));
            await call('Page.navigate', { url });
            let ready = false;
            for (let i = 0; i < 100; i++) {
                ready = await evaluate(`location.href === ${JSON.stringify(url)} && document.readyState === 'complete' && !!document.querySelector('button')`);
                if (ready) { break; }
                await delay(50);
            }
            if (!ready) { throw new Error(`Page did not load: ${scene.name}`); }
            await evaluate('document.fonts.ready.then(()=>true)');
            if (scene.details) {
                await evaluate("[...document.querySelectorAll('details')].find(d=>d.querySelector('summary').textContent==='Measurement details for developers').open=true");
            }
            if (!await evaluate('document.documentElement.scrollWidth <= window.innerWidth')) { throw new Error(`Page overflows horizontally: ${scene.name}`); }
            let clip = { x: 0, y: 0, width: scene.width, height: scene.height || 900, scale: 1 };
            if (scene.full) { clip.height = await evaluate('Math.ceil(document.documentElement.scrollHeight)'); }
            if (scene.sections || scene.details) {
                const bounds = await evaluate(`(() => {
                    const nodes = ${scene.details
                        ? "[[...document.querySelectorAll('details')].find(d=>d.querySelector('summary').textContent==='Measurement details for developers')]"
                        : `[...document.querySelectorAll('section')].filter(s=>${JSON.stringify(scene.sections)}.includes(s.querySelector('h2')?.textContent))`};
                    return {top:Math.floor(nodes[0].getBoundingClientRect().top+scrollY), bottom:Math.ceil(nodes.at(-1).getBoundingClientRect().bottom+scrollY)};
                })()`);
                clip.y = Math.max(0, bounds.top - 12); clip.height = bounds.bottom - clip.y + 16;
            }
            const png = await call('Page.captureScreenshot', { format: 'png', captureBeyondViewport: true, clip });
            await fs.writeFile(path.join(output, scene.name + '.png'), Buffer.from(png.data, 'base64'));
            const action = scene.sidebar
                ? "document.querySelector('[data-action=usage]').click();window.lastAction"
                : "document.querySelector('[data-usage=refresh]').click();window.lastAction";
            const message = await evaluate(action);
            if (scene.sidebar ? message?.type !== 'collection' || message?.action !== 'usage' : message?.type !== 'usage' || message?.action !== 'refresh') {
                throw new Error(`Expected button message missing: ${scene.name}`);
            }
            console.log(`${scene.name}.png: ${clip.width} x ${clip.height} CSS pixels, rendered at 2x`);
        }
        await call('Browser.close');
    } finally {
        socket?.close(); browser.kill();
    }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
