// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test, type TestContext } from 'node:test';
import { createHash, X509Certificate } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { parse } from 'jsonc-parser';
import { checkAgentConnections, type AgentHealthHttp } from '../agentConfigHealth';
import { airkHttpDeclaration, httpConfigurationConnectionId, migrateAgentConfig, refreshHttpAgentConfig } from '../agentMigration';
import { RECONNECT_LEDGER_KEY, captureReconnectLedger, clientReferences, findingsDigest, hostReconnectAction, ledgerLaunchers, mergeReconnectLedgers, normalizedUrl, plainPath,
    reconnectGroups, replacedHostArtifacts, validateReconnectLedger, type ClientReferences } from '../agentReconnect';
import { parseTomlDeclarations } from '../agentToml';
import { commandExecutable, credentialHelperCommand, credentialHelperConnectionId, httpServerFields, type LaunchSpec } from '../model';
import { ServiceError } from '../serviceClient';
import { vscodeCredentialInputForId, type HttpConnectionInfo } from '../serviceHttp';
import { RuntimeHost } from '../serviceBootstrap';
import { catalogObject, type CatalogObject, type CatalogReview } from '../indexingCatalog';
import { nativeScopeLaunchSpec } from '../serviceWorkspace';
import { reviewCombinedScopeSelection } from '../scopeSelection';
import * as globalInventory from '../globalAgentInventory';
import { serviceTestHost } from './serviceTestHost';
import { ca } from './fixtures/httpTlsFixture';
import { predecessorHttpFixture } from './fixtures/predecessorHttpFixture';

const win = process.platform === 'win32';
const sha256 = (value: string) => createHash('sha256').update(value, 'utf8').digest('hex');
/** The machine-specific spellings the replaced 1.8.20 installation left in agent files. */
const userBase = path.resolve(os.tmpdir(), 'airk-reconnect-user');
const launcher = path.join(userBase, 'AIRK', 'launcher', win ? 'airk-launcher.exe' : 'airk-launcher');
const descriptor = path.join(userBase, 'Code', 'User', 'globalStorage', 'iokaio.airk', 'explorer-assistance', 'launch-1.json');
const namespaced = (root: string) => win ? '\\\\?\\' + root : root;
/** The exact `.mcp.json` shape observed on the affected machine. */
const userMcpJson = (root: string, command = launcher) => JSON.stringify({ mcpServers: { airk: { command, args: ['--root', namespaced(root), 'mcp', '--explorer-descriptor', descriptor] } } }, null, 2) + '\n';
/** The exact `.codex/config.toml` shape observed on the affected machine. */
const userCodexToml = (root: string, command = launcher) => `[mcp_servers.airk]\ncommand = ${JSON.stringify(command)}\nargs = ${JSON.stringify(['--root', namespaced(root), 'mcp', '--explorer-descriptor', descriptor])}\n`;

const install = path.resolve('fixture-reconnect-install'), storage = path.resolve('fixture-reconnect-storage');
const id = '1'.repeat(32), url = 'https://127.0.0.1:57421/mcp', oldUrl = 'https://127.0.0.1:57400/mcp';
const info: HttpConnectionInfo = { schema: 1, connection_id: id, endpoint: { schema: 1, installation: '2'.repeat(64), user: 'fixture', namespace: '3'.repeat(32), generation: 1, service_instance: '4'.repeat(32),
    executable_sha256: '5'.repeat(64), url, ca_sha256: '6'.repeat(64), certificate_sha256: '7'.repeat(64), state: 'ready', predecessor_digest: null },
    selection: { kind: 'scoped', selection: { schema: 1, context_key: '8'.repeat(64), namespace: '3'.repeat(32), expected_catalog_revision: 1, grant_id: 'grt_' + '9'.repeat(32), expected_grant_revision: 1,
        index_id: 'idx_' + 'a'.repeat(32), project_ids: ['prj_' + 'b'.repeat(32)] } }, ca_certificate_file: path.join(install, 'ca.pem'), credential_file: path.join(install, `credential-${id}.token`) };
const helper = { executable: path.join(install, 'runtimes', 'ab'.repeat(32), win ? 'airk.exe' : 'airk'), installDir: install };
const projectRoot = path.resolve('fixture-reconnect-project');
const spec = (root = projectRoot): LaunchSpec => ({ root, dataDir: path.resolve('fixture-reconnect-data'), collection: 'fixture', name: 'fixture', command: helper.executable,
    service: { installDir: install, project: 'c'.repeat(64) }, http: info });
const references: ClientReferences = { executables: [launcher, path.join(userBase, 'AIRK', 'runtime', 'runtimes', 'old', win ? 'airk.exe' : 'airk')], httpEndpoint: oldUrl, caSha256: 'd'.repeat(64) };

async function folder(t: TestContext, files: Record<string, string> = {}): Promise<string> {
    const root = await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(), 'airk-reconnect-')));
    t.after(() => fs.rm(root, { recursive: true, force: true }));
    for (const [relative, source] of Object.entries(files)) {
        const file = path.join(root, ...relative.split('/'));
        await fs.mkdir(path.dirname(file), { recursive: true });
        await fs.writeFile(file, source);
    }
    return root;
}

test('the replacement plan client references are validated, and an older plan without them yields none', () => {
    assert.equal(clientReferences({ required: true, agents: 'text' }), undefined);
    assert.deepEqual(clientReferences({ client_references: { executables: references.executables, http_endpoint: oldUrl, ca_sha256: 'd'.repeat(64) } }), references);
    assert.deepEqual(clientReferences({ client_references: { executables: [], http_endpoint: null, ca_sha256: null } }), { executables: [], httpEndpoint: null, caSha256: null });
    for (const invalid of [{ executables: ['relative/airk.exe'], http_endpoint: null, ca_sha256: null }, { executables: [], http_endpoint: 'https://example.com/mcp', ca_sha256: null },
        { executables: [], http_endpoint: null, ca_sha256: 'XYZ' }, { executables: 'airk', http_endpoint: null, ca_sha256: null }]) {
        assert.throws(() => clientReferences({ client_references: invalid }), /invalid_replacement_client_references/, JSON.stringify(invalid));
    }
});

test('capture before replacement records the exact launcher entries of .mcp.json and .codex/config.toml with digests and the recorded folder', () => {
    const root = projectRoot, mcp = userMcpJson(root), codex = `model = "keep"\n${userCodexToml(root)}\n[mcp_servers.other]\ncommand = "node"\n`;
    const claudeUser = JSON.stringify({ projects: { [root]: { mcpServers: { airk: { type: 'http', url: oldUrl, headers: { Authorization: 'Bearer ${AIRK_MCP_TOKEN_' + 'E'.repeat(32) + '}' } } } } }, mcpServers: { other: { command: 'node', args: ['x'] } } });
    const ledger = captureReconnectLedger([
        { agent: 'claude', scope: 'project', destination: path.join(root, '.mcp.json'), source: mcp },
        { agent: 'codex', scope: 'project', destination: path.join(root, '.codex', 'config.toml'), source: codex },
        { agent: 'claude', scope: 'user', destination: path.join(userBase, '.claude.json'), source: claudeUser },
        { agent: 'cursor', scope: 'project', destination: path.join(root, '.cursor', 'mcp.json'), source: '{"mcpServers": ' },
    ], references, install, 1234);
    assert.deepEqual(ledger.entries.map(entry => [entry.agent, entry.scope, entry.serverKey, entry.reference, entry.root, entry.containerPath]), [
        ['claude', 'project', 'airk', 'executable', root, ['mcpServers']],
        ['codex', 'project', 'airk', 'executable', root, ['mcp_servers']],
        ['claude', 'user', 'airk', 'endpoint', undefined, ['projects', root, 'mcpServers']],
    ]);
    assert.equal(ledger.entries[0].fileSha256, sha256(mcp)); assert.equal(ledger.entries[1].fileSha256, sha256(codex));
    assert.equal(ledger.entries[0].entrySha256, sha256(JSON.stringify(parse(mcp).mcpServers.airk)));
    assert.deepEqual(ledger.blockers.map(blocker => blocker.destination), [path.join(root, '.cursor', 'mcp.json')], 'an unparsable file is a blocker, never a guess');
    assert.equal(JSON.stringify(ledger).includes('"other"'), false, 'unrelated servers and file bytes are not recorded');
    assert.equal(JSON.stringify(ledger).includes('keep'), false);
    assert.deepEqual(validateReconnectLedger(structuredClone(ledger)), ledger);
    assert.throws(() => validateReconnectLedger({ ...ledger, entries: [{ ...ledger.entries[0], entrySha256: 'x' }] }), /invalid_reconnect_ledger/);
    assert.throws(() => validateReconnectLedger({ ...ledger, schema: 2 }), /invalid_reconnect_ledger/);
    assert.deepEqual(ledgerLaunchers(ledger), references.executables);
    // A later capture keeps unfinished entries it can no longer see.
    const later = captureReconnectLedger([], { ...references, executables: [] }, install, 5678);
    assert.equal(mergeReconnectLedgers(ledger, later).entries.length, 3);
    assert.equal(plainPath('\\\\?\\D:\\code\\airk'), 'D:\\code\\airk'); assert.equal(plainPath('\\\\?\\UNC\\server\\share'), '\\\\server\\share');
});

