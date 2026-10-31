// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { configureShellPath, publishLauncherCommand, ShellTouch } from '../cliInstall';
import { defaultProfiles, ownedCommandDirectories, removeShellIntegration, windowsPathRemovalScript } from '../cliRemove';
import * as native from '../native';

async function fixture(context: { after(fn: () => Promise<void>): void }): Promise<string> {
    const root = await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(), 'airk-cli-remove-')));
    context.after(() => fs.rm(root, { recursive: true, force: true }));
    return root;
}

test('removal reverses only owned blocks, keeps user edits and deletes only files AIRK created', async context => {
    // Profile text uses a POSIX-representable storage path on every OS; nothing is written there.
    const root = await fixture(context), home = path.join(root, 'home'), storage = path.join(path.sep, 'opt', 'airk-fixture-storage');
    await fs.mkdir(home);
    await fs.writeFile(path.join(home, '.bashrc'), '# personal\nexport KEEP=yes\n');
    const [current, legacy] = ownedCommandDirectories(storage);
    const touched: ShellTouch[] = [];
    const env = { SHELL: '/bin/bash', XDG_CONFIG_HOME: path.join(home, '.config') };
    assert.deepEqual(await configureShellPath(current, home, env, 'all', touched), []);
    // A retired 1.5-era block for the legacy layout is recognized too.
    assert.deepEqual(await configureShellPath(legacy, home, env, 'all'), []);
    await fs.writeFile(path.join(home, '.zshrc'), (await fs.readFile(path.join(home, '.zshrc'), 'utf8')).replace('esac', 'esac # user edited'));
    const result = await removeShellIntegration(storage, touched, 'linux', { wrappers: false });
    assert.equal(await fs.readFile(path.join(home, '.bashrc'), 'utf8'), '# personal\nexport KEEP=yes\n');
    await assert.rejects(fs.access(path.join(home, '.profile')), /ENOENT/, 'a profile AIRK created is removed when empty');
    await assert.rejects(fs.access(path.join(home, '.config', 'fish', 'conf.d', 'airk-path.fish')), /ENOENT/);
    assert.match(await fs.readFile(path.join(home, '.zshrc'), 'utf8'), /user edited/, 'a modified block is preserved');
    assert.ok(result.preserved.some(message => message.includes('.zshrc')));
    assert.ok(result.removed.some(message => message.includes('.bashrc')));
    // Repeating after completion or interruption changes nothing else.
    const again = await removeShellIntegration(storage, touched, 'linux', { wrappers: false });
    assert.deepEqual(again.removed, []);
});

test('wrapper removal deletes the owned wrapper, can keep it for VS Code terminals and preserves an unrecognized command', async context => {
    const root = await fixture(context), storage = path.join(root, 'storage');
    if (process.platform === 'win32') { context.mock.method(native, 'runSystemText', async () => '{"removed":0}'); }
    const [current] = ownedCommandDirectories(storage);
    const name = process.platform === 'win32' ? 'airk.cmd' : 'airk';
    await publishLauncherCommand(path.join(root, 'launcher', 'airk-launcher'), storage);
    await removeShellIntegration(storage, [], process.platform, { wrappers: false });
    await fs.access(path.join(current, name));
    await removeShellIntegration(storage, [], process.platform);
    await assert.rejects(fs.access(path.dirname(current)), /ENOENT/, 'the wrapper and empty owned directories are removed');
    await publishLauncherCommand(path.join(root, 'launcher', 'airk-launcher'), storage);
    await fs.writeFile(path.join(current, name), 'echo replaced by the user\n');
    const result = await removeShellIntegration(storage, [], process.platform);
    assert.ok(result.preserved.some(message => message.includes('unrecognized terminal command')));
    await fs.access(path.join(current, name));
});

test('default profile candidates never mark ordinary startup files as AIRK-created', () => {
    const profiles = defaultProfiles('/home/fixture', { ZDOTDIR: '/home/fixture/zsh', XDG_CONFIG_HOME: 'relative' });
    assert.ok(profiles.some(entry => entry.file === path.join('/home/fixture/zsh', '.zshrc') && !entry.created));
    assert.deepEqual(profiles.filter(entry => entry.created).map(entry => entry.file), [path.join('/home/fixture', '.config', 'fish', 'conf.d', 'airk-path.fish')]);
});

test('Windows PATH removal keeps other entries and the value kind using an isolated registry double', { skip: process.platform !== 'win32' }, async () => {
    const owned = ['C:\\Users\\AIRK\'s name\\storage\\service-cli\\bin', 'C:\\Users\\AIRK\'s name\\storage\\cli\\bin'];
    // Exercise the production script without writing the real registry or broadcasting a change.
    const script = windowsPathRemovalScript(owned)
        .replace("[Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')", '$airkFake')
        .replace('if ($airkRemoved -gt 0) {\n    Add-Type', 'if ($false) {\n    Add-Type');
    const harness = `$airkFake = [pscustomobject]@{ Value = '${owned[0].replace(/'/g, "''")};%USERPROFILE%\\keep;C:\\Tools;"${owned[1].replace(/'/g, "''")}\\"'; Kind = [Microsoft.Win32.RegistryValueKind]::ExpandString; Writes = 0 }
$airkFake | Add-Member ScriptMethod GetValueNames { @('Path') }
$airkFake | Add-Member ScriptMethod GetValue { param($name, $default, $options) $this.Value }
$airkFake | Add-Member ScriptMethod GetValueKind { param($name) $this.Kind }
$airkFake | Add-Member ScriptMethod SetValue { param($name, $value, $kind) $this.Value = $value; $this.Kind = $kind; $this.Writes++ }
$airkFake | Add-Member ScriptMethod Dispose { }
${script}
Write-Output (ConvertTo-Json @{ value = $airkFake.Value; kind = [string]$airkFake.Kind; writes = $airkFake.Writes } -Compress)`;
    const powershell = path.join(process.env.SystemRoot!, 'System32/WindowsPowerShell/v1.0/powershell.exe');
    const lines = (await native.runSystemText(powershell, ['-NoProfile', '-NonInteractive', '-EncodedCommand', Buffer.from(harness, 'utf16le').toString('base64')])).trim().split(/\r?\n/);
    assert.deepEqual(JSON.parse(lines[0]), { removed: 2 });
    assert.deepEqual(JSON.parse(lines[1]), { value: '%USERPROFILE%\\keep;C:\\Tools', kind: 'ExpandString', writes: 1 });
});

test('Windows removal calls PowerShell through the system runner, never the AIRK runner', { skip: process.platform !== 'win32' }, async context => {
    const root = await fixture(context);
    const system = context.mock.method(native, 'runSystemText', async () => '{"removed":1}');
    const airk = context.mock.method(native, 'runText', async () => { throw new Error('AIRK runner used'); });
    const result = await removeShellIntegration(path.join(root, 'storage'), [], 'win32');
    assert.equal(system.mock.callCount(), 1); assert.equal(airk.mock.callCount(), 0);
    assert.deepEqual(result.removed, ['Removed 1 AIRK entry from the user PATH.']);
});
