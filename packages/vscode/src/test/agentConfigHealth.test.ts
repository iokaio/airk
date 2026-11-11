// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test, type TestContext } from 'node:test';
import { createRequire } from 'node:module';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import * as vm from 'node:vm';
import { buildSync } from 'esbuild';
import { agentConfigFindings, checkAgentConfigHealth } from '../agentConfigHealth';

/** A fresh project root with the given project-relative files; removed after the test. */
async function project(t: TestContext, files: Record<string, string>): Promise<string> {
    const root = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-config-health-'));
    t.after(() => fs.rm(root, { recursive: true, force: true }));
    for (const [relative, source] of Object.entries(files)) {
        const file = path.join(root, ...relative.split('/'));
        await fs.mkdir(path.dirname(file), { recursive: true });
        await fs.writeFile(file, source);
    }
    return root;
}
/** Injected existence: only the listed commands exist; every probe is recorded. */
function existing(...commands: string[]) {
    const probes: string[] = [];
    return { probes, exists: async (command: string) => { probes.push(command); return commands.includes(command); } };
}
async function snapshot(root: string, files: string[]): Promise<string[]> {
    return Promise.all(files.map(async relative => {
        const file = path.join(root, ...relative.split('/')), info = await fs.stat(file);
        return `${relative}:${info.mtimeMs}:${(await fs.readFile(file)).toString('base64')}`;
    }));
}

test('a Claude stdio AIRK entry whose launcher command is missing is reported with its file, key, command and recorded root', async t => {
    const launcher = path.join(os.tmpdir(), 'airk-removed-install', 'launcher', process.platform === 'win32' ? 'airk-launcher.exe' : 'airk-launcher');
    const source = JSON.stringify({ mcpServers: { airk: { command: launcher, args: ['--root', '/workspace', 'mcp', '--explorer-descriptor', '/descriptor'] } } }, null, 2);
    const root = await project(t, { '.mcp.json': source }), probe = existing();
    const before = await snapshot(root, ['.mcp.json']);
    const health = await checkAgentConfigHealth(root, { exists: probe.exists });
    assert.deepEqual(health.findings, [{ agent: 'claude', label: 'Claude Code', file: path.join(root, '.mcp.json'), scope: 'project', serverKey: 'airk', command: launcher, root: '/workspace', problem: 'stdio_missing_command' }]);
    assert.deepEqual(health.checked, ['.mcp.json']);
    assert.deepEqual(probe.probes, [launcher]);
    assert.deepEqual(await snapshot(root, ['.mcp.json']), before, 'the check never modifies a configuration file');
});

test('AIRK stdio entries whose command exists, or is a PATH command, are retired: the runtime refuses stdio MCP', async t => {
    const present = path.join(os.tmpdir(), 'airk-installed', 'airk');
    const root = await project(t, { '.mcp.json': JSON.stringify({ mcpServers: { airk: { command: present, args: ['mcp'] }, 'airk-docs': { type: 'stdio', command: 'airk', args: ['mcp'] } } }) });
    const probe = existing(present);
    const health = await checkAgentConfigHealth(root, { exists: probe.exists });
    assert.deepEqual(health.findings.map(({ serverKey, problem, command }) => ({ serverKey, problem, command })), [
        { serverKey: 'airk', problem: 'stdio_retired', command: present },
        { serverKey: 'airk-docs', problem: 'stdio_retired', command: 'airk' },
    ]);
    assert.deepEqual(health.checked, ['.mcp.json']);
    assert.deepEqual(probe.probes, [present], 'a PATH-relative command is never probed');
});

