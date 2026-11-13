// SPDX-License-Identifier: Apache-2.0
// Exercise the actual composition method without constructing VS Code's host.
import { strict as assert } from 'node:assert';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { test } from 'node:test';
import * as ts from 'typescript';
import * as agents from '../airkctl/agents/index';
import type { Entry } from '../generated/i2';

const repo = path.resolve(__dirname, '../../../..');
const projects = ['/reviewed'];
const capability = 'agents-entry-remove/1';

interface Host {
    airkctl: { state: { kind: 'ready'; capabilities: string[] }; session: { json(argv: readonly string[]): Promise<unknown> } };
    requireReady(): Promise<void>;
    folders(): string[];
    output: { appendLine(line: string): void; show(): void };
    notify(kind: string, message: string): void;
}

function actualReconnect(vscode: unknown): (this: Host) => Promise<void> {
    const file = path.join(repo, 'packages/vscode/src/serviceExtension.ts');
    const text = fs.readFileSync(file, 'utf8');
    const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
    const owner = source.statements.find(node => ts.isClassDeclaration(node) && node.name?.text === 'SplitExtension');
    assert.ok(owner && ts.isClassDeclaration(owner), 'actual SplitExtension');
    const method = owner.members.find(node => ts.isMethodDeclaration(node) && node.name.getText(source) === 'reconnectAgents');
    assert.ok(method && ts.isMethodDeclaration(method) && method.body, 'actual reconnectAgents body');
    const imports = source.statements.find(node => ts.isImportDeclaration(node)
        && ts.isStringLiteral(node.moduleSpecifier) && node.moduleSpecifier.text === './airkctl/agents/index');
    assert.ok(imports && ts.isImportDeclaration(imports), 'real native agent wrappers');
    const bindings = imports.importClause?.namedBindings;
    assert.ok(bindings && ts.isNamedImports(bindings));
    const dependencies: Record<string, unknown> = { vscode };
    for (const binding of bindings.elements.filter(item => !item.isTypeOnly)) {
        dependencies[binding.name.text] = (agents as Record<string, unknown>)[binding.propertyName?.text ?? binding.name.text];
    }
    const emitted = ts.transpileModule(`async function reconnectAgents() ${method.body.getText(source)}`, {
        compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
    }).outputText;
    return new Function(...Object.keys(dependencies), `${emitted}\nreturn reconnectAgents;`)(...Object.values(dependencies));
}

function entry(agent: Entry['agent'], key: string, status: Entry['status'], id?: string): Entry {
    return {
        agent, scope: 'project', file: `/reviewed/${agent}.config`,
        containerPath: [agent === 'codex' ? 'mcp_servers' : agent === 'copilot' ? 'servers' : 'mcpServers'],
        serverKey: key, transport: id ? 'https' : 'stdio', status,
        ...(id ? { connectionId: id, url: 'https://127.0.0.1:62133/mcp' } : { command: 'airk' }),
    };
}

function mixed(): Entry[] {
    return (['claude', 'codex', 'copilot'] as const).flatMap((agent, index) => [
        entry(agent, `airk-${agent}-healthy`, 'ok', String(index + 1).repeat(32)),
        entry(agent, `airk-${agent}-repair`, 'https_endpoint_mismatch', String(index + 4).repeat(32)),
        entry(agent, `airk-${agent}-orphan`, 'https_unknown_connection', String(index + 7).repeat(32)),
        entry(agent, `airk-${agent}-retired`, 'stdio_retired'),
    ]);
}

function removable(row: Entry): boolean {
    return row.status === 'https_unknown_connection' || row.status === 'stdio_retired' || row.status === 'stdio_missing_command';
}

function fixture(initial: Entry[], options: {
    capabilities?: string[]; confirmation?: () => void; repairErrors?: boolean; removalError?: string; malformedAfter?: boolean;
} = {}) {
    let current = structuredClone(initial);
    let lists = 0;
    const calls: string[][] = [], messages: string[] = [], logs: string[] = [];
    const survivors = new Map(initial.map(row => [row.serverKey, JSON.stringify(row)]));
    const native = {
        state: { kind: 'ready' as const, capabilities: options.capabilities ?? ['agents/1', capability] },
        session: { async json(argv: readonly string[]): Promise<unknown> {
            calls.push([...argv]);
            assert.equal(argv[0], 'agents');
            if (argv[1] === 'list') {
                lists += 1;
                if (options.malformedAfter && lists > 1) { return { entries: null, findings: 'unavailable' }; }
                return {
                    checked: current.map(row => row.file), blockers: [], entries: structuredClone(current),
                    findings: current.filter(row => row.status !== 'ok' && row.status !== null).map(row => ({ ...row, problem: row.status })),
                };
            }
            if (argv[1] === 'repair') {
                current = current.map(row => row.status === 'https_endpoint_mismatch' ? { ...row, status: 'ok' } : row);
                return { schema: 1, blockers: [], actions: options.repairErrors ? [{ file: '/reviewed/repair.config', serverKey: 'airk-repair', changed: null, error: 'repair refused' }] : [] };
            }
            if (argv[1] === 'remove') {
                const selected = argv.flatMap((argument, index) => argument === '--entry' ? [JSON.parse(argv[index + 1]) as Entry] : []);
                // Model the real old command's unrestricted behavior when no
                // selector is supplied, so the composition defect is observable.
                const targets = selected.length ? selected : current;
                const actions = targets.map(row => {
                    const error = row.serverKey === options.removalError ? 'removal refused' : null;
                    if (!error) { survivors.delete(row.serverKey); }
                    return { file: row.file, serverKey: row.serverKey, connectionId: row.connectionId ?? null, changed: error ? null : true, error };
                });
                return { schema: 1, blockers: [], actions };
            }
            throw new Error(`Unexpected native call ${JSON.stringify(argv)}`);
        } },
    };
    const show = async (message: string) => { messages.push(message); return undefined; };
    const vscode = { window: {
        showInformationMessage: show, showErrorMessage: show,
        showWarningMessage: async (message: string) => {
            messages.push(message);
            if (message.includes('stale agent')) { options.confirmation?.(); return 'Repair'; }
            return undefined;
        },
    } };
    const host: Host = {
        airkctl: native, async requireReady() {}, folders: () => [...projects],
        output: { appendLine: line => logs.push(line), show() {} },
        notify: (_kind, message) => { messages.push(message); },
    };
    return {
        calls, messages, logs, survivors,
        replace(rows: Entry[]) { current = structuredClone(rows); },
        run: () => actualReconnect(vscode).call(host),
    };
}

