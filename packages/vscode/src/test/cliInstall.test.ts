// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { execFileSync } from 'node:child_process';
import { configureShellPath, installCli, publishCli, windowsPathScript } from '../cliInstall';
import * as native from '../native';

async function fixture(context: { after(fn: () => Promise<void>): void }): Promise<string> {
    const root = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-cli-install-'));
    context.after(() => fs.rm(root, { recursive: true, force: true }));
    return fs.realpath(root);
}

test('stable executable updates without stale copies, preserves old binary on missing input, and is idempotent', async context => {
    const root = await fixture(context), helper = path.join(root, 'verified-helper');
    await fs.writeFile(helper, 'first version');
    const directory = await publishCli(helper, path.join(root, 'Editor Storage'));
    const executable = path.join(directory, process.platform === 'win32' ? 'airk.exe' : 'airk');
    const first = await fs.stat(executable);
    await publishCli(helper, path.join(root, 'Editor Storage'));
    assert.equal((await fs.stat(executable)).mtimeMs, first.mtimeMs);
    await fs.writeFile(helper, 'next version');
    assert.equal(await publishCli(helper, path.join(root, 'Editor Storage')), directory);
    assert.equal(await fs.readFile(executable, 'utf8'), 'next version');
    await assert.rejects(publishCli(path.join(root, 'missing'), path.join(root, 'Editor Storage')), /ENOENT/);
    assert.equal(await fs.readFile(executable, 'utf8'), 'next version');
    assert.deepEqual(await fs.readdir(directory), [path.basename(executable)]);
    if (process.platform !== 'win32') { assert.equal((await fs.stat(executable)).mode & 0o777, 0o755); }
});

