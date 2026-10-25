// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { randomUUID } from 'node:crypto';
import { LaunchSpec, launchArgs } from './model';
import { runText } from './native';
import { renderReport } from './report';
import { usageAction } from './usageReport';

async function withJson<T>(value: unknown, action: (file: string) => Promise<T>): Promise<T> {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-usage-'));
    const file = path.join(directory, 'input.json');
    try { await fs.writeFile(file, JSON.stringify(value), { flag: 'wx', mode: 0o600 }); return await action(file); }
    finally { await fs.unlink(file).catch(() => undefined); await fs.rmdir(directory); }
}

async function prices(): Promise<object | undefined> {
    const ask = (title: string, prompt: string, value?: string) => vscode.window.showInputBox({ title, prompt, value, ignoreFocusOut: true });
    const model = await ask('Cost assumptions', 'Model name matching your measurements (for example, your provider’s model ID)'); if (!model) { return; }
    const currency = await ask('Currency', 'Three-letter currency code', 'USD'); if (!currency) { return; }
    const rateDate = await ask('Price date', 'Date these rates apply, YYYY-MM-DD', new Date().toISOString().slice(0, 10)); if (!rateDate) { return; }
    const input = await ask('Input price', 'Price per million input tokens. Use your provider’s rate; AIRK does not fetch prices.'); if (input === undefined) { return; }
    const output = await ask('Output price (optional)', 'Price per million output tokens. Leave blank if unknown.'); if (output === undefined) { return; }
    const cached = await ask('Cached input price (optional)', 'Price per million cached input tokens. Leave blank if unknown.'); if (cached === undefined) { return; }
    const assumption = await vscode.window.showQuickPick([
        { label: 'Show session costs only', value: false, description: 'No assumption about whether the estimated avoided context was cached.' },
        { label: 'Also price estimated context reduction', value: true, description: 'Assume every estimated avoided input token would have been uncached.' }
    ], { title: 'What should the cost estimate include?' });
    if (!assumption) { return; }
    return { model, currency, rate_date: rateDate, input_price_per_million: input,
        output_price_per_million: output || null, cached_input_treatment: cached ? 'separate_rate' : 'unknown',
        cached_input_price_per_million: cached || null, context_assumes_uncached: assumption.value };
}