test('health classifies every AIRK entry class from project, user-level and ledger evidence, and leaves unrelated servers alone', async t => {
    const root = await folder(t, { '.mcp.json': userMcpJson('placeholder'), '.codex/config.toml': userCodexToml('placeholder', process.execPath) });
    await fs.writeFile(path.join(root, '.mcp.json'), userMcpJson(root));
    await fs.writeFile(path.join(root, '.codex', 'config.toml'), userCodexToml(root, process.execPath));
    const dead = 'e'.repeat(32), moved = 'b'.repeat(32), current = 'c'.repeat(32);
    const bearer = (value: string) => 'Bearer ${AIRK_MCP_TOKEN_' + value.toUpperCase() + '}';
    const claudeUser = JSON.stringify({
        mcpServers: { 'airk-moved': { type: 'http', url: oldUrl, headers: { Authorization: bearer(moved) } }, 'airk-current': { type: 'http', url, headers: { Authorization: bearer(current) } },
            'airk-foreign-stdio': { command: path.join(userBase, 'elsewhere', 'airk.exe'), args: ['mcp'] }, other: { command: 'node' } },
        projects: { [root]: { mcpServers: { airk: { type: 'http', url, headers: { Authorization: bearer(dead) } } } } },
    });
    const copilotUser = JSON.stringify({ servers: { airk: { type: 'http', url, headers: { Authorization: `Bearer \${input:airkToken_${current}}` } } }, inputs: [vscodeCredentialInputForId(current)] });
    const userClaude = path.join(userBase, '.claude.json'), userCopilot = path.join(userBase, 'Code', 'User', 'mcp.json');
    const lookups: string[] = [];
    let trustAsked = 0;
    const http: AgentHealthHttp = { endpointUrl: url, lookup: async connection => { lookups.push(connection); return connection === dead ? { status: 'not_owned' } : { status: 'owned', url }; },
        issuerTrusted: async () => { trustAsked++; return false; } };
    const health = await checkAgentConnections([root], { exists: async command => command !== launcher, verifiedLaunchers: [launcher], http,
        userFiles: [{ agent: 'claude', destination: userClaude, source: claudeUser }, { agent: 'copilot', destination: userCopilot, source: copilotUser }] });
    assert.deepEqual(health.entries.map(entry => [entry.scope, path.basename(entry.file), entry.serverKey, entry.status]), [
        ['project', 'config.toml', 'airk', 'stdio_retired'],
        ['project', '.mcp.json', 'airk', 'stdio_missing_command'],
        ['user', '.claude.json', 'airk-moved', 'https_endpoint_mismatch'],
        ['user', '.claude.json', 'airk-current', 'https_trust_missing'],
        ['user', '.claude.json', 'airk', 'https_unknown_connection'],
        ['user', 'mcp.json', 'airk', 'ok'],
    ]);
    assert.deepEqual(health.findings.map(finding => finding.problem), ['stdio_retired', 'stdio_missing_command', 'https_endpoint_mismatch', 'https_trust_missing', 'https_unknown_connection']);
    assert.equal(health.findings[1].root, root, 'the recorded --root is reported without its namespace prefix');
    assert.deepEqual(health.findings[4].containerPath, ['projects', root, 'mcpServers']);
    assert.equal(trustAsked, 1, 'issuer trust is asked at most once per check');
    assert.deepEqual([...new Set(lookups)].sort(), [current, dead, moved].sort());
    assert.equal(health.digest, findingsDigest([...health.findings].reverse()), 'the digest is independent of order');
    assert.notEqual(health.digest, findingsDigest(health.findings.slice(1)));
    // Without native observations HTTPS entries are never guessed.
    const offline = await checkAgentConnections([root], { exists: async () => true, userFiles: [{ agent: 'claude', destination: userClaude, source: claudeUser }] });
    assert.deepEqual(offline.findings.map(finding => finding.problem), ['stdio_retired', 'stdio_retired']);
});

test('a user-level stdio entry needs ownership evidence: a verified AIRK launcher or a ledger capture', async () => {
    const source = JSON.stringify({ mcpServers: { airk: { command: launcher, args: ['--root', namespaced(projectRoot), 'mcp'] }, 'airk-own': { command: path.join(userBase, 'own', 'airk.exe'), args: [] } } });
    const userFiles = [{ agent: 'claude' as const, destination: path.join(userBase, '.claude.json'), source }];
    assert.deepEqual((await checkAgentConnections([], { exists: async () => false, userFiles })).findings, []);
    const verified = await checkAgentConnections([], { exists: async () => false, userFiles, verifiedLaunchers: [launcher] });
    assert.deepEqual(verified.findings.map(finding => [finding.serverKey, finding.scope, finding.problem, finding.root]), [['airk', 'user', 'stdio_missing_command', projectRoot]]);
    const ledger = captureReconnectLedger([{ agent: 'claude', scope: 'user', destination: userFiles[0].destination, source }], { executables: [path.join(userBase, 'own', 'airk.exe')], httpEndpoint: null, caSha256: null }, install, 1);
    const recorded = await checkAgentConnections([], { exists: async () => false, userFiles, ledger });
    assert.deepEqual(recorded.findings.map(finding => [finding.serverKey, finding.ledger]), [['airk-own', true]]);
});

test('Connect a coding agent rewrites the universal launcher entry in place once the launcher is verified, keeping comments, other servers and disabled state', () => {
    const args = ['--root', namespaced(projectRoot), 'mcp', '--explorer-descriptor', descriptor];
    const source = `// keep this comment\n{\n  "mcpServers": {\n    "other": { "command": "node", "args": ["server.js"] },\n    "airk": { "command": ${JSON.stringify(launcher)}, "args": ${JSON.stringify(args)}, "disabled": true }\n  }\n}\n`;
    assert.throws(() => migrateAgentConfig('claude', spec(), storage, source, true, undefined, true), /Unrecognized airk connection; existing configuration preserved for review/);
    const after = migrateAgentConfig('claude', spec(), storage, source, true, undefined, true, false, undefined, undefined, { verifiedLaunchers: [launcher], helper });
    const value = parse(after);
    assert.ok(after.startsWith('// keep this comment'));
    assert.deepEqual(value.mcpServers.other, { command: 'node', args: ['server.js'] });
    assert.deepEqual(value.mcpServers.airk, { type: 'http', url, headers: { Authorization: 'Bearer ${AIRK_MCP_TOKEN_' + id.toUpperCase() + '}', 'X-AIRK-Response-Mode': 'terse' },
        headersHelper: credentialHelperCommand(helper, id), disabled: true });
    assert.equal(after.includes('launcher'), false, 'no stdio or launcher command remains');
    assert.equal(migrateAgentConfig('claude', spec(), storage, after, true, undefined, true, false, undefined, undefined, { verifiedLaunchers: [launcher], helper }), after, 'idempotent');
    // Codex: the exact TOML table, with an unrelated server and a disabled flag.
    const toml = `model = "keep-model"\n# AIRK connection\n${userCodexToml(projectRoot).trimEnd()}\nenabled = false\n\n[mcp_servers.other]\ncommand = "node"\n`;
    assert.throws(() => migrateAgentConfig('codex', spec(), storage, toml, true, undefined, true), /Unrecognized airk connection/);
    const codex = migrateAgentConfig('codex', spec(), storage, toml, true, undefined, true, false, undefined, undefined, { verifiedLaunchers: [launcher], helper });
    const rows = parseTomlDeclarations(codex), airk = JSON.parse(JSON.stringify(rows.find(row => row.serverKey === 'airk')!.fields));
    assert.deepEqual(airk, { url, http_headers_helper: credentialHelperCommand(helper, id), enabled: false, http_headers: { 'X-AIRK-Response-Mode': 'terse' } });
    assert.deepEqual(JSON.parse(JSON.stringify(rows.find(row => row.serverKey === 'other')!.fields)), { command: 'node' });
    assert.ok(codex.startsWith('model = "keep-model"\n# AIRK connection\n'));
    assert.equal(httpConfigurationConnectionId('codex', airk), id);
    assert.equal(airkHttpDeclaration('codex', airk, id), true);
});

test('foreign entries stay refused even with verified launchers', () => {
    const verified = { verifiedLaunchers: [launcher], helper };
    const foreign = JSON.stringify({ mcpServers: { airk: { command: path.join(userBase, 'someone-else', 'airk.exe'), args: ['--root', projectRoot, 'mcp'] } } });
    assert.throws(() => migrateAgentConfig('claude', spec(), storage, foreign, true, undefined, true, false, undefined, undefined, verified), /Unrecognized airk connection/);
    const otherRoot = JSON.stringify({ mcpServers: { airk: { command: launcher, args: ['--root', path.resolve('another-project'), 'mcp'] } } });
    assert.throws(() => migrateAgentConfig('claude', spec(), storage, otherRoot, true, undefined, true, false, undefined, undefined, verified), /Unrecognized airk connection/);
    const token = JSON.stringify({ mcpServers: { airk: { type: 'http', url, headers: { Authorization: 'Bearer hard-coded-secret' } } } });
    assert.throws(() => migrateAgentConfig('claude', spec(), storage, token, true, undefined, true, false, undefined, undefined, { ...verified, staleHttpIds: new Set(['e'.repeat(32)]) }), /Unrecognized airk connection/);
});

test('an HTTPS entry whose connection the native owner no longer knows is replaced only when reported stale', () => {
    const dead = 'e'.repeat(32), environment = 'AIRK_MCP_TOKEN_' + dead.toUpperCase();
    const claude = JSON.stringify({ mcpServers: { airk: { type: 'http', url, headers: { Authorization: `Bearer \${${environment}}`, 'X-AIRK-Response-Mode': 'full' } }, other: { command: 'node' } } });
    assert.throws(() => migrateAgentConfig('claude', spec(), storage, claude, true, undefined, true), /Unrecognized airk connection/);
    const after = parse(migrateAgentConfig('claude', spec(), storage, claude, true, undefined, true, false, undefined, undefined, { staleHttpIds: new Set([dead]), helper }));
    assert.equal(httpConfigurationConnectionId('claude', after.mcpServers.airk), id);
    assert.equal(after.mcpServers.airk.headers['X-AIRK-Response-Mode'], 'full', 'the response preference is kept');
    assert.deepEqual(after.mcpServers.other, { command: 'node' });
    const copilot = JSON.stringify({ servers: { airk: { type: 'http', url: oldUrl, headers: { Authorization: `Bearer \${input:airkToken_${dead}}` } } }, inputs: [vscodeCredentialInputForId(dead)] });
    const replaced = parse(migrateAgentConfig('copilot', spec(), storage, copilot, true, undefined, true, false, undefined, undefined, { staleHttpIds: new Set([dead]) }));
    assert.deepEqual(replaced.inputs.map((input: { id: string }) => input.id), ['airkToken_' + id], 'the stale credential input is removed');
    const codex = `[mcp_servers.airk]\nurl = "${oldUrl}"\nbearer_token_env_var = "${environment}"\n`;
    const rewritten = parseTomlDeclarations(migrateAgentConfig('codex', spec(), storage, codex, true, undefined, true, false, undefined, undefined, { staleHttpIds: new Set([dead]), helper }))[0].fields;
    assert.equal(rewritten.bearer_token_env_var, undefined); assert.equal(credentialHelperConnectionId(rewritten.http_headers_helper), id);
});