test('shell setup preserves startup commands and login selection, respects custom directories and repeats without duplicates', async context => {
    const root = await fixture(context), home = path.join(root, 'home');
    await fs.mkdir(home);
    const original = '# personal settings\nexport ORIGINAL=yes';
    await fs.writeFile(path.join(home, '.bash_login'), original);
    await fs.writeFile(path.join(home, '.bashrc'), original);
    const env = { ZDOTDIR: path.join(home, 'custom zsh'), XDG_CONFIG_HOME: path.join(home, 'custom config') };
    // Use a POSIX absolute path on every OS; profile rendering does not execute it here.
    const directory = '/opt/AIRK test\'s $(ignored)/bin';
    assert.deepEqual(await configureShellPath(directory, home, env), []);
    const bash = await fs.readFile(path.join(home, '.bashrc'), 'utf8');
    assert.ok(bash.startsWith(original + '\n'));
    assert.match(bash, /'"'"'/);
    assert.match(bash, /\$\(ignored\)/);
    await assert.rejects(fs.access(path.join(home, '.bash_profile')), /ENOENT/);
    assert.match(await fs.readFile(path.join(home, '.bash_login'), 'utf8'), /AIRK CLI PATH/);
    assert.match(await fs.readFile(path.join(env.ZDOTDIR, '.zshrc'), 'utf8'), /AIRK CLI PATH/);
    assert.match(await fs.readFile(path.join(env.XDG_CONFIG_HOME, 'fish/conf.d/airk-path.fish'), 'utf8'), /set -gx PATH/);
    assert.deepEqual(await configureShellPath(directory, home, env), []);
    assert.equal(await fs.readFile(path.join(home, '.bashrc'), 'utf8'), bash);
});

test('unsupported PATH characters and blocked profiles report failure without losing other shell setup', async context => {
    const root = await fixture(context);
    await fs.mkdir(path.join(root, '.bashrc'));
    const errors = await configureShellPath('/opt/airk/bin', root, {});
    assert.equal(errors.length, 1); assert.match(errors[0], /non-file shell profile/);
    assert.match(await fs.readFile(path.join(root, '.zprofile'), 'utf8'), /AIRK CLI PATH/);
    await assert.rejects(configureShellPath('/opt/bad:path', root, {}), /cannot be represented/);
    await assert.rejects(configureShellPath('/opt/airk', root, { ZDOTDIR: 'relative' }), /must be absolute/);
});

test('hard-linked and oversized profiles are refused without modifying either file', async context => {
    const root = await fixture(context), original = path.join(root, 'unrelated.txt');
    await fs.writeFile(original, 'preserve original bytes');
    await fs.link(original, path.join(root, '.profile'));
    await fs.writeFile(path.join(root, '.bashrc'), Buffer.alloc(1_048_577, 0x61));
    const errors = await configureShellPath('/opt/airk/bin', root, {});
    assert.equal(errors.length, 2);
    assert.ok(errors.some(error => error.includes('hard-linked')));
    assert.ok(errors.some(error => error.includes('1 MiB')));
    assert.equal(await fs.readFile(original, 'utf8'), 'preserve original bytes');
    assert.equal((await fs.stat(path.join(root, '.bashrc'))).size, 1_048_577);
});

test('redirected profile directories are refused without writing to their targets', async context => {
    const root = await fixture(context), target = path.join(root, 'target'), redirected = path.join(root, 'redirected');
    await fs.mkdir(target);
    await fs.symlink(target, redirected, process.platform === 'win32' ? 'junction' : 'dir');
    const errors = await configureShellPath('/opt/airk/bin', root, { ZDOTDIR: redirected });
    assert.equal(errors.length, 2);
    assert.ok(errors.every(error => error.includes('redirected shell profile directory')));
    assert.deepEqual(await fs.readdir(target), []);
});

test('replacement between inspection and opening cannot append to an unrelated file', async context => {
    const root = await fixture(context), profile = path.join(root, '.profile'), unrelated = path.join(root, 'unrelated');
    await fs.writeFile(profile, 'original profile');
    await fs.writeFile(unrelated, 'unrelated content');
    const promises = require('node:fs/promises') as typeof fs;
    const originalOpen = promises.open;
    let replaced = false;
    context.mock.method(promises, 'open', async (...args: Parameters<typeof fs.open>) => {
        if (args[0] === profile && !replaced) {
            replaced = true;
            await fs.rename(profile, path.join(root, 'original-profile'));
            await fs.link(unrelated, profile);
        }
        return originalOpen(...args);
    });
    const errors = await configureShellPath('/opt/airk/bin', root, {});
    assert.equal(errors.length, 1);
    assert.equal(await fs.readFile(unrelated, 'utf8'), 'unrelated content');
    assert.equal(await fs.readFile(path.join(root, 'original-profile'), 'utf8'), 'original profile');
});

test('POSIX startup executes safely, keeps arguments intact and adds PATH only once', { skip: process.platform === 'win32' }, async context => {
    const root = await fixture(context), home = path.join(root, 'home');
    const directory = path.join(root, "space ' $() bin");
    await fs.mkdir(directory); await fs.mkdir(home);
    await fs.writeFile(path.join(directory, 'airk'), '#!/bin/sh\nprintf "%s\\n" "$@"\n', { mode: 0o755 });
    assert.deepEqual(await configureShellPath(directory, home, {}), []);
    const output = execFileSync('/bin/sh', ['-c', '. "$1"; . "$1"; airk "two words" "literal $ value"; printf "%s" "$PATH"', 'fixture', path.join(home, '.profile')],
        { encoding: 'utf8', env: { ...process.env, PATH: '/usr/bin:/bin' } });
    assert.equal(output, `two words\nliteral $ value\n${directory}:/usr/bin:/bin`);
});

test('Windows PATH script preserves expandable values and is idempotent using an isolated registry double', { skip: process.platform !== 'win32' }, async () => {
    const directory = "C:\\Users\\AIRK's name\\bin";
    // Exercise the production script without writing the real registry or broadcasting a change.
    const script = windowsPathScript(directory)
        .replace("[Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')", '$airkFake')
        .replace('if (-not $airkFound) {\n    Add-Type', 'if ($false) {\n    Add-Type');
    const harness = `$airkFake = [pscustomobject]@{ Value = '%USERPROFILE%\\existing;C:\\Tools'; Writes = 0 }
$airkFake | Add-Member ScriptMethod GetValue { param($name, $fallback, $options) $this.Value }
$airkFake | Add-Member ScriptMethod GetValueNames { @('Path') }
$airkFake | Add-Member ScriptMethod GetValueKind { [Microsoft.Win32.RegistryValueKind]::ExpandString }
$airkFake | Add-Member ScriptMethod SetValue { param($name, $value, $kind) if ($kind -ne [Microsoft.Win32.RegistryValueKind]::ExpandString) { throw 'Kind lost' }; $this.Value = $value; $this.Writes++ }
$airkFake | Add-Member ScriptMethod Dispose {}
${script}
${script}
@{ value = $airkFake.Value; writes = $airkFake.Writes } | ConvertTo-Json -Compress`;
    const powershell = path.join(process.env.SystemRoot!, 'System32/WindowsPowerShell/v1.0/powershell.exe');
    const result = JSON.parse(await native.runText(powershell, ['-NoProfile', '-NonInteractive', '-EncodedCommand', Buffer.from(harness, 'utf16le').toString('base64')]));
    assert.equal(result.value, `%USERPROFILE%\\existing;C:\\Tools;${directory}`);
    assert.equal(result.writes, 1);
});

test('Windows installer exposes a real executable in fresh terminal PATH without Cargo', { skip: process.platform !== 'win32' }, async context => {
    const root = await fixture(context);
    // A real executable proves command discovery; no Rust build or live registry edit is needed.
    let terminalPath = '';
    const runner = context.mock.method(native, 'runText', async () => '');
    const variables = { persistent: false, description: '', prepend: (name: string, value: string) => {
        assert.equal(name, 'PATH'); terminalPath = value;
    } };
    await installCli(process.execPath, { globalStorageUri: { fsPath: path.join(root, 'Editor Storage') }, environmentVariableCollection: variables } as any);
    assert.equal(variables.persistent, true); assert.equal(runner.mock.callCount(), 1);
    const command = runner.mock.calls[0].arguments;
    assert.match(command[0]!, /WindowsPowerShell/);
    assert.ok(command[1]!.includes('-EncodedCommand'));
    const output = execFileSync(path.join(process.env.SystemRoot!, 'System32/WindowsPowerShell/v1.0/powershell.exe'),
        ['-NoProfile', '-NonInteractive', '-Command', 'airk --version'],
        { encoding: 'utf8', env: { ...process.env, PATH: terminalPath + process.env.PATH } });
    assert.equal(output.trim(), process.version);
});