test('HTTPS AIRK entries are not checked by the configuration check', async t => {
    const root = await project(t, {
        '.mcp.json': JSON.stringify({ mcpServers: { airk: { type: 'http', url: 'https://127.0.0.1:45123/mcp', headers: { Authorization: 'Bearer ${AIRK_MCP_TOKEN_0123}' } } } }),
        '.vscode/mcp.json': JSON.stringify({ servers: { airk: { type: 'http', url: 'https://127.0.0.1:45123/mcp', command: path.join(os.tmpdir(), 'gone', 'airk') } } }),
        '.codex/config.toml': '[mcp_servers.airk]\nurl = "https://127.0.0.1:45123/mcp"\nbearer_token_env_var = "AIRK_MCP_TOKEN_0123"\n',
    });
    const probe = existing();
    const health = await checkAgentConfigHealth(root, { exists: probe.exists });
    assert.deepEqual(health.findings, []);
    assert.deepEqual(health.checked, ['.codex/config.toml', '.mcp.json', '.vscode/mcp.json']);
    assert.deepEqual(probe.probes, []);
});

test('Codex TOML, Copilot and nested-folder AIRK entries with missing commands are reported in location order', async t => {
    const gone = (name: string) => path.join(os.tmpdir(), 'airk-removed-install', name);
    const root = await project(t, {
        '.codex/config.toml': `model = "user-model"\n[mcp_servers.airk-legacy]\ncommand = ${JSON.stringify(gone('codex'))}\nargs = ["mcp"]\n[mcp_servers.airk-legacy.env]\nAIRK_CLIENT = "codex"\n`,
        '.vscode/mcp.json': JSON.stringify({ servers: { airk: { type: 'stdio', command: gone('copilot'), args: ['mcp'] } }, inputs: [] }),
        '.junie/mcp/mcp.json': JSON.stringify({ mcpServers: { airk_explorer: { command: gone('junie'), args: [] } } }),
    });
    const health = await checkAgentConfigHealth(root, { exists: existing().exists });
    assert.deepEqual(health.findings, [
        { agent: 'codex', label: 'Codex', file: path.join(root, '.codex', 'config.toml'), scope: 'project', serverKey: 'airk-legacy', command: gone('codex'), problem: 'stdio_missing_command' },
        { agent: 'copilot', label: 'VS Code / GitHub Copilot Chat', file: path.join(root, '.vscode', 'mcp.json'), scope: 'project', serverKey: 'airk', command: gone('copilot'), problem: 'stdio_missing_command' },
        { agent: 'junie', label: 'JetBrains Junie', file: path.join(root, '.junie', 'mcp', 'mcp.json'), scope: 'project', serverKey: 'airk_explorer', command: gone('junie'), problem: 'stdio_missing_command' },
    ]);
    assert.deepEqual(health.checked, ['.codex/config.toml', '.vscode/mcp.json', '.junie/mcp/mcp.json']);
});

test('non-AIRK servers and AIRK keys outside the project container are ignored', async t => {
    const gone = path.join(os.tmpdir(), 'airk-removed-install', 'airk-launcher');
    const root = await project(t, {
        '.mcp.json': JSON.stringify({ mcpServers: { other: { command: gone }, airkish: { command: gone }, 'my-airk': { command: gone } }, projects: { '/elsewhere': { mcpServers: { airk: { command: gone } } } } }),
        '.vscode/mcp.json': JSON.stringify({ mcpServers: { airk: { command: gone } }, servers: { notes: { command: gone } } }),
        '.codex/config.toml': `[profiles.work.mcp_servers.airk]\ncommand = ${JSON.stringify(gone)}\n[mcp_servers.other]\ncommand = ${JSON.stringify(gone)}\n`,
    });
    const probe = existing();
    const health = await checkAgentConfigHealth(root, { exists: probe.exists });
    assert.deepEqual(health.findings, []);
    assert.deepEqual(probe.probes, []);
});