test('the credential helper is a one-shot native command with shell-safe paths; refresh follows the current runtime image', () => {
    const command = credentialHelperCommand(helper, id);
    assert.equal(command, `${helper.executable} runtime http-credential --install-dir ${install} --connection-id ${id} --format header-json`);
    assert.equal(credentialHelperConnectionId(command), id);
    const spaced = { executable: path.join(userBase, 'Program Files', 'airk.exe'), installDir: install };
    assert.equal(credentialHelperCommand(spaced, id).startsWith(`"${spaced.executable}" runtime`), true);
    assert.equal(credentialHelperConnectionId(credentialHelperCommand(spaced, id)), id);
    for (const unsafe of ['C:\\a"b\\airk.exe', path.join(userBase, '%PATH%', 'airk.exe'), path.join(userBase, '$HOME', 'airk.exe'), '\\\\server\\share\\airk.exe', 'relative/airk.exe']) {
        if (!path.isAbsolute(unsafe) && !unsafe.startsWith('\\\\')) { assert.throws(() => credentialHelperCommand({ executable: unsafe, installDir: install }, id)); continue; }
        assert.throws(() => credentialHelperCommand({ executable: unsafe, installDir: install }, id), /http_credential_helper_path_unsupported/, unsafe);
    }
    assert.equal(credentialHelperConnectionId('node evil.js --connection-id ' + id), undefined);
    assert.deepEqual(httpServerFields('codex', info, helper), { url, http_headers_helper: command, http_headers: { 'X-AIRK-Response-Mode': 'terse' } });
    assert.deepEqual(httpServerFields('codex', info), { url, bearer_token_env_var: 'AIRK_MCP_TOKEN_' + id.toUpperCase(), http_headers: { 'X-AIRK-Response-Mode': 'terse' } });
    assert.equal((httpServerFields('claude', info, helper) as { headersHelper: string }).headersHelper, command);
    assert.equal('headersHelper' in httpServerFields('vscode', info, helper), false, 'VS Code uses its credential input command');
    assert.equal(airkHttpDeclaration('claude', { type: 'http', url, headers: { Authorization: 'Bearer ${AIRK_MCP_TOKEN_' + id.toUpperCase() + '}' }, headersHelper: credentialHelperCommand(helper, 'f'.repeat(32)) }, id), false,
        'a helper for another connection is not AIRK\u2019s shape');
    // An endpoint refresh keeps the helper; a new runtime image replaces its executable.
    const moved = { ...helper, executable: path.join(install, 'runtimes', 'cd'.repeat(32), win ? 'airk.exe' : 'airk') };
    const source = `[mcp_servers.airk]\nurl = "${oldUrl}"\nhttp_headers_helper = ${JSON.stringify(command)}\n`;
    const kept = parseTomlDeclarations(refreshHttpAgentConfig('codex', source, info, oldUrl))[0].fields;
    assert.equal(kept.http_headers_helper, command); assert.equal(kept.bearer_token_env_var, undefined); assert.equal(kept.url, url);
    const followed = parseTomlDeclarations(refreshHttpAgentConfig('codex', source, info, oldUrl, 'airk', undefined, moved))[0].fields;
    assert.equal(followed.http_headers_helper, credentialHelperCommand(moved, id));
});

test('only the removed installation\u2019s VS Code trust records are retired', () => {
    const trust = path.resolve('fixture-storage', 'http-trust');
    const retire = replacedHostArtifacts({
        mementoKeys: ['airk.http.connection.1.abc', 'airk.http.legacy.1.def', 'airk.http.union.id.1.x', 'airk.setupReview.1', 'airk.reconnect.ledger.1'],
        environment: [{ name: 'AIRK_MCP_TOKEN_' + 'A'.repeat(32), replace: true, value: 'airk1.x' }, { name: 'NODE_EXTRA_CA_CERTS', replace: true, value: path.join(trust, 'old.pem') },
            { name: 'CODEX_CA_CERTIFICATE', replace: true, value: path.join(trust, 'kept.pem') }, { name: 'PATH', replace: false, value: '/bin' }, { name: 'NODE_EXTRA_CA_CERTS_OTHER', replace: true, value: path.join(trust, 'old.pem') }],
        bundles: [{ file: path.join(trust, 'old.pem'), certificates: ['d'.repeat(64)] }, { file: path.join(trust, 'kept.pem'), certificates: ['0'.repeat(64)] }, { file: path.resolve('elsewhere.pem'), certificates: ['d'.repeat(64)] }],
    }, references, trust);
    assert.deepEqual(retire, { mementos: ['airk.http.connection.1.abc', 'airk.http.legacy.1.def', 'airk.http.union.id.1.x'], environment: ['AIRK_MCP_TOKEN_' + 'A'.repeat(32), 'NODE_EXTRA_CA_CERTS'],
        bundles: [path.join(trust, 'old.pem')], retained: [path.join(trust, 'kept.pem')] });
    assert.deepEqual(replacedHostArtifacts({ mementoKeys: [], environment: [], bundles: [{ file: path.join(trust, 'old.pem'), certificates: ['d'.repeat(64)] }] }, { ...references, caSha256: null }, trust).bundles, [],
        'without the removed CA digest no bundle is deleted');
});

test('Reconnect groups entries by recorded folder and names the exact host action', () => {
    const groups = reconnectGroups([{ root: win ? 'D:\\Work' : '/work', n: 1 }, { root: win ? 'd:\\work' : '/work', n: 2 }, { root: undefined, n: 3 }, { root: win ? 'D:\\Other' : '/other', n: 4 }]);
    assert.deepEqual(groups.map(group => group.entries.map(entry => entry.n)), [[1, 2], [3], [4]]);
    assert.match(hostReconnectAction('claude'), /\/mcp/); assert.match(hostReconnectAction('claude'), /Reconnect/);
    assert.match(hostReconnectAction('codex'), /Restart Codex/); assert.match(hostReconnectAction('copilot'), /Restart Server/);
});

// ---- Extension host: capture before --apply, proactive warning, Reconnect.

function editor(f: Awaited<ReturnType<typeof serviceTestHost>>, destination: string, initial: string) {
    const document = { uri: { fsPath: destination }, isDirty: false, version: 1, text: initial, saves: 0, getText() { return this.text; }, positionAt: (offset: number) => offset,
        async save() { this.saves++; await fs.writeFile(destination, this.text); return true; } };
    f.stub.workspace.openTextDocument = async () => document;
    f.stub.workspace.applyEdit = async (edit: { after: string }) => { document.text = edit.after; document.version++; return true; };
    return document;
}
const controllerOf = (f: Awaited<ReturnType<typeof serviceTestHost>>) => f.context.subscriptions.find((value: any) => typeof value.reconnectAgents === 'function');

/** Seed the canonical native disposal read independently of the host Mementos.
 * Other IDs get a complete negative ownership observation, never a name guess. */
function replacementOwnership(context:TestContext,f:Awaited<ReturnType<typeof serviceTestHost>>,connection:HttpConnectionInfo,token=`airk1.${connection.connection_id}.`+'a'.repeat(64)){
    const proof=predecessorHttpFixture(f.binding.controlDir,f.binding.runtimeSha256,connection.connection_id);
    proof.endpoint={...structuredClone(connection.endpoint)};proof.namespace=connection.endpoint.namespace;
    proof.ca_certificate_file=connection.ca_certificate_file;proof.credential_file=connection.credential_file;
    proof.credential_sha256=sha256(token);proof.qualification.current_namespace=connection.endpoint.namespace;
    proof.snapshot={...proof.snapshot,endpoint:proof.endpoint,namespace:proof.namespace,
        owners:[{connection_id:connection.connection_id,ownership:'expired',credential_sha256:proof.credential_sha256}]};
    context.mock.method(RuntimeHost.prototype,'lookupPredecessorHttpOwnership',async(...[current,id,scope]:Parameters<RuntimeHost['lookupPredecessorHttpOwnership']>)=>{
        assert.equal(current.runtimeSha256,proof.predecessor);assert.equal(scope.predecessor,proof.predecessor);assert.equal(scope.controlDir,proof.control_dir);
        const owned=id===connection.connection_id;
        return {...structuredClone(proof),connection_id:id,status:owned?'owned':'not_owned',ownership:owned?proof.ownership:null,
            credential_sha256:owned?proof.credential_sha256:null,ca_certificate_file:owned?proof.ca_certificate_file:null,credential_file:owned?proof.credential_file:null} as any;
    });
}

