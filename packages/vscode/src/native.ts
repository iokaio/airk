// SPDX-License-Identifier: Apache-2.0
import { execFile, spawn } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { LaunchSpec, launchArgs } from './model';

/** Serialize this extension host's writers; native locks still arbitrate other processes. */
export class NativeWriteQueue {
    private tails = new Map<string, Promise<void>>();
    run<T>(spec: LaunchSpec, operation: () => Promise<T>): Promise<T> {
        const directory = path.resolve(spec.dataDir);
        const key = JSON.stringify([process.platform === 'win32' ? directory.toLowerCase() : directory, spec.collection]);
        const result = (this.tails.get(key) ?? Promise.resolve()).then(operation);
        const tail = result.then(() => undefined, () => undefined);
        this.tails.set(key, tail);
        void tail.then(() => { if (this.tails.get(key) === tail) { this.tails.delete(key); } });
        return result;
    }
}

export async function prepareHelper(extensionPath: string, storage: string, version: string): Promise<string> {
    const executable = process.platform === 'win32' ? 'airk.exe' : 'airk';
    const native = path.join(extensionPath, 'native');
    const manifest = JSON.parse(await fs.readFile(path.join(native, 'manifest.json'), 'utf8'));
    const platform = `${process.platform}-${process.arch}`;
    if (manifest.version !== version || manifest.vsix_target !== platform) {
        throw new Error(`AIRK helper does not match extension ${version} / ${platform}. Install the matching platform VSIX.`);
    }
    const digest: unknown = manifest.payload?.[executable];
    if (typeof digest !== 'string' || !/^[0-9a-f]{64}$/.test(digest)) { throw new Error('AIRK native manifest has no valid executable digest.'); }
    const source = path.join(native, executable);
    const bytes = await fs.readFile(source);
    if (createHash('sha256').update(bytes).digest('hex') !== digest) { throw new Error('AIRK bundled helper digest mismatch.'); }
    // Version/digest-specific cached paths survive extension directory replacement.
    const directory = path.join(storage, 'helpers', `${version}-${platform}-${digest.slice(0, 16)}`);
    await fs.mkdir(directory, { recursive: true });
    const destination = path.join(directory, executable);
    try {
        const cached = await fs.readFile(destination);
        if (createHash('sha256').update(cached).digest('hex') !== digest) { throw new Error('AIRK cached helper digest mismatch.'); }
    } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== 'ENOENT') { throw error; }
        const temporary = path.join(directory, randomUUID() + '.tmp');
        try {
            await fs.writeFile(temporary, bytes, { flag: 'wx', mode: 0o755 });
            await fs.rename(temporary, destination);
        } finally { await fs.unlink(temporary).catch(() => undefined); }
    }
    const actual = await runText(destination, ['--version']);
    if (actual.trim() !== `airk ${version}`) { throw new Error('AIRK helper version mismatch.'); }
    return destination;
}

export function runText(command: string, args: string[], cwd?: string): Promise<string> {
    return new Promise((resolve, reject) => {
        execFile(command, args, { cwd, env: { ...process.env, AIRK_CLIENT: 'vscode' }, windowsHide: true, maxBuffer: 16 * 1024 * 1024, timeout: 300000 }, (error, stdout, stderr) => {
            if (error) { reject(new Error(stderr.trim() || error.message)); }
            else { resolve(stdout); }
        });
    });
}

export async function runJson<T>(spec: LaunchSpec, command: string, extra: string[] = []): Promise<T> {
    return JSON.parse(await runText(spec.command, [...launchArgs(spec, command), ...extra], spec.root)) as T;
}

/** Real bounded stdio initialization/tool-list/status probe, independent of any coding agent. */
export function probeMcp(spec: LaunchSpec, expectedTools: 14 | 15 | 16 = 16, clientVersion = 'development'): Promise<{ tools: string[]; status: unknown }> {
    return new Promise((resolve, reject) => {
        const child = spawn(spec.command, launchArgs(spec), { cwd: spec.root, env: { ...process.env, AIRK_CLIENT: 'vscode' }, windowsHide: true, stdio: 'pipe' });
        let buffer = '', stderr = '', total = 0, done = false;
        const requests = new Map<number, { resolve(value: any): void; reject(error: Error): void }>();
        const finish = (error?: Error, result?: { tools: string[]; status: unknown }) => {
            if (done) { return; } done = true; clearTimeout(timer);
            child.stdin.end(); child.kill();
            for (const request of requests.values()) { request.reject(error ?? new Error('Probe finished.')); }
            requests.clear();
            if (error) { reject(error); } else { resolve(result!); }
        };
        const timer = setTimeout(() => finish(new Error('AIRK MCP probe timed out.')), 15000);
        child.on('error', error => finish(error));
        child.on('exit', code => { if (!done) { finish(new Error(`AIRK MCP exited (${code}): ${stderr}`)); } });
        child.stdin.on('error', error => finish(error));
        child.stdout.setEncoding('utf8');
        child.stderr.on('data', bytes => { stderr = (stderr + bytes.toString()).slice(-4096); });
        child.stdout.on('data', bytes => {
            total += Buffer.byteLength(bytes);
            if (total > 1024 * 1024) { finish(new Error('AIRK MCP probe response exceeded budget.')); return; }
            buffer += bytes.toString();
            let end: number;
            while ((end = buffer.indexOf('\n')) >= 0) {
                const line = buffer.slice(0, end); buffer = buffer.slice(end + 1);
                try {
                    const message = JSON.parse(line);
                    const request = requests.get(message.id);
                    if (request) {
                        requests.delete(message.id);
                        if (message.error) { request.reject(new Error(JSON.stringify(message.error))); }
                        else { request.resolve(message.result); }
                    }
                } catch { finish(new Error('AIRK MCP emitted invalid JSON.')); return; }
            }
        });
        const request = (id: number, method: string, params: unknown): Promise<any> => new Promise((resolveRequest, rejectRequest) => {
            requests.set(id, { resolve: resolveRequest, reject: rejectRequest });
            child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n');
        });
        void (async () => {
            await request(1, 'initialize', { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'airk-extension-check', version: clientVersion } });
            child.stdin.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');
            const listed = await request(2, 'tools/list', {});
            const tools: string[] = listed.tools.map((tool: { name: string }) => tool.name);
            const required = ['airk_status', 'airk_symbols', 'airk_code_facts', 'airk_relationships', ...(expectedTools >= 15 ? ['airk_changes'] : []), ...(expectedTools >= 16 ? ['airk_history'] : [])];
            if (tools.length !== expectedTools || !required.every(name => tools.includes(name))) { throw new Error(`AIRK MCP tool inventory differs from the expected ${expectedTools} tools.`); }
            const status = await request(3, 'tools/call', { name: 'airk_status', arguments: {} });
            if (status.isError) { throw new Error('AIRK MCP status failed. Index the collection first.'); }
            finish(undefined, { tools, status });
        })().catch(error => finish(error instanceof Error ? error : new Error(String(error))));
    });
}
