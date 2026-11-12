// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test, type TestContext } from 'node:test';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
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