test('replacement retires only the replaced installation HTTP records and terminal credentials',async context=>{
    const f=await serviceTestHost(context,{automatic:false,registered:false,bundledVersion:'1.8.23',runtimeVersion:'1.8.23'});
    try{
        const oldId='a'.repeat(32),otherId='b'.repeat(32),oldKey='airk.http.legacy.1.'+'a'.repeat(64),otherKey='airk.http.legacy.1.'+'b'.repeat(64);
        const opaqueKey='airk.http.connection.1.'+'c'.repeat(64),otherInstall=path.join(f.root,'other-installation');
        const oldInfo:HttpConnectionInfo={...structuredClone(info),connection_id:oldId,ca_certificate_file:path.join(f.binding.controlDir,'http-1','ca.pem'),
            credential_file:path.join(f.binding.controlDir,'http-1',`credential-${oldId}.token`)};
        const otherInfo:HttpConnectionInfo={...structuredClone(info),connection_id:otherId,
            endpoint:{...info.endpoint,installation:'f'.repeat(64),url:'https://127.0.0.1:57499/mcp',ca_sha256:'e'.repeat(64)},
            ca_certificate_file:path.join(otherInstall,'control','http-1','ca.pem'),credential_file:path.join(otherInstall,'control','http-1',`credential-${otherId}.token`)};
        f.values.set(oldKey,oldInfo);f.values.set(otherKey,otherInfo);f.values.set(opaqueKey,'c'.repeat(32));
        const oldVariable='AIRK_MCP_TOKEN_'+oldId.toUpperCase(),otherVariable='AIRK_MCP_TOKEN_'+otherId.toUpperCase();
        const environment=new Map([[oldVariable,{type:1,value:`airk1.${oldId}.`+'1'.repeat(64)}],[otherVariable,{type:1,value:`airk1.${otherId}.`+'2'.repeat(64)}]]);
        replacementOwnership(context,f,oldInfo,environment.get(oldVariable)!.value);
        f.context.environmentVariableCollection={forEach:(visit:(name:string,value:{type:number;value:string})=>void)=>environment.forEach((value,name)=>visit(name,value)),
            get:(name:string)=>environment.get(name),delete:(name:string)=>environment.delete(name),replace:(name:string,value:string)=>environment.set(name,{type:1,value}),prepend(){}};
        const otherBefore=JSON.stringify({record:f.values.get(otherKey),opaque:f.values.get(opaqueKey),credential:environment.get(otherVariable)});
        f.state.replacementPlan={required:true,expected_predecessor:'ef'.repeat(32),reason:'replace_before_1_9_0',installed_version:'1.8.23',locations:[],
            client_references:{executables:[],http_endpoint:oldInfo.endpoint.url,ca_sha256:oldInfo.endpoint.ca_sha256}};
        await f.commands.get('airk.installCli')!();
        assert.equal(JSON.stringify({record:f.values.get(otherKey),opaque:f.values.get(opaqueKey),credential:environment.get(otherVariable)}),otherBefore,
            'replacing one installation preserves another installation and records whose owner cannot be proven');
        assert.equal(f.values.has(oldKey),false,'the matching owned predecessor record is retired');
        assert.equal(environment.has(oldVariable),false,'only the matching predecessor credential reference is retired');
        assert.equal(f.registrations.size,0);assert.equal(f.writes.some(value=>value.reserve_project||value.enqueue||value.configure_watch||value.catalog_index_setup_apply),false);
    }finally{await f.close();}
});

test('replacement retains retry evidence and reports actual trust-bundle removals when unlink is refused',async context=>{
    const f=await serviceTestHost(context,{automatic:false,registered:false,bundledVersion:'1.8.23',runtimeVersion:'1.8.23'});
    try{
        const trust=path.join(f.context.globalStorageUri.fsPath,'http-trust'),bundle=path.join(trust,'predecessor.pem');
        await fs.mkdir(trust,{recursive:true});await fs.writeFile(bundle,ca);
        const issuer=createHash('sha256').update(new X509Certificate(ca).raw).digest('hex'),source=f.folders[0].uri.fsPath;
        const connectionId='d'.repeat(32),connection:HttpConnectionInfo={...structuredClone(info),connection_id:connectionId,
            endpoint:{...info.endpoint,url:oldUrl,ca_sha256:issuer},ca_certificate_file:path.join(f.binding.controlDir,'http-1','ca.pem'),
            credential_file:path.join(f.binding.controlDir,'http-1',`credential-${connectionId}.token`)};
        replacementOwnership(context,f,connection);
        const stat=await fs.lstat(bundle,{bigint:true});
        controllerOf(f).ownedHttpTrustBundles.set(bundle,{sha256:sha256(ca),dev:stat.dev.toString(),ino:stat.ino.toString(),size:Number(stat.size),
            namespace:connection.endpoint.namespace,installation:connection.endpoint.installation,connectionId});
        const sourceFile=path.join(source,'untouched.txt');await fs.writeFile(sourceFile,'source bytes stay unchanged\n');
        f.state.replacementPlan={required:true,expected_predecessor:'ef'.repeat(32),reason:'replace_before_1_9_0',installed_version:'1.8.23',locations:[],
            client_references:{executables:[],http_endpoint:oldUrl,ca_sha256:issuer}};
        // The canonical exact-publication removal does not yield between its
        // final identity check and unlink. Inject refusal at that real API.
        const filesystem:typeof import('node:fs')=require('node:fs'),unlink=filesystem.unlinkSync;let refusals=0;
        const refusedUnlink=context.mock.method(filesystem,'unlinkSync',(filename:Parameters<typeof filesystem.unlinkSync>[0])=>{
            if(String(filename)===bundle){refusals++;throw Object.assign(new Error('fictional owned trust-bundle sharing refusal'),{code:'EACCES'});}
            return unlink(filename);
        });
        await f.commands.get('airk.installCli')!();
        assert.equal(refusals,1,'the actual owned bundle unlink reaches the injected refusal once');
        const retained=await fs.readFile(bundle,'utf8');assert.equal(retained,ca);
        assert.ok([...f.values.keys()].some(key=>key.startsWith('airk.installOperation.1.')),
            'failed owned cleanup retains the exact operation and client references for retry');
        assert.equal(controllerOf(f).installationFailed,true,'installation cannot claim completed host cleanup');
        assert.ok(!f.calls.some(line=>/Retired the removed installation.*1 trust bundle\(s\)/.test(line)),f.calls.join('\n'));
        assert.ok(f.calls.some(line=>line.includes(bundle)&&/refus|Could not remove|retained|incomplete/i.test(line)),f.calls.join('\n'));
        const pending=[...f.values.entries()].find(([key])=>key.startsWith('airk.installOperation.1.'))![1] as any;
        assert.equal(pending.hostCleanup.complete,false);assert.deepEqual(pending.hostCleanup.removed,[]);
        assert.deepEqual(pending.hostCleanup.retained,[bundle]);
        assert.equal(pending.hostArtifacts.artifacts.bundles[0].sha256,sha256(ca),'retry retains the exact original publication digest');
        assert.equal(pending.hostArtifacts.proofs[0].connection_id,connectionId,'retry retains native disposal proof captured before uninstall');
        refusedUnlink.mock.restore();
        controllerOf(f).ownedHttpTrustBundles.clear();
        context.mock.method(RuntimeHost.prototype,'lookupPredecessorHttpOwnership',async()=>{throw new Error('removed predecessor control must not be reread');});
        f.state.replacementPlan={required:false,expected_predecessor:'ef'.repeat(32),reason:'installation_already_replaced',locations:[]};
        await f.commands.get('airk.installCli')!();
        await assert.rejects(fs.lstat(bundle),{code:'ENOENT'});
        assert.equal(f.calls.filter(line=>line==='installation-replaced').length,1,'host cleanup retry does not repeat native uninstall');
        assert.equal([...f.values.keys()].some(key=>key.startsWith('airk.installOperation.1.')),false,'successful exact cleanup releases its operation');
        assert.equal(await fs.readFile(sourceFile,'utf8'),'source bytes stay unchanged\n');
        assert.equal(f.writes.some(value=>value.reserve_project||value.enqueue||value.configure_watch||value.catalog_index_setup_apply),false);
    }finally{await f.close();}
});

test('shared automatic operation retains every immutable host capture after its Memento hint is lost',async context=>{
    const f=await serviceTestHost(context,{automatic:false,registered:false,bundledVersion:'1.8.23',runtimeVersion:'1.8.23'});
    try{
        const controller=controllerOf(f),observed=await controller.runtime().discover(),operation='a'.repeat(64),connectionId='b'.repeat(32);
        const connection:HttpConnectionInfo={...structuredClone(info),connection_id:connectionId,ca_certificate_file:path.join(f.binding.controlDir,'http-1','ca.pem'),credential_file:path.join(f.binding.controlDir,'http-1',`credential-${connectionId}.token`)};
        replacementOwnership(context,f,connection);
        const first='airk.http.connection.1.'+'1'.repeat(64),second='airk.http.connection.1.'+'2'.repeat(64);
        f.values.set(first,connectionId);
        const captureA=await controller.captureReplacementHostArtifacts(observed,undefined);
        await controller.saveReplacementRecord(observed,operation,{generation:operation,expectedPredecessor:'ef'.repeat(32),hostArtifacts:captureA},true);
        f.values.delete(first);f.values.set(second,connectionId);
        const captureB=await controller.captureReplacementHostArtifacts(observed,undefined);
        await controller.saveReplacementRecord(observed,operation,{generation:operation,expectedPredecessor:'ef'.repeat(32),hostArtifacts:captureB},true);
        f.values.set(first,connectionId);
        for(const key of [...f.values.keys()])if(key.startsWith('airk.installOperation.1.'))f.values.delete(key);
        assert.equal(controller.replacementCaptures(observed).length,2,'same-operation publications never overwrite one another');
        context.mock.method(RuntimeHost.prototype,'replacementPlan',async()=>({required:false,expected_predecessor:'ef'.repeat(32),reason:'installation_already_replaced',locations:[]}));
        const retry=await controller.replacementOperation(observed,false);assert.equal(retry.operationId,operation);
        await controller.replacePreviousInstallation(observed,retry.operationId,retry.generationId);
        assert.equal(f.values.has(first),false);assert.equal(f.values.has(second),false);
        assert.equal(controller.replacementCaptures(observed).length,0,'only successfully retired exact captures are removed');
        assert.equal(f.calls.includes('installation-replaced'),false);assert.equal(f.writes.length,0);
    }finally{await f.close();}
});

