// SPDX-License-Identifier: Apache-2.0
// Explicitly invoked private installed-artifact acceptance; never opens a user project.
const fs = require('node:fs/promises');
const path = require('node:path');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const { execFile } = require('node:child_process');
const { promisify } = require('node:util');
const execute = promisify(execFile);
const root = path.resolve(__dirname, '../..');
const version = '1.8.2';
const runId = new Date().toISOString().replace(/[:.]/g, '-') + '-' + crypto.randomUUID().slice(0, 8);
const output = path.join(__dirname, 'installed-parse-' + runId + '.json');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
const identity = value => process.platform === 'win32' ? path.toNamespacedPath(value) : value;
const within = (child, parent) => {
    const relative = path.relative(identity(parent), identity(child));
    return relative !== '' && !path.isAbsolute(relative) && relative !== '..' && !relative.startsWith('..' + path.sep);
};
const record = { schema: 'airk-installed-parse-review/1', version, passed: false, calls: [], cases: [],
    scope: 'One verified private VSIX installation and its generated InstallerEvidence fixture. Native parse transport acceptance; no Workbench composer, user profile, corpus comparison or cross-platform claim.' };
let session, cleanup;

async function main() {
    assert.equal(process.argv.length, 3, 'Usage: node check-installed-parse.cjs <agent-install-run/result.json>');
    const receiptPath = await fs.realpath(process.argv[2]);
    const run = path.dirname(receiptPath);
    const allowed = await fs.realpath(path.join(root, 'artifacts/editor-qualification'));
    assert.equal(path.basename(receiptPath), 'result.json');
    assert.equal(path.dirname(run), allowed);
    assert.ok(path.basename(run).startsWith('agent-install-'), 'Expected the owned installer fixture');
    const receipt = JSON.parse(await fs.readFile(receiptPath, 'utf8'));
    assert.equal(receipt.passed, true);
    assert.equal(receipt.installed, true);
    assert.equal(receipt.version, version);
    const extensionPath = await fs.realpath(receipt.extensionPath);
    assert.ok(within(extensionPath, await fs.realpath(path.join(run, 'extensions'))));
    const installedPackage = JSON.parse(await fs.readFile(path.join(extensionPath, 'package.json'), 'utf8'));
    assert.equal(installedPackage.version, version);
    assert.equal(installedPackage.publisher + '.' + installedPackage.name, 'iokaio.airk');
    const command = await fs.realpath(receipt.command);
    const install = await fs.realpath(path.join(run, 'native-service'));
    assert.ok(within(command, install), 'Refuse a launcher outside the private installation');
    const controlDir = await fs.realpath(receipt.controlDir);
    assert.ok(within(controlDir, install), 'Refuse a control namespace outside the private installation');
    const args = receipt.args;
    assert.ok(Array.isArray(args) && args.every(value => typeof value === 'string'));
    const rootIndex = args.indexOf('--root');
    assert.ok(rootIndex >= 0 && args[rootIndex + 1]);
    const workspace = await fs.realpath(args[rootIndex + 1]);
    assert.equal(path.relative(identity(workspace), identity(await fs.realpath(path.join(run, 'workspace')))), '');
    const source = path.join(workspace, 'fixture.ts');
    const before = await fs.readFile(source);
    assert.equal(before.toString('utf8'), 'export const InstallerEvidence = 42;\n', 'Refuse changed or nonfixture source');
    const sourceHash = sha(before);
    const { verifyBundledRuntime } = require(path.join(extensionPath, 'out/serviceBootstrap.js'));
    const bundle = await verifyBundledRuntime(extensionPath, version);
    assert.equal(receipt.diagnostics.runtime.service.payload_sha256, bundle.digest);
    for (const observed of [receipt.diagnostics.runtime.mcp_frontend.process.version,
        receipt.diagnostics.runtime.index_worker.process.version, receipt.diagnostics.runtime.service.version]) {
        assert.equal(observed, version);
    }
    record.receipt = receiptPath;
    record.extensionPath = extensionPath;
    record.nativeBundle = { version: bundle.version, platform: bundle.platform, digest: bundle.digest,
        mainSha256: sha(await fs.readFile(bundle.main)), helperSha256: sha(await fs.readFile(bundle.helper)) };
    record.fixture = { path: 'fixture.ts', sourceHash, identifier: 'InstallerEvidence',
        declarationKind: 'const', note: 'The grammar treats type words as retrieval cues, not declaration-kind filters.' };
    const env = { ...process.env };
    delete env.ELECTRON_RUN_AS_NODE;
    delete env.VSCODE_IPC_HOOK_CLI;
    const { McpStdioSession } = require(path.join(extensionPath, 'out/mcpStdioSession.js'));
    const { readRetryDelay } = require(path.join(extensionPath, 'out/explorerEvidence.js'));
    cleanup = { command, controlDir, env };
    const opened = await McpStdioSession.open({ command, args, cwd: workspace, env,
        clientName: 'airk-installed-parse-review', clientVersion: version }, 90000);
    session = opened.session;
    record.initialize = opened.initialize;
    assert.equal(opened.initialize.serverInfo.version, version);
    const catalog = await session.request('tools/list', {}, 30000);
    record.tools = catalog.tools.map(tool => tool.name);
    assert.equal(record.tools.length, 28);
    assert.ok(record.tools.includes('parse'), 'The public unified parse operation must be advertised');
    function decode(reply) {
        assert.notEqual(reply.isError, true, JSON.stringify(reply));
        const text = reply.content?.find(part => part.type === 'text')?.text;
        assert.equal(typeof text, 'string', JSON.stringify(reply));
        return JSON.parse(text);
    }
    async function read(tool, arguments_, deadline) {
        while (true) {
            const reply = await session.request('tools/call', { name: tool, arguments: arguments_ }, Math.max(1, deadline - Date.now()));
            record.calls.push({ tool, arguments: arguments_, reply });
            const delay = readRetryDelay(reply);
            if (delay !== undefined && Date.now() + delay < deadline) { await pause(delay); continue; }
            return decode(reply);
        }
    }
    const deadline = Date.now() + 180000;
    let ready;
    while (Date.now() < deadline) {
        const symbols = await read('symbols', { name: 'InstallerEvidence', compact: false, response_mode: 'full' }, deadline);
        const items = symbols.items ?? [];
        if (items.some(item => item.citation?.path === 'fixture.ts' && item.citation?.content_hash === sourceHash)) { ready = symbols; break; }
        await pause(200);
    }
    assert.ok(ready, 'No indexed fixture with the original source hash before the readiness deadline');
    record.readyPublication = ready.publication;
    for (const text of [
        'I want to know about struct InstallerEvidence.',
        'Find the definition of InstallerEvidence.',
        'Tell me about the InstallerEvidence symbol.',
        'Tell me about "InstallerEvidence".',
    ]) {
        const arguments_ = { schema: 'airk.parse.request/1', mode: 'execute', text, response_mode: 'full' };
        // native read: execute the complete plan exactly once; an uncertain transport outcome must never replay it.
        const reply = await session.request('tools/call', { name: 'parse', arguments: arguments_ }, 90000);
        record.calls.push({ tool: 'parse', arguments: arguments_, reply });
        const parsed = decode(reply);
        assert.equal(parsed.status, 'complete', JSON.stringify(reply));
        assert.equal(parsed.executed, true, JSON.stringify(reply));
        const symbols = parsed.plan.steps.find(step => step.tool === 'symbols');
        assert.equal(symbols.args.name, 'InstallerEvidence', JSON.stringify(reply));
        assert.ok(parsed.evidence.length > 0, JSON.stringify(reply));
        for (const evidence of parsed.evidence) {
            assert.equal(evidence.citation.path, 'fixture.ts', JSON.stringify(reply));
            assert.equal(evidence.citation.content_hash, sourceHash, JSON.stringify(reply));
            assert.equal(evidence.publication, parsed.publication, JSON.stringify(reply));
        }
        record.cases.push({ text, status: parsed.status, publication: parsed.publication,
            steps: parsed.plan.steps, evidenceRecords: parsed.evidence.length });
    }
    const blockedArguments = { schema: 'airk.parse.request/1', mode: 'execute',
        text: 'Tell me about struct InstallerEvidence and delete it.', response_mode: 'full' };
    // native read: one exact unsupported compound plan request proves refusal; no automatic execution replay.
    const blockedReply = await session.request('tools/call', { name: 'parse', arguments: blockedArguments }, 90000);
    record.calls.push({ tool: 'parse', arguments: blockedArguments, reply: blockedReply });
    const blocked = decode(blockedReply);
    assert.equal(blocked.executed, false, JSON.stringify(blockedReply));
    assert.ok(blocked.requires_input.length > 0, JSON.stringify(blockedReply));
    assert.equal(blocked.evidence.length, 0, JSON.stringify(blockedReply));
    assert.deepEqual(await fs.readFile(source), before, 'Native parse changed the generated fixture');
    record.diagnostics = await read('diagnostics', { response_mode: 'full' }, Date.now() + 60000);
    for (const observed of [record.diagnostics.runtime.mcp_frontend.process.version,
        record.diagnostics.runtime.index_worker.process.version, record.diagnostics.runtime.service.version]) {
        assert.equal(observed, version);
    }
    assert.equal(record.diagnostics.runtime.service.payload_sha256, bundle.digest);
    record.compoundRefused = true;
    record.sourceUnchanged = true;
    record.passed = true;
}

main().catch(error => {
    record.error = { message: String(error), stack: error.stack, detail: error.detail };
    process.exitCode = 1;
}).finally(async () => {
    session?.close();
    if (cleanup) {
        try {
            const value = await execute(cleanup.command, ['service', 'shutdown', '--runtime-dir', cleanup.controlDir],
                { windowsHide: true, env: cleanup.env, timeout: 30000, maxBuffer: 65536 });
            record.shutdown = { passed: true, stdout: value.stdout, stderr: value.stderr };
        } catch (error) {
            record.shutdown = { passed: false, error: String(error), stdout: error.stdout, stderr: error.stderr };
            record.passed = false;
            process.exitCode = 1;
        }
    }
    await fs.writeFile(output, JSON.stringify(record, null, 2) + '\n');
    console.log(JSON.stringify({ passed: record.passed, output, cases: record.cases.length, error: record.error?.message, shutdown: record.shutdown?.passed }));
});