/** Each panel captures a collection; stale workspace/storage bindings reject all actions. */
export async function openUsagePanel(spec: LaunchSpec, valid: () => boolean,
    write: (args: string[], valid?: () => boolean) => Promise<unknown>): Promise<void> {
    const panel = vscode.window.createWebviewPanel('airk.usage', `AIRK Usage & Savings: ${spec.name}`, vscode.ViewColumn.Active, { enableScripts: true, localResourceRoots: [] });
    let days = 30, price: object | undefined, report: object = {}, busy = false, disposed = false;
    const check = () => { if (disposed || !vscode.workspace.isTrusted || !valid()) { throw new Error('This collection or storage selection changed. Reopen Usage & Savings from the current collection.'); } };
    const commit = (args:string[]) => { check();return write(args,()=>{try{check();return true;}catch{return false;}}); };
    const read = async (format = 'json', chosenPrice = price): Promise<string> => {
        check();
        const args = ['report', '--days', String(days), '--format', format];
        return chosenPrice ? withJson(chosenPrice, file => runText(spec.command, [...launchArgs(spec, 'usage'), ...args, '--price-scenario', file], path.dirname(spec.command),180000))
            : runText(spec.command, [...launchArgs(spec, 'usage'), ...args], path.dirname(spec.command),180000);
    };
    const render = () => { check(); panel.webview.html = renderReport('Usage & Savings', 'usage', { ...report, days, busy }); };
    const refresh = async () => { report = JSON.parse(await read()); render(); };
    const show = async (content: string, language: string) => { check(); await vscode.window.showTextDocument(await vscode.workspace.openTextDocument({ content, language }), { preview: false }); };
    const listener = panel.webview.onDidReceiveMessage(async message => {
        const action = usageAction(message);
        const json = message && typeof message === 'object' && Object.keys(message).length === 1 && message.type === 'openJson';
        if ((!action && !json) || busy) { return; }
        busy = true;
        try {
            check();
            render();
            if (json || action?.action === 'json') { await show(JSON.stringify(report, null, 2), 'json'); return; }
            if (!action) { return; }
            switch (action.action) {
                case 'days': days = action.days!; await refresh(); break;
                case 'refresh': await refresh(); break;
                case 'csv': case 'markdown': {
                    const content = await read(action.action);
                    const destination = await vscode.window.showSaveDialog({ saveLabel: 'Export usage report', filters: action.action === 'csv' ? { CSV: ['csv'] } : { Markdown: ['md'] } });
                    if (destination) { check(); await vscode.workspace.fs.writeFile(destination, Buffer.from(content, 'utf8')); }
                    break;
                }
                case 'prices': {
                    const selected = await prices(); if (!selected) { break; }
                    const candidate = JSON.parse(await read('json', selected)); check(); price = selected; report = candidate; render(); break;
                }
                case 'removePrices': price = undefined; await refresh(); break;
                case 'recording': {
                    const selected = await vscode.window.showQuickPick([
                        { label: 'Record daily totals', mode: 'aggregates-only', description: 'Recommended. No query, prompt or source text is stored.' },
                        { label: 'Turn recording off', mode: 'disabled', description: 'Keep existing history; stop recording future requests.' },
                        { label: 'Record totals and short-lived diagnostics', mode: 'diagnostic-events', description: 'Content-free request measurements, bounded to 128 events and 24 hours.' }
                    ], { title: 'Local usage recording' });
                    if (selected) { check(); await commit(['configure', '--mode', selected.mode]); await refresh(); } break;
                }
                case 'clear': {
                    const answer = await vscode.window.showWarningMessage(`Clear all usage history for ${spec.name}? Daily totals, longer-term totals, imported measurements and feedback will be removed. Your source files and index stay intact.`, { modal: true }, 'Clear history');
                    if (answer === 'Clear history') { check(); await commit(['clear']); await refresh(); } break;
                }
                case 'import': {
                    const files = await vscode.window.showOpenDialog({ canSelectMany: false, filters: { 'Content-free measurement JSON': ['json'] }, title: 'Select an AIRK session or paired-task measurement' });
                    if (!files?.[0]) { break; }
                    {
                        check();const file=files[0].fsPath;
                        const preview = JSON.parse(await runText(spec.command,[...launchArgs(spec,'usage'),'import',file,'--preview'],path.dirname(spec.command),180000));
                        if(typeof preview.input_sha256!=='string'||!/^[a-f\d]{64}$/i.test(preview.input_sha256))throw new Error('Measurement preview identity is unavailable.');
                        await show(JSON.stringify(preview, null, 2), 'json');
                        const answer = await vscode.window.showInformationMessage(`Import the displayed measurement into ${spec.name}? It will remain labelled as supplied evidence. Request links, if present, must still be retained in this collection.`, { modal: true }, 'Import measurement');
                        if (answer === 'Import measurement') { check(); await commit(['import',file,'--if-sha256',preview.input_sha256]); await refresh(); }
                    } break;
                }
                case 'template': {
                    await show(JSON.stringify({ schema_version: 1, event_id: randomUUID().replace(/-/g, ''), unix_ms: Date.now(), client: 'unknown', content_kind: 'unknown', provenance: 'client_reported',
                        context: null, session: { model_revision_sha256: 'REPLACE_WITH_MODEL_REVISION_SHA256', requests: 1, input_tokens: null, cached_input_tokens: null, output_tokens: null, elapsed_ms: null, tool_round_trips: null },
                        paired_task: null, airk_request_ids: [] }, null, 2), 'json');
                    void vscode.window.showInformationMessage('Fill in final session counters and the model revision digest, save the JSON, then use Import measurements. Leave unavailable counters null. Include retained AIRK request_id values to link the session.'); break;
                }
                case 'feedback': {
                    const id = await vscode.window.showInputBox({ title: 'Rate an AIRK result', prompt: 'Paste the request_id from the AIRK result (32 hexadecimal characters)', validateInput: value => /^[a-f\d]{32}$/i.test(value) ? undefined : 'Enter a 32-character request ID.' });
                    if (!id) { break; }
                    const rating = await vscode.window.showQuickPick([{ label: 'Useful', value: 'useful' }, { label: 'Partly useful', value: 'partly-useful' }, { label: 'Not useful', value: 'not-useful' }], { title: 'Did this result help?' });
                    if (rating) { check(); await commit(['feedback', id, '--usefulness', rating.value]); await refresh(); } break;
                }
            }
        } catch (error) { void vscode.window.showErrorMessage(`AIRK usage: ${String(error)}`); }
        finally { busy = false; try { render(); } catch { /* Stale/disposed panels stay unauthorized. */ } }
    });
    panel.onDidDispose(() => { disposed = true; listener.dispose(); });
    try { await refresh(); } catch (error) { panel.dispose(); throw error; }
}
