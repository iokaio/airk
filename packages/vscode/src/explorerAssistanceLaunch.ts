// SPDX-License-Identifier: Apache-2.0
// Launch contract for the Explorer Assistance server. External agents run
// `airk explorer-assistance --descriptor <dir>/launch-1.json` through the stable launcher;
// the native forwarder verifies this descriptor before starting VS Code's own runtime.
import { createHash } from 'node:crypto';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { writePrivate } from './explorerAssistanceHost';

export const LAUNCH_SCHEMA = 'airk-explorer-assistance-launch/1';
export const SERVER_FILE = path.join('out', 'explorerAssistance.js');

export const assistanceDirectory = (storage: string) => path.join(storage, 'explorer-assistance');
export const launchDescriptorPath = (storage: string) => path.join(assistanceDirectory(storage), 'launch-1.json');

export interface AssistanceRuntime { readonly path: string; readonly electron: boolean }
/** The editor's own runtime: Electron run as Node locally, the server's node in remote windows. */
export const editorRuntime = (): AssistanceRuntime => ({ path: process.execPath, electron: Boolean(process.versions.electron) });

/** Arguments and environment for running the bundled server with the editor's runtime. */
export function serverInvocation(extensionRoot: string, storage: string, runtime = editorRuntime()) {
    return { command: runtime.path, args: [path.join(extensionRoot, SERVER_FILE), '--assistance-dir', assistanceDirectory(storage)],
        env: { ...(runtime.electron ? { ELECTRON_RUN_AS_NODE: '1' } : {}), NODE_OPTIONS: null, NODE_PATH: null } as Record<string, string | null> };
}

/**
 * Rewrite the launch descriptor atomically when anything it names changed. Called on
 * every trusted activation, so extension or editor updates take effect on the next start.
 */
export function writeLaunchDescriptor(storage: string, extensionRoot: string, version: string, runtime = editorRuntime()): boolean {
    const server = path.join(extensionRoot, SERVER_FILE);
    const fields = { schema: LAUNCH_SCHEMA, extension_version: version, extension_root: extensionRoot, server,
        server_sha256: createHash('sha256').update(fs.readFileSync(server)).digest('hex'), runtime: { path: runtime.path, electron: runtime.electron } };
    const file = launchDescriptorPath(storage);
    try {
        const { written_unix_ms: _, ...existing } = JSON.parse(fs.readFileSync(file, 'utf8'));
        if (JSON.stringify(existing) === JSON.stringify(fields)) { return false; }
    } catch { /* missing or unreadable: rewrite */ }
    writePrivate(file, JSON.stringify({ ...fields, written_unix_ms: Date.now() }));
    return true;
}