test('replacement refuses unsupported predecessor ownership reads before destructive native apply',async context=>{
    const f=await serviceTestHost(context,{automatic:false,registered:false,bundledVersion:'1.8.23',runtimeVersion:'1.8.23'});
    try{
        const key='airk.http.connection.1.'+'a'.repeat(64),value='b'.repeat(32);f.values.set(key,value);
        f.state.replacementPlan={required:true,expected_predecessor:'ef'.repeat(32),reason:'replace_before_1_9_0',locations:[],client_references:{executables:[],http_endpoint:null,ca_sha256:null}};
        context.mock.method(RuntimeHost.prototype,'lookupPredecessorHttpOwnership',async()=>{throw new ServiceError('unsupported_predecessor_cleanup_purpose');});
        await f.commands.get('airk.installCli')!();
        assert.equal(f.values.get(key),value);assert.equal(f.calls.includes('installation-replaced'),false);
        assert.equal(f.state.ensured,0);assert.equal(controllerOf(f).installationFailed,true);
        assert.ok([...f.values.keys()].some(value=>value.startsWith('airk.installOperation.1.')));
        assert.equal(f.writes.length,0,'an unsupported read creates no index, watch, grant, or job');
    }finally{await f.close();}
});

test('replacement retains a changed predecessor host preimage and its exact retry operation',async context=>{
    const f=await serviceTestHost(context,{automatic:false,registered:false,bundledVersion:'1.8.23',runtimeVersion:'1.8.23'});
    try{
        const connectionId='e'.repeat(32),key='airk.http.connection.1.'+'d'.repeat(64);
        const connection:HttpConnectionInfo={...structuredClone(info),connection_id:connectionId,ca_certificate_file:path.join(f.binding.controlDir,'http-1','ca.pem'),credential_file:path.join(f.binding.controlDir,'http-1',`credential-${connectionId}.token`)};
        replacementOwnership(context,f,connection);f.values.set(key,connection);
        f.state.replacementPlan={required:true,expected_predecessor:'ef'.repeat(32),reason:'replace_before_1_9_0',locations:[],client_references:{executables:[],http_endpoint:connection.endpoint.url,ca_sha256:connection.endpoint.ca_sha256}};
        const changed={...connection,connection_id:'f'.repeat(32)},apply=RuntimeHost.prototype.replaceInstallation;
        context.mock.method(RuntimeHost.prototype,'replaceInstallation',async function(this:RuntimeHost,...args:Parameters<RuntimeHost['replaceInstallation']>){
            const result=await apply.apply(this,args);f.values.set(key,changed);return result;
        });
        await f.commands.get('airk.installCli')!();
        assert.deepEqual(f.values.get(key),changed,'a concurrent publication never inherits predecessor ownership from its key');
        const pending=[...f.values.entries()].find(([name])=>name.startsWith('airk.installOperation.1.'))![1] as any;
        assert.equal(pending.hostCleanup.complete,false);assert.deepEqual(pending.hostCleanup.removed,[]);
        assert.ok(pending.hostCleanup.retained.includes('memento:'+key));
        assert.equal(pending.hostArtifacts.artifacts.mementos[0].sha256,sha256(JSON.stringify(connection)));
        assert.equal(f.state.ensured,0);assert.equal(f.writes.length,0);
    }finally{await f.close();}
});

test('replacement records agent entries before --apply, retires the removed installation\u2019s records, and warns once per findings digest', async context => {
    const f = await serviceTestHost(context, { automatic: false, registered: false, bundledVersion: '1.8.19', runtimeVersion: '1.8.11', installedVersion: '1.8.19' });
    try {
        await controllerOf(f).refreshAgentAlerts(); // The activation check finishes before the fixture files exist.
        const root = f.folders[0].uri.fsPath, destination = path.join(root, '.mcp.json');
        await fs.writeFile(destination, userMcpJson(root));
        const user = path.join(f.root, 'home', '.claude.json'), userSource = JSON.stringify({ projects: { [root]: { mcpServers: { airk: { command: launcher, args: ['--root', namespaced(root), 'mcp'] } } } } });
        context.mock.method(globalInventory, 'inventoryGlobalAgentConfigs', async () => ({ files: [{ agent: 'claude', destination: user, source: userSource, sha256: sha256(userSource), declarations: globalInventory.globalConfigDeclarations('claude', userSource) }], checked: [user], blockers: [] }));
        f.values.set('airk.http.connection.1.' + 'a'.repeat(64), 'b'.repeat(32));
        replacementOwnership(context,f,{...structuredClone(info),connection_id:'b'.repeat(32),endpoint:{...info.endpoint,ca_sha256:'d'.repeat(64)},
            ca_certificate_file:path.join(f.binding.controlDir,'http-1','ca.pem'),credential_file:path.join(f.binding.controlDir,'http-1',`credential-${'b'.repeat(32)}.token`)});
        f.state.replacementPlan = { required: true, expected_predecessor:'ef'.repeat(32),reason: 'replace_before_1_9_0', installed_version: '1.8.11', locations: ['fixture-previous'],
            client_references: { executables: [launcher], http_endpoint: null, ca_sha256: 'd'.repeat(64) } };
        const order: string[] = [], original = RuntimeHost.prototype.replaceInstallation;
        context.mock.method(RuntimeHost.prototype, 'replaceInstallation', async function (this: RuntimeHost, ...args: Parameters<RuntimeHost['replaceInstallation']>) {
            order.push(f.values.has(RECONNECT_LEDGER_KEY) ? 'captured' : 'not_captured', f.values.has('airk.http.connection.1.' + 'a'.repeat(64)) ? 'records_present' : 'records_gone');
            return original.apply(this, args);
        });
        await f.commands.get('airk.installCli')!();
        assert.deepEqual(order, ['captured', 'records_present'], 'the ledger is written before --apply; old records are retired after it');
        const ledger = validateReconnectLedger(f.values.get(RECONNECT_LEDGER_KEY));
        assert.deepEqual(ledger.entries.map(entry => [entry.scope, entry.destination, entry.serverKey, entry.root]).sort(),
            [['project', destination, 'airk', root], ['user', user, 'airk', root]].sort());
        assert.equal(ledger.entries.find(entry => entry.scope === 'project')!.fileSha256, sha256(userMcpJson(root)));
        assert.equal(f.values.has('airk.http.connection.1.' + 'a'.repeat(64)), false);
        assert.ok(f.calls.some(call => String(call).includes('recorded them for Reconnect')), f.calls.join('\n'));
        assert.equal(await fs.readFile(destination, 'utf8'), userMcpJson(root), 'capture never edits a file');
        const controller = controllerOf(f);
        await controller.refreshAgentAlerts(); await controller.refreshAgentAlerts();
        const warnings = f.notices.filter(notice => notice.actions.includes('Reconnect'));
        assert.equal(warnings.length, 1, 'one notification per findings digest: ' + JSON.stringify(warnings));
        assert.deepEqual(warnings[0].actions, ['Reconnect', 'Show details']);
        const alerts = controller.agentConfigAlerts();
        assert.equal(alerts.total, 2); assert.deepEqual(alerts.findings.map((finding: { problem: string }) => finding.problem), ['stdio_missing_command', 'stdio_missing_command']);
        assert.equal(controller.explorerHost().agentConfigAlerts()?.digest, alerts.digest, 'the Explorer banner reads the same findings');
        assert.equal(f.writes.some(value => 'catalog_index_setup_apply' in value || 'reserve_project' in value), false, 'no index is created');
    } finally { await f.close(); }
});

/**
 * Synthetic per-stage failure boundary. The separate realScopeChain retains
 * actual Search scope Apply, grant review and combined-spec construction.
 */
test('replacement captures the discovered default launcher even when its image is already missing from the native roster',async context=>{
    const f=await serviceTestHost(context,{automatic:false,registered:false,bundledVersion:'1.8.22',runtimeVersion:'1.8.20'});
    try{
        await controllerOf(f).refreshAgentAlerts();
        const {destination,source}=await brokenProject(f);
        f.state.replacementPlan={required:true,expected_predecessor:'ef'.repeat(32),reason:'replace_before_1_9_0',installed_version:'1.8.20',locations:[],client_references:{executables:[],http_endpoint:null,ca_sha256:null}};
        await f.commands.get('airk.installCli')!();
        const ledger=validateReconnectLedger(f.values.get(RECONNECT_LEDGER_KEY));
        assert.equal(ledger.entries.filter(entry=>entry.destination===destination).length,1);
        assert.equal(await fs.readFile(destination,'utf8'),source,'capture leaves the stale declaration intact until reviewed reconnect');
        assert.equal(f.writes.some(value=>value.catalog_index_setup_apply||value.reserve_project||value.enqueue),false);
    }finally{await f.close();}
});