test('malformed and oversized files are reported as unreadable without throwing, and other files are still checked', async t => {
    const gone = path.join(os.tmpdir(), 'airk-removed-install', 'airk-launcher');
    const root = await project(t, {
        '.mcp.json': '{"mcpServers": {"airk": {"command": ',
        '.codex/config.toml': `[mcp_servers.airk]\ncommand = ${JSON.stringify(gone)}\n`,
        '.vscode/mcp.json': JSON.stringify({ servers: {} }) + ' '.repeat(1_048_576),
        '.cursor/mcp.json': '{"mcpServers": {"airk": {"command": "a"}, "airk": {"command": "b"}}}',
    });
    const health = await checkAgentConfigHealth(root, { exists: existing().exists });
    assert.deepEqual(health.findings.map(({ agent, problem, serverKey }) => ({ agent, problem, serverKey })), [
        { agent: 'codex', problem: 'stdio_missing_command', serverKey: 'airk' },
        { agent: 'claude', problem: 'unreadable', serverKey: undefined },
        { agent: 'copilot', problem: 'unreadable', serverKey: undefined },
        { agent: 'cursor', problem: 'unreadable', serverKey: undefined },
    ]);
    for (const finding of health.findings.slice(1)) { assert.ok(finding.detail, `${finding.agent} unreadable finding explains why`); assert.equal(finding.command, undefined); }
    assert.equal(health.findings[1].file, path.join(root, '.mcp.json'));
    assert.deepEqual(health.checked, ['.codex/config.toml'], 'only parsed files are listed as checked');
    const direct = await agentConfigFindings('gemini', '/project/.gemini/settings.json', '[1, 2]', existing().exists);
    assert.deepEqual(direct.map(finding => finding.problem), ['unreadable']);
});

test('a project without agent configuration files has no findings, and a failed trust or selection check stops the read', async t => {
    const empty = await project(t, {});
    assert.deepEqual(await checkAgentConfigHealth(empty, { exists: existing().exists }), { checked: [], findings: [] });
    const root = await project(t, { '.mcp.json': JSON.stringify({ mcpServers: { airk: { command: path.join(os.tmpdir(), 'gone', 'airk') } } }) });
    let calls = 0;
    await assert.rejects(checkAgentConfigHealth(root, { exists: existing().exists, check: () => { if (++calls > 2) { throw new Error('selection superseded'); } } }), /selection superseded/);
});

// ---- Management > Agents rendering (actual component, minimal hook host).
const managePath = path.resolve(__dirname, '../../src/explorerWebview/manage.tsx');
let manageBundle: string | undefined;
function nodes(node: any): any[] {
    if (Array.isArray(node)) { return node.flatMap(nodes); }
    return !node || typeof node !== 'object' ? [] : [node, ...nodes(node.props?.children)];
}
function text(node: any): string {
    if (node === undefined || node === null || typeof node === 'boolean') { return ''; }
    return Array.isArray(node) ? node.map(text).join('') : typeof node === 'object' ? text(node.props?.children) : String(node);
}
function agentsScreen(project: string, replies: unknown[]) {
    manageBundle ??= buildSync({ entryPoints: [managePath], bundle: true, write: false, platform: 'node', format: 'cjs', jsx: 'automatic', jsxImportSource: 'preact',
        external: ['preact', 'preact/hooks', 'preact/jsx-runtime', 'airk-explorer-help'] }).outputFiles[0].text;
    const slots: any[] = [], effects: (() => unknown)[] = [], sent: any[] = [], events = new Map<string, (event: any) => void>();
    let cursor = 0;
    const hooks = {
        useState(initial: any) {
            const at = cursor++;
            if (!(at in slots)) { slots[at] = typeof initial === 'function' ? initial() : initial; }
            return [slots[at], (next: any) => { slots[at] = typeof next === 'function' ? next(slots[at]) : next; }];
        },
        useRef(initial: any) { const at = cursor++; return slots[at] ??= { current: initial }; },
        useEffect(effect: () => unknown, dependencies?: unknown[]) {
            const at = cursor++, prior = slots[at];
            if (!prior || !dependencies || dependencies.some((value, i) => !Object.is(value, prior[i]))) { effects.push(effect); }
            slots[at] = dependencies;
        },
        useMemo(get: () => unknown) { return get(); },
        useContext() { return { dismissed: [] }; },
    };
    const module = { exports: {} as any }, nativeRequire = createRequire(managePath);
    vm.runInNewContext(manageBundle, { module, exports: module.exports, TextEncoder,
        require: (id: string) => id === 'preact/hooks' ? hooks : id === 'airk-explorer-help' ? { topics: [] } : nativeRequire(id),
        setTimeout: () => 1, clearTimeout() {}, setInterval: () => 1, clearInterval() {},
        window: { addEventListener: (name: string, listener: (event: any) => void) => events.set(name, listener) },
        acquireVsCodeApi: () => ({ getState() {}, setState() {}, postMessage(message: any) {
            sent.push(message);
            const data = message.type === 'agents.configHealth' ? replies.shift() : null;
            queueMicrotask(() => events.get('message')?.({ data: { v: 1, type: 'result', requestId: message.requestId, ok: true, data } }));
        } }),
    });
    const state: any = { trusted: true, selectedProject: project, projects: [],
        management: { trusted: true, connected: true, busy: false, authority: 'fixture', checkedAt: 0, responseMode: 'terse', enabled: {} } };
    return {
        sent,
        render() {
            cursor = 0;
            const screen = module.exports.AgentsScreen({ state, route: { workspace: 'manage', tab: 'agents' }, go() {}, publications: [] });
            const element = nodes(screen).find(node => typeof node.type === 'function' && /^AgentConfigCheck\d*$/.test(node.type.name));
            assert.ok(element, 'Management > Agents renders the agent configuration check');
            assert.equal(element.props.project, project);
            return element.type(element.props);
        },
        async flush() { while (effects.length) { effects.shift()!(); } await new Promise(resolve => setImmediate(resolve)); },
    };
}
const button = (tree: any, label: RegExp) => nodes(tree).find(node => node.type === 'button' && label.test(text(node)));