test('actual Reconnect sends only reviewed orphan and retired entries, preserving healthy and repaired entries across clients', async () => {
    const initial = mixed();
    const f = fixture(initial);
    await f.run();
    const selected = [initial[2], initial[3], initial[6], initial[7], initial[10], initial[11]];
    assert.deepEqual(f.calls.filter(argv => argv[1] === 'remove'), [[
        'agents', 'remove', '--project', '/reviewed', '--no-user',
        ...selected.flatMap(row => ['--entry', JSON.stringify(row)]),
    ]]);
    assert.deepEqual([...f.survivors.keys()].sort(), initial.filter(row => !removable(row)).map(row => row.serverKey).sort());
    for (const row of initial.filter(row => row.status === 'ok')) {
        assert.equal(f.survivors.get(row.serverKey), JSON.stringify(row), 'unselected bytes');
    }
});

test('actual Reconnect does not delete targets introduced or changed during confirmation', async () => {
    const reviewed = entry('claude', 'airk-reviewed', 'https_unknown_connection', '1'.repeat(32));
    const replacement = { ...reviewed, connectionId: '2'.repeat(32) };
    let f: ReturnType<typeof fixture>;
    f = fixture([reviewed], { confirmation: () => f.replace([replacement, entry('codex', 'airk-new', 'stdio_retired')]) });
    await assert.rejects(f.run(), /changed|review/i);
    assert.equal(f.calls.filter(argv => argv[1] === 'remove').length, 0, 'changed/unreviewed rows require a fresh review');
});

test('actual Reconnect refuses selected removal when the verified CLI lacks its capability', async () => {
    const f = fixture(mixed(), { capabilities: ['agents/1'] });
    let refusal: unknown;
    try { await f.run(); } catch (error) { refusal = error; }
    assert.equal(f.calls.filter(argv => argv[1] === 'remove').length, 0, 'older clients never get unrestricted removal');
    assert.ok([...f.messages, String(refusal ?? '')].some(message => /capabilit|updat|support/i.test(message)), 'missing-capability reason reaches the existing command error handler or UI');
});

for (const kind of ['repair', 'removal'] as const) {
    test(`actual Reconnect preserves per-action ${kind} errors instead of reporting overall success`, async () => {
        const f = fixture(mixed(), kind === 'repair' ? { repairErrors: true } : { removalError: 'airk-claude-orphan' });
        let refusal: unknown;
        try { await f.run(); } catch (error) { refusal = error; }
        assert.ok(!f.messages.some(message => message.includes('agent entries were repaired.')), 'partial errors are not success');
        assert.ok([...f.messages, String(refusal ?? '')].some(message => /failed|refus|error|incomplete/i.test(message)), 'partial failure reaches the command error handler or UI');
        assert.ok(f.logs.some(line => line.includes(`${kind} refused`)), 'complete native action error remains logged');
    });
}

test('actual Reconnect treats malformed follow-up evidence as unavailable, never as no remaining failures', async () => {
    const f = fixture(mixed(), { malformedAfter: true });
    try { await f.run(); } catch { /* The existing safe command wrapper displays thrown failures. */ }
    assert.equal(f.calls.filter(argv => argv[1] === 'remove').length, 0);
    assert.ok(!f.messages.some(message => message.includes('agent entries were repaired.')), 'missing evidence is not a pass');
    assert.ok(f.logs.some(line => line.includes('"entries": null') && line.includes('"findings": "unavailable"')), 'the complete unavailable native observation remains inspectable');
});

test('actual Reconnect does not call unclassified native entries current', async () => {
    const f = fixture([entry('claude', 'airk-unknown', null, '1'.repeat(32))]);
    await assert.rejects(f.run(), /incomplete|unavailable/i);
    assert.equal(f.calls.filter(argv => argv[1] !== 'list').length, 0);
    assert.ok(!f.messages.some(message => message.includes('every recognized agent entry is current')));
});