function reviewedChain(context: TestContext, f: Awaited<ReturnType<typeof serviceTestHost>>, root: string, options: {
    existing?: boolean; scope?: () => unknown } = {}) {
    const controller = controllerOf(f), native = controller.console.controller, namespace = '34'.repeat(16), index = 'idx_' + '7a'.repeat(16);
    const calls: string[] = [], selections: unknown[] = [];
    let scoped = false;
    const session = { schema: 1 as const, context_key: '9b'.repeat(32), namespace, expected_catalog_revision: 3, grant_id: 'grt_' + '8c'.repeat(16), expected_grant_revision: 1, index_id: index, project_ids: ['prj_' + '6d'.repeat(16)] };
    const scopeSpec: LaunchSpec = { command: f.binding.executable, root, dataDir: path.join(f.root, 'data', 'reconnect'), collection: 'reconnect', name: path.basename(root),
        service: { installDir: f.binding.installDir, project: '5e'.repeat(32), direct: true }, catalogSession: session };
    const descriptor = (revision: number) => ({ response: { status: 'catalog_host_context', descriptor: { schema: 1, namespace, host: '56'.repeat(32), key: '9b'.repeat(32), catalog_revision: revision, context: null } } });
    let revision = 1;
    context.mock.method(controller, 'prepareWindowContext', async () => { calls.push('window_context'); });
    context.mock.method(controller, 'currentWindowContext', () => descriptor(revision));
    const backing = { binding: '5e'.repeat(32), root, data_root: scopeSpec.dataDir, collection: 'reconnect' };
    context.mock.method(controller, 'currentCatalogScope', () => scoped ? { spec: scopeSpec, selection: { session, index: { id: index, name: 'Existing', backing }, host: '56'.repeat(32) } } : undefined);
    const read = native.catalogRead.bind(native);
    context.mock.method(native, 'catalogRead', async (method: Record<string, unknown>) => {
        if (!('catalog_page' in method)) { return read(method); }
        calls.push('index_lookup');
        return { status: 'catalog_page', page: { namespace, revision, continuation: null, items: options.existing
            ? [{ kind: 'index', value: { id: index, name: 'Existing', definition_revision: 1, source_revision: 1, backing, tombstone: false, source_import: 'complete' } }] : [] } };
    });
    context.mock.method(native, 'catalogIndexSetupDraft', async () => { calls.push('setup_draft'); throw new Error('Reconnect must not draft index setup'); });
    context.mock.method(native, 'catalogIndexSetupApply', async () => { calls.push('setup_apply'); throw new Error('Reconnect must not apply index setup'); });
    context.mock.method(controller, 'applySearchScope', async (selection: unknown) => {
        calls.push('access_grant'); selections.push(selection);
        if (options.scope) { return options.scope(); }
        revision = 3; scoped = true; return { status: 'search_scope_selected' };
    });
    context.mock.method(RuntimeHost.prototype, 'httpTrust', async () => ({ outcome: 'trusted', trusted: true, ca_sha256: 'd'.repeat(64) }));
    return { calls, selections, index, controller };
}
async function brokenProject(f: Awaited<ReturnType<typeof serviceTestHost>>) {
    const root = f.folders[0].uri.fsPath, destination = path.join(root, '.mcp.json');
    const universal = path.join(f.root, 'launcher', win ? 'airk-launcher.exe' : 'airk-launcher');
    const source = `// my notes\n${JSON.stringify({ mcpServers: { other: { command: 'node', args: ['server.js'] }, airk: { command: universal, args: ['--root', namespaced(root), 'mcp', '--explorer-descriptor', descriptor] } } }, null, 2)}\n`;
    await fs.writeFile(destination, source);
    return { root, destination, source, document: editor(f, destination, source) };
}

/** Keep the real Apply -> grant review -> combined LaunchSpec chain. Only the
 * native metadata/mutation boundary is synthetic, so singleton combined scopes
 * cannot be replaced with the obsolete catalogSession shape by the test. */
async function realScopeChain(context: TestContext, f: Awaited<ReturnType<typeof serviceTestHost>>, root: string) {
    const controller = controllerOf(f);
    // Empty installations have no host binding until the real console and host
    // have connected. Establish both before supplying synthetic catalog rows;
    // keep their real context/generation/connection fences throughout Apply.
    await controller.console.controller.refresh();
    assert.equal(controller.console.controller.state.connected, true, controller.console.controller.state.message);
    await controller.refresh();
    assert.equal(controller.currentBinding(), true, f.calls.join('\n'));
    const native = controller.console.controller;
    const namespace = '34'.repeat(16), host = '56'.repeat(32), indexId = 'idx_' + '7a'.repeat(16), sourceId = 'src_' + '2b'.repeat(16);
    const projects = new Map<string, CatalogObject>(), memberships: CatalogObject[] = [], mutations: CatalogObject[] = [];
    let revision = 1, contextKey = '9b'.repeat(32), grant: CatalogObject | undefined;
    const index = { id: indexId, name: 'Reviewed index', definition_revision: 1, source_revision: 1, source_import: 'complete', tombstone: false, policy: null,
        backing: { binding: '5e'.repeat(32), root: win ? path.toNamespacedPath(root) : root, data_root: path.join(f.root, 'data', 'existing'), collection: 'existing' } };
    const descriptor = () => ({ schema: 1, namespace, host, key: contextKey, catalog_revision: revision, context: null });
    context.mock.method(controller, 'openWorkbenchAfterSetup', async () => {});
    context.mock.method(native, 'catalogPending', () => []);
    context.mock.method(native, 'catalogRead', async (method: CatalogObject) => {
        if (method.catalog_host_context) {
            contextKey = String(catalogObject(method.catalog_host_context).key);
            return { status: 'catalog_host_context', descriptor: descriptor() };
        }
        if (method.catalog_detail) {
            const entity = catalogObject(catalogObject(method.catalog_detail).entity);
            const value = entity.kind === 'index' ? index : entity.kind === 'project' ? projects.get(String(entity.id))
                : entity.kind === 'grant' && grant ? { grant, provenance: { kind: 'user_reviewed_project_grant' } } : undefined;
            return { status: 'catalog_detail', detail: { namespace, revision, item: value ? { kind: entity.kind, value } : null } };
        }
        const request = catalogObject(catalogObject(method.catalog_page).request);
        const items = request.kind === 'indexes' ? [{ kind: 'index', value: index }]
            : request.kind === 'sources' ? [{ kind: 'source', value: { id: sourceId, index_id: indexId, canonical_path: index.backing.root, aliases: [], owner: true, enabled: true } }]
            : memberships.filter(member => member.project_id === request.project_id).map(value => ({ kind: 'membership', value }));
        return { status: 'catalog_page', page: { namespace, revision, items, continuation: null } };
    });
    const review = (operation: CatalogObject): CatalogReview => ({ request: { caller: namespace, key: { namespace, epoch: 1, nonce: namespace }, expected_catalog_revision: revision, operation }, response: {}, previewDigest: host, requestDigest: contextKey } as CatalogReview);
    context.mock.method(native, 'catalogPrepare', async (operation: CatalogObject) => review(operation));
    context.mock.method(native, 'catalogApply', async (prepared: CatalogReview) => {
        const operation = prepared.request.operation; mutations.push(operation);
        if (operation.operation === 'create_project') projects.set(String(operation.id), { id: operation.id, name: operation.name, definition_revision: 1, scope_revision: 1, defaults_revision: 1, default_indexes: [] });
        if (operation.operation === 'set_membership') {
            const member = catalogObject(operation.membership); memberships.push(member);
            projects.get(String(member.project_id))!.scope_revision = Number(projects.get(String(member.project_id))!.scope_revision) + 1;
        }
        if (operation.operation === 'create_grant') grant = catalogObject(operation.grant);
        return { status: 'catalog_applied', receipt: { catalog_revision: ++revision } };
    });
    context.mock.method(native, 'catalogGrantDraft', async (_key: string, selection: CatalogObject) => review({ operation: 'create_grant', grant: {
        id: 'grt_' + '6c'.repeat(16), host, revision: 1, revoked: false,
        memberships: memberships.filter(member => (selection.project_ids as string[]).includes(String(member.project_id))).map(membership => ({ membership, source_revision: 1, policy_epoch: 0, policy_digest: null })) } }));
    context.mock.method(native, 'catalogIndexSetupDraft', async () => { throw new Error('Reconnect must not draft index setup'); });
    context.mock.method(native, 'catalogIndexSetupApply', async () => { throw new Error('Reconnect must not apply index setup'); });
    context.mock.method(RuntimeHost.prototype, 'httpTrust', async () => ({ outcome: 'trusted', trusted: true, ca_sha256: 'd'.repeat(64) }));
    return { controller, index, indexId, mutations, descriptor, grant: () => grant };
}

test('Reconnect records interleaved project and user entries by identity after grouping their roots',async context=>{
    const f=await serviceTestHost(context,{automatic:false,registered:false,roots:2});
    try{
        await controllerOf(f).refreshAgentAlerts();
        const first=await brokenProject(f),secondRoot=f.folders[1].uri.fsPath,secondFile=path.join(secondRoot,'.mcp.json');
        const universal=path.join(f.root,'launcher',win?'airk-launcher.exe':'airk-launcher'),secondSource=userMcpJson(secondRoot,universal);
        const user=path.join(f.root,'home','config.toml'),userSource=userCodexToml(first.root,universal);
        await fs.writeFile(secondFile,secondSource);await fs.mkdir(path.dirname(user),{recursive:true});await fs.writeFile(user,userSource);
        context.mock.method(globalInventory,'inventoryGlobalAgentConfigs',async()=>{
            const source=await fs.readFile(user,'utf8');return {files:[{agent:'codex',destination:user,source,sha256:sha256(source),declarations:globalInventory.globalConfigDeclarations('codex',source)}],checked:[user],blockers:[]};
        });
        f.values.set(RECONNECT_LEDGER_KEY,captureReconnectLedger([
            {agent:'claude',scope:'project',destination:first.destination,source:first.source},
            {agent:'claude',scope:'project',destination:secondFile,source:secondSource},
            {agent:'codex',scope:'user',destination:user,source:userSource},
        ],{executables:[universal],httpEndpoint:null,caSha256:null},f.binding.installDir,1));
        const chain=await realScopeChain(context,f,first.root),controller=chain.controller;
        await fs.mkdir(path.dirname(f.binding.executable),{recursive:true});await fs.writeFile(f.binding.executable,'');
        const save=controller.saveMigrationConfiguration.bind(controller);
        context.mock.method(controller,'saveMigrationConfiguration',async(destination:string,before:string,after:string,check:()=>void)=>{
            const saved=await save(destination,before,after,check);
            if(destination===first.destination)await fs.writeFile(user,userCodexToml(first.root,process.execPath));
            return saved;
        });
        await f.commands.get('airk.reconnectAgents')!();
        const results=controller.agentConfigAlerts()?.results??[];
        assert.deepEqual(results.map((value:{file:string;status:string})=>[value.file,value.status]),[
            [first.destination,'awaiting_host_reconnect'],[user,'recovery_required'],[secondFile,'awaiting_index'],
        ],'the actual entrypoints group root1 before root2 and retain distinct outcomes');
        const ledger=validateReconnectLedger(f.values.get(RECONNECT_LEDGER_KEY));
        for(const entry of ledger.entries){
            const result=results.find((value:{file:string;serverKey:string})=>value.file===entry.destination&&value.serverKey===entry.serverKey);
            assert.ok(result,entry.destination);
            assert.deepEqual({status:entry.status,detail:entry.detail},{status:result.status,detail:result.detail},entry.destination);
        }
        assert.equal(await fs.readFile(secondFile,'utf8'),secondSource);
        assert.equal(await fs.readFile(user,'utf8'),userCodexToml(first.root,process.execPath),'the concurrent user edit is never overwritten');
        assert.equal(f.writes.some(value=>value.reserve_project||value.enqueue||value.configure_watch||value.catalog_index_setup_apply),false);
    }finally{await f.close();}
});

