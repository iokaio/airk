// SPDX-License-Identifier: Apache-2.0
// `vscode:uninstall` removes only editor-owned storage (plan §8.5): no native
// uninstall, no process, nothing outside `<user data>/User/globalStorage/iokaio.airk`.
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { editorUserData, EXTENSION_ID, removeEditorStorage } from '../uninstallHook';

test('the editor user-data folder follows the standard per-user layouts only', () => {
    assert.equal(editorUserData('/home/u/.vscode/extensions', '/home/u', 'linux'), '/home/u/.config/Code');
    assert.equal(editorUserData('/home/u/.vscode-insiders/extensions', '/home/u', 'linux'), '/home/u/.config/Code - Insiders');
    assert.equal(editorUserData('/Users/u/.vscode/extensions', '/Users/u', 'darwin'), '/Users/u/Library/Application Support/Code');
    assert.equal(editorUserData('C:\\Users\\u\\.vscode\\extensions', 'C:\\Users\\u', 'win32', 'C:\\Users\\u\\AppData\\Roaming'), 'C:\\Users\\u\\AppData\\Roaming\\Code');
    // Portable, custom or foreign layouts are left alone.
    assert.equal(editorUserData('/opt/vscode/data/extensions', '/home/u', 'linux'), undefined);
    assert.equal(editorUserData('/home/u/.cursor/extensions', '/home/u', 'linux'), undefined);
    assert.equal(editorUserData('/home/other/.vscode/extensions', '/home/u', 'linux'), undefined);
    assert.equal(editorUserData('C:\\Users\\u\\.vscode\\extensions', 'C:\\Users\\u', 'win32'), undefined, 'no APPDATA, no guess');
});

test('only this extension\'s global storage folder is removed; a link is refused', async () => {
    const root = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-uninstall-'));
    try {
        const storage = path.join(root, 'User', 'globalStorage', EXTENSION_ID), other = path.join(root, 'User', 'globalStorage', 'other.extension');
        await fs.mkdir(path.join(storage, 'workbench'), { recursive: true }); await fs.mkdir(other, { recursive: true });
        await fs.writeFile(path.join(storage, 'workbench', 'session.json'), '{}'); await fs.writeFile(path.join(other, 'keep.json'), '{}');
        assert.equal(await removeEditorStorage(root), 'removed');
        await assert.rejects(fs.lstat(storage));
        assert.equal(await fs.readFile(path.join(other, 'keep.json'), 'utf8'), '{}');
        assert.equal(await removeEditorStorage(root), 'absent');
        const target = path.join(root, 'elsewhere'); await fs.mkdir(target);
        try { await fs.symlink(target, storage, process.platform === 'win32' ? 'junction' : 'dir'); } catch { return; }
        assert.equal(await removeEditorStorage(root), 'refused');
        await fs.access(target);
    } finally { await fs.rm(root, { recursive: true, force: true }); }
});

test('the hook starts no process and names no native uninstall', async () => {
    const source = await fs.readFile(path.resolve(__dirname, '..', '..', 'src', 'uninstallHook.ts'), 'utf8');
    assert.doesNotMatch(source, /child_process|spawn\(|execFile|runtime uninstall|airkctl/);
});
