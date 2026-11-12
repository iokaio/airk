// SPDX-License-Identifier: Apache-2.0
/**
 * `vscode:uninstall` (plan §8.5): removes only editor-owned storage of this
 * extension (`<editor user data>/User/globalStorage/iokaio.airk`). It runs no
 * native uninstall, starts no process, and leaves the AIRK Service, its
 * indexes, agent configuration and every project source untouched; the
 * service has its own uninstaller.
 */
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';

export const EXTENSION_ID = 'iokaio.airk';
type Platform = 'win32' | 'darwin' | 'linux';

/** The editor user-data folder that owns `extensionsDir`, for the standard
 * per-user layouts only; a portable or custom layout returns undefined. */
export function editorUserData(extensionsDir: string, home: string, platform: Platform, appData?: string): string | undefined {
    const api = platform === 'win32' ? path.win32 : path.posix;
    const products: Record<string, string> = { '.vscode': 'Code', '.vscode-insiders': 'Code - Insiders', '.vscode-oss': 'VSCodium' };
    if (api.basename(extensionsDir) !== 'extensions') { return undefined; }
    const marker = api.dirname(extensionsDir);
    const same = (a: string, b: string) => platform === 'win32' ? a.toLowerCase() === b.toLowerCase() : a === b;
    if (!same(api.dirname(marker), api.normalize(home))) { return undefined; }
    const product = products[api.basename(marker)];
    if (!product) { return undefined; }
    switch (platform) {
        case 'win32': return appData ? api.join(appData, product) : undefined;
        case 'darwin': return api.join(home, 'Library', 'Application Support', product);
        default: return api.join(home, '.config', product);
    }
}

/** Remove `<userData>/User/globalStorage/iokaio.airk` when it is a real folder. */
export async function removeEditorStorage(userData: string): Promise<'removed' | 'absent' | 'refused'> {
    const storage = path.join(userData, 'User', 'globalStorage', EXTENSION_ID);
    let info;
    try { info = await fs.lstat(storage); } catch (error) {
        if ((error as NodeJS.ErrnoException).code === 'ENOENT') { return 'absent'; }
        throw error;
    }
    if (!info.isDirectory() || info.isSymbolicLink()) { return 'refused'; }
    await fs.rm(storage, { recursive: true, force: false });
    return 'removed';
}

async function main(): Promise<void> {
    const extensionsDir = path.resolve(__dirname, '..', '..');
    const platform = (process.platform === 'win32' || process.platform === 'darwin' ? process.platform : 'linux') as Platform;
    const userData = editorUserData(extensionsDir, os.userInfo().homedir, platform, process.env.APPDATA);
    if (!userData) { process.stdout.write('AIRK: editor storage location not recognized; nothing was removed.\n'); return; }
    const result = await removeEditorStorage(userData);
    process.stdout.write(`AIRK: editor storage ${result}. AIRK Service, its indexes and agent configuration were not changed.\n`);
}
if (require.main === module) { void main().catch(error => { process.stderr.write(String(error) + '\n'); process.exitCode = 1; }); }