test('Reconnect recognizes the reviewed singleton combined adapter without depending on host setup', async context => {
    const f = await serviceTestHost(context, { registered: false });
    try {
        const controller = controllerOf(f), root = f.folders[0].uri.fsPath;
        const namespace = '34'.repeat(16), host = '56'.repeat(32), key = '9b'.repeat(32), project = 'prj_' + '6d'.repeat(16), index = 'idx_' + '7a'.repeat(16), grantId = 'grt_' + '6c'.repeat(16);
        const descriptor = { schema: 1, namespace, host, key, catalog_revision: 1, context: null };
        const detail = (kind: string, value: unknown) => ({ namespace, revision: 1, item: { kind, value } });
        const grant = { id: grantId, host, revision: 1, revoked: false, memberships: [{ membership: { project_id: project, index_id: index, revision: 1,
            scope: [{ source_root_id: 'src_' + '2b'.repeat(16), kind: 'subtree', path: '' }] }, source_revision: 1, policy_epoch: 0, policy_digest: null }] };
        const selection = reviewCombinedScopeSelection({ grant_id: grantId, expected_grant_revision: 1, project_ids: [project] }, descriptor,
            detail('grant', { grant, provenance: { kind: 'user_reviewed_project_grant' } }), [detail('index', { id: index, name: 'Existing', definition_revision: 1,
                source_revision: 1, source_import: 'complete', tombstone: false, policy: null,
                backing: { binding: '5e'.repeat(32), root: win ? path.toNamespacedPath(root) : root, data_root: path.join(f.root, 'data'), collection: 'existing' } })]);
        const selectedSpec = nativeScopeLaunchSpec(f.binding, selection);
        assert.equal(selectedSpec.catalogSession, undefined); assert.ok(selectedSpec.combinedSession);
        context.mock.method(controller, 'currentCatalogScope', () => ({ spec: selectedSpec, selection }));
        const result = controller.reconnectScope(root);
        assert.ok(result?.combinedSession, 'nativeScopeLaunchSpec emits a supported reviewed combined session for one index');
        assert.deepEqual(result.combinedSession.index_ids, [index]);
        assert.equal(f.writes.length, 0);
    } finally { await f.close(); }
});

test('Reconnect accepts the actual singleton combined session produced by Search scope Apply', async context => {
    const f = await serviceTestHost(context, { registered: false });
    try {
        await controllerOf(f).refreshAgentAlerts();
        const { root, destination, document } = await brokenProject(f), chain = await realScopeChain(context, f, root);
        await fs.mkdir(path.dirname(f.binding.executable), { recursive: true }); await fs.writeFile(f.binding.executable, '');
        await f.commands.get('airk.reconnectAgents')!();
        assert.deepEqual(chain.mutations.map(value => value.operation), ['create_project', 'set_membership', 'create_grant'], JSON.stringify(chain.controller.agentConfigAlerts()?.results));
        assert.equal(chain.controller.currentCatalogScope().spec.catalogSession, undefined);
        assert.ok(chain.controller.currentCatalogScope().spec.combinedSession);
        assert.deepEqual(chain.controller.agentConfigAlerts().results.map((value: { status: string }) => value.status), ['awaiting_host_reconnect']);
        assert.equal(document.saves, 1);
        const saved = parse(await fs.readFile(destination, 'utf8'));
        assert.deepEqual(saved.mcpServers.other, { command: 'node', args: ['server.js'] });
        assert.equal(saved.mcpServers.airk.url, f.httpUrl);
        const connection = f.state.httpConnections.find(value => value.client === 'claude');
        assert.ok(connection && connection.selection.kind === 'combined');
        assert.deepEqual(connection.selection.selection.index_ids, [chain.indexId], 'Reconnect retains the exact reviewed index subset');
    } finally { await f.close(); }
});

test('Reconnect checks reviewed combined members instead of the first display root and never widens the index subset', async context => {
    const f = await serviceTestHost(context, { registered: false });
    try {
        await controllerOf(f).refreshAgentAlerts();
        const root = f.folders[0].uri.fsPath, chain = await realScopeChain(context, f, root);
        await chain.controller.prepareWindowContext();
        await chain.controller.applySearchScope({ index_ids: [chain.indexId], project_ids: [] });
        const active = chain.controller.currentCatalogScope(), secondId = 'idx_' + '69'.repeat(16), otherRoot = path.join(f.root, 'different-source');
        const second = { ...chain.index, id: secondId, name: 'Other', backing: { ...chain.index.backing, root: otherRoot, collection: 'other' } };
        const grant = chain.grant()!, members = grant.memberships as CatalogObject[], first = members[0];
        const expanded = { ...grant, memberships: [...members, { ...first, membership: { ...catalogObject(first.membership), index_id: secondId } }] };
        const native = chain.descriptor(), detail = (kind: string, value: unknown) => ({ namespace: native.namespace, revision: native.catalog_revision, item: { kind, value } });
        const reviewed = (index_ids?: string[]) => reviewCombinedScopeSelection({ grant_id: grant.id, expected_grant_revision: grant.revision,
            project_ids: active.selection.session.project_ids, ...(index_ids ? { index_ids } : {}) }, native,
            detail('grant', { grant: expanded, provenance: { kind: 'user_reviewed_project_grant' } }), (index_ids ?? [secondId, chain.indexId]).map(id => detail('index', id === secondId ? second : chain.index)));
        active.selection = reviewed();
        active.spec = nativeScopeLaunchSpec(f.binding, active.selection);
        const selected = chain.controller.reconnectScope(root);
        assert.ok(selected?.combinedSession, 'a matching later reviewed member is eligible');
        assert.equal(plainPath(selected.root), root);
        assert.deepEqual(selected.combinedSession.index_ids, [chain.indexId]);
        active.selection = reviewed([secondId]); active.spec = nativeScopeLaunchSpec(f.binding, active.selection); active.spec.root = root;
        assert.equal(chain.controller.reconnectScope(root), undefined, 'a display root cannot invent a reviewed member');
    } finally { await f.close(); }
});

test('Reconnect without an index waits for explicit Index library creation without native mutations or configuration writes', async context => {
    const f = await serviceTestHost(context, { registered: false });
    try {
        await controllerOf(f).refreshAgentAlerts();
        const { root, destination, source, document } = await brokenProject(f), chain = reviewedChain(context, f, root);
        await f.commands.get('airk.reconnectAgents')!();
        assert.deepEqual(chain.calls, ['index_lookup']);
        assert.deepEqual(chain.selections, []);
        assert.equal(f.state.httpConnections.length, 0);
        assert.equal(document.saves, 0);
        assert.equal(await fs.readFile(destination, 'utf8'), source);
        const results = chain.controller.agentConfigAlerts()?.results ?? [];
        assert.deepEqual(results.map((result: { status: string }) => result.status), ['awaiting_index']);
        assert.match(results[0].detail, /Indexing → Index library/);
        assert.match(results[0].detail, /left unchanged/);
        assert.ok(!f.warnings.some(value => /Create .*AIRK index/.test(value)), 'Reconnect never offers to create an index');
        assert.equal(f.writes.some(value => 'catalog_index_setup_apply' in value || 'reserve_project' in value), false);
    } finally { await f.close(); }
});

test('Reconnect grants an existing index for the folder instead of creating another', async context => {
    const f = await serviceTestHost(context, { registered: false });
    try {
        await controllerOf(f).refreshAgentAlerts();
        const { root } = await brokenProject(f), chain = reviewedChain(context, f, root, { existing: true });
        await f.commands.get('airk.reconnectAgents')!();
        assert.ok(f.warnings.includes('Reconnect coding agents to AIRK?'));
        assert.deepEqual(chain.calls.filter(call => call !== 'window_context'), ['index_lookup', 'access_grant']);
        assert.deepEqual(chain.selections, [{ index_ids: [chain.index], project_ids: [] }]);
        assert.deepEqual((chain.controller.agentConfigAlerts()?.results ?? []).map((result: { status: string }) => result.status), ['awaiting_host_reconnect']);
    } finally { await f.close(); }
});