test('Management > Agents lists a broken AIRK entry with its repair and re-checks on request', async () => {
    const project = 'ab'.repeat(32), launcher = 'C:\\Users\\fixture\\AppData\\Local\\AIRK\\launcher\\airk-launcher.exe';
    const finding = { agent: 'claude', label: 'Claude Code', file: 'D:\\work\\.mcp.json', scope: 'project', serverKey: 'airk', command: launcher, root: 'D:\\work', problem: 'stdio_missing_command' };
    const screen = agentsScreen(project, [{ project, checked: ['.mcp.json'], findings: [finding] }, { project, checked: ['.mcp.json'], findings: [] }]);
    screen.render(); await screen.flush();
    assert.deepEqual(screen.sent.map(message => [message.type, message.project]), [['agents.configHealth', project]]);
    let tree = screen.render(), content = text(tree);
    assert.match(content, /Agent configuration check/);
    for (const expected of ['Claude Code', 'D:\\work\\.mcp.json', 'airk', `Missing command: ${launcher}`, 'Project folder: D:\\work',
        'This entry starts an AIRK program that no longer exists (an earlier AIRK installation was replaced or removed). AIRK now connects agents over local HTTPS. Reconnect replaces this entry for Claude Code; other servers and settings are kept.']) {
        assert.ok(content.includes(expected), expected);
    }
    button(tree, /^Reconnect…$/).props.onClick(); await screen.flush();
    assert.deepEqual(screen.sent.at(-1).type, 'manage.command'); assert.equal(screen.sent.at(-1).command, 'airk.reconnectAgents');
    button(tree, /^Connect a coding agent…$/).props.onClick(); await screen.flush();
    assert.deepEqual(screen.sent.at(-1).type, 'manage.command'); assert.equal(screen.sent.at(-1).command, 'airk.setupAgent');
    button(tree, /^Check again$/).props.onClick(); screen.render(); await screen.flush();
    assert.equal(screen.sent.filter(message => message.type === 'agents.configHealth').length, 2);
    tree = screen.render(); content = text(tree);
    assert.ok(content.includes("No broken AIRK entries found in this project's agent configuration files."));
    assert.equal(content.includes('Missing command'), false);
});