for (const [name, code] of [['ambiguous indexes', 'reconnect_index_selection_required'], ['incomplete inventory', 'reconnect_index_inventory_incomplete']] as const) {
    test(`Reconnect preserves entries when existing-index lookup has ${name}`, async context => {
        const f = await serviceTestHost(context, { registered: false });
        try {
            await controllerOf(f).refreshAgentAlerts();
            const { root, destination, source } = await brokenProject(f), chain = reviewedChain(context, f, root, { existing: true });
            const index = (suffix: string) => ({ kind: 'index', value: { id: 'idx_' + suffix.repeat(16), name: 'Existing', definition_revision: 1, source_revision: 1,
                source_import: 'complete', tombstone: false, backing: { root } } });
            context.mock.method(chain.controller.console.controller, 'catalogRead', async () => ({ status: 'catalog_page', page: { namespace: '34'.repeat(16), revision: 1,
                items: name === 'ambiguous indexes' ? [index('7a'), index('7b')] : [index('7a')], continuation: name === 'incomplete inventory' ? { cursor: 'same' } : null } }));
            await f.commands.get('airk.reconnectAgents')!();
            const results = chain.controller.agentConfigAlerts()?.results ?? [];
            assert.deepEqual(results.map((value: { status: string }) => value.status), ['awaiting_index']);
            assert.ok(results[0].detail.includes(code), results[0].detail);
            assert.deepEqual(chain.selections, [], 'no scope is granted from ambiguous or incomplete evidence');
            assert.equal(f.state.httpConnections.length, 0);
            assert.equal(await fs.readFile(destination, 'utf8'), source);
        } finally { await f.close(); }
    });
}

test('without the confirmation nothing is created, granted, enrolled or written', async context => {
    const f = await serviceTestHost(context, { registered: false });
    try {
        await controllerOf(f).refreshAgentAlerts();
        const { root, destination, source } = await brokenProject(f), chain = reviewedChain(context, f, root, { existing: true });
        f.state.confirm = false;
        await f.commands.get('airk.reconnectAgents')!();
        assert.deepEqual(chain.calls, ['index_lookup'], 'only the read-only lookup that names the plan; the window context (key save, host context read) waits for the confirmation');
        assert.equal(f.state.httpConnections.length, 0);
        assert.equal(await fs.readFile(destination, 'utf8'), source);
    } finally { await f.close(); }
});

const refusal = (code: string, outcomeUnknown = false) => () => { throw new ServiceError(code, outcomeUnknown, { status: outcomeUnknown ? 'catalog_outcome_unknown' : 'failure', code }); };
for (const [stage, options, status, code] of [
    ['access grant', { existing: true, scope: refusal('catalog_outcome_unconfirmed_reconcile_first') }, 'awaiting_index', /access grant could not complete \(catalog_outcome_unconfirmed_reconcile_first\)/],
    ['unknown access grant outcome', { existing: true, scope: refusal('catalog_outcome_unknown', true) }, 'recovery_required', /outcome is unknown/],
    ['unconfirmed native scope outcome', { existing: true, scope: () => { throw Object.assign(new Error('search_scope_outcome_unconfirmed'), { nativeResponse: { status: 'catalog_outcome_unknown' } }); } }, 'recovery_required', /outcome is unknown.*Native status: catalog_outcome_unknown/],
] as const) test(`a refusal at the ${stage} stage reports ${status} with its typed code and writes no configuration`, async context => {
    const f = await serviceTestHost(context, { registered: false });
    try {
        await controllerOf(f).refreshAgentAlerts();
        const { root, destination, source } = await brokenProject(f), chain = reviewedChain(context, f, root, options);
        await f.commands.get('airk.reconnectAgents')!();
        const results = chain.controller.agentConfigAlerts()?.results ?? [];
        assert.deepEqual(results.map((result: { status: string }) => result.status), [status]);
        assert.match(results[0].detail, code); assert.match(results[0].detail, /left unchanged/);
        assert.equal(await fs.readFile(destination, 'utf8'), source, 'the agent configuration is unchanged');
        assert.equal(f.state.httpConnections.length, 0, 'no HTTPS connection is enrolled');
    } finally { await f.close(); }
});

// ---- Retired stdio paths: nothing starts an MCP server over stdio in production.

test('the evidence client and the stdio session refuse stdio MCP without starting a process', async () => {
    const { EvidenceClient, EvidenceFailure } = await import('../explorerEvidence');
    const { McpStdioSession, McpSessionError } = await import('../mcpStdioSession');
    const client = new EvidenceClient({ ...spec(), http: undefined }, 'test');
    try { await assert.rejects(client.call('airk_status', {}), (error: unknown) => error instanceof EvidenceFailure && error.error.code === 'SCOPE_NOT_GRANTED'); }
    finally { client.dispose(); }
    await assert.rejects(McpStdioSession.open({ command: process.execPath, args: ['-e', 'setInterval(()=>{},1000)'], cwd: os.tmpdir(), clientName: 'test', clientVersion: '1' }, 1000),
        (error: unknown) => error instanceof McpSessionError && error.code === 'stdio_mcp_retired');
});

// ---- Review corrections: endpoint spelling, helper-only references, helper image refresh.

test('capture matches the removed endpoint in any equivalent spelling and entries that name a removed image only through a credential helper', () => {
    const removedHelper = (id: string) => credentialHelperCommand({ executable: references.executables[1], installDir: install }, id);
    const claude = JSON.stringify({ mcpServers: {
        upper: { type: 'http', url: 'HTTPS://127.0.0.1:57400/mcp/', headers: { Authorization: 'Bearer ${AIRK_MCP_TOKEN_' + 'A'.repeat(32) + '}' } },
        helper: { type: 'http', url: 'https://127.0.0.1:57999/mcp', headers: { Authorization: 'Bearer ${AIRK_MCP_TOKEN_' + 'B'.repeat(32) + '}' }, headersHelper: removedHelper('b'.repeat(32)) },
        current: { type: 'http', url: 'https://127.0.0.1:57401/mcp', headers: { Authorization: 'Bearer ${AIRK_MCP_TOKEN_' + 'C'.repeat(32) + '}' } } } });
    const codex = `[mcp_servers.airk]\nurl = "https://127.0.0.1:57999/mcp"\nhttp_headers_helper = ${JSON.stringify(removedHelper('d'.repeat(32)))}\n`;
    const ledger = captureReconnectLedger([{ agent: 'claude', scope: 'user', destination: path.join(userBase, '.claude.json'), source: claude },
        { agent: 'codex', scope: 'project', destination: path.join(projectRoot, '.codex', 'config.toml'), source: codex }], references, install, 1);
    assert.deepEqual(ledger.entries.map(entry => [entry.serverKey, entry.reference]), [['upper', 'endpoint'], ['helper', 'executable'], ['airk', 'executable']]);
    assert.equal(normalizedUrl('HTTPS://127.0.0.1:443/mcp/'), 'https://127.0.0.1/mcp');
    assert.equal(normalizedUrl('https://127.0.0.1:57400/mcp'), normalizedUrl('https://127.0.0.1:57400/mcp/'));
    assert.notEqual(normalizedUrl('https://127.0.0.1:57400/mcp'), normalizedUrl('https://127.0.0.1:57401/mcp'));
    assert.equal(commandExecutable(`"${path.join(userBase, 'Program Files', 'airk.exe')}" runtime`), path.join(userBase, 'Program Files', 'airk.exe'));
});

test('a current HTTPS entry whose credential helper names a retired runtime image is a refreshable finding, not a Reconnect', async () => {
    const retired = { executable: path.join(install, 'runtimes', '00'.repeat(32), win ? 'airk.exe' : 'airk'), installDir: install };
    const source = JSON.stringify({ mcpServers: { airk: { type: 'http', url, headers: { Authorization: 'Bearer ${AIRK_MCP_TOKEN_' + id.toUpperCase() + '}' }, headersHelper: credentialHelperCommand(retired, id) } } });
    const probes: string[] = [];
    const health = await checkAgentConnections([], { exists: async command => { probes.push(command); return command !== retired.executable; },
        userFiles: [{ agent: 'claude', destination: path.join(userBase, '.claude.json'), source }],
        http: { lookup: async () => ({ status: 'owned', url }), issuerTrusted: async () => true } });
    assert.deepEqual(health.findings.map(finding => finding.problem), ['https_helper_missing']);
    assert.deepEqual(probes, [retired.executable]);
    const current = await checkAgentConnections([], { exists: async () => true, userFiles: [{ agent: 'claude', destination: path.join(userBase, '.claude.json'), source }],
        http: { lookup: async () => ({ status: 'owned', url }), issuerTrusted: async () => true } });
    assert.deepEqual(current.findings, []);
});

test('a runtime change rewrites owned credential helpers to the new image before retire-previous runs', async context => {
    const f = await serviceTestHost(context, { automatic: false });
    try {
        const definitions = await f.api.getServerDefinitions(), connection = String(definitions[0].headers.Authorization).split('.')[1];
        const info = await RuntimeHost.prototype.httpConnectionInfo.call({} as RuntimeHost, f.binding, connection);
        const row = [...f.registrations.values()][0], destination = path.join(row.root, '.mcp.json');
        const retired = { executable: path.join(f.binding.installDir, 'runtimes', '00'.repeat(32), win ? 'airk.exe' : 'airk'), installDir: f.binding.installDir };
        const source = JSON.stringify({ mcpServers: { airk: { type: 'http', url: info.endpoint.url, headers: { Authorization: 'Bearer ${AIRK_MCP_TOKEN_' + connection.toUpperCase() + '}', 'X-AIRK-Response-Mode': 'terse' },
            headersHelper: credentialHelperCommand(retired, connection), disabled: true }, other: { command: 'keep' } } }, null, 2) + '\n';
        await fs.writeFile(destination, source); editor(f, destination, source);
        let atRetire: string | undefined;
        context.mock.method(RuntimeHost.prototype, 'retirePrevious', async (selected: typeof f.binding) => {
            atRetire ??= await fs.readFile(destination, 'utf8');
            return { active: selected.runtimeSha256, retired_previous: null };
        });
        await f.commands.get('airk.installCli')!();
        assert.ok(atRetire, 'retire-previous ran');
        const value = parse(atRetire!);
        assert.equal(value.mcpServers.airk.headersHelper, credentialHelperCommand({ executable: f.binding.executable, installDir: f.binding.installDir }, connection),
            'the helper names the selected image before the previous one is retired');
        assert.equal(value.mcpServers.airk.disabled, true); assert.deepEqual(value.mcpServers.other, { command: 'keep' });
    } finally { await f.close(); }
});
