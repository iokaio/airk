// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { execFileSync } from 'node:child_process';
import { configureShellPath, installCli, installLauncherCli, publishCli, publishLauncherCommand, retireCliExecutables, windowsPathScript } from '../cliInstall';
import * as native from '../native';

async function fixture(context: { after(fn: () => Promise<void>): void }): Promise<string> {
    const root = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-cli-install-'));
    context.after(() => fs.rm(root, { recursive: true, force: true }));
    return fs.realpath(root);
}

test('service CLI forwards to the original launcher and updates idempotently',async context=>{
    const root=await fixture(context),launcher=path.join(root,'approved runtime','airk-launcher.exe');
    const directory=await publishLauncherCommand(launcher,root,'win32'),command=path.join(directory,'airk.cmd');
    const content=await fs.readFile(command,'utf8');
    assert.ok(content.includes(`"${launcher}" %*`));assert.match(content,/exit \/b %errorlevel%/);
    const before=await fs.stat(command);await publishLauncherCommand(launcher,root,'win32');
    assert.equal((await fs.stat(command)).mtimeMs,before.mtimeMs);
    await assert.rejects(publishLauncherCommand(path.join(root,'bad%PATH%'),root,'win32'),/cannot be represented/);
    if(process.platform!=='win32'){
        const executable=path.join(root,"launcher with ' quotes");
        await fs.writeFile(executable,'#!/bin/sh\nprintf "%s\\n" "$@"\nexit 7\n',{mode:0o755});
        const bin=await publishLauncherCommand(executable,root);
        const result=require('node:child_process').spawnSync(path.join(bin,'airk'),['space argument','$(literal)'],{encoding:'utf8'});
        assert.equal(result.status,7);assert.equal(result.stdout,'space argument\n$(literal)\n');
    }
});

test('CLI retirement removes previous binaries of any version while preserving current commands and unrelated files',async context=>{
    const root=await fixture(context),launcher=path.join(root,'approved','airk-launcher.exe');
    const bin=await publishLauncherCommand(launcher,root,'win32');
    await fs.mkdir(path.join(root,'cli','bin'),{recursive:true});
    const old=path.join(root,'cli','bin','airk.exe'),shadow=path.join(bin,'airk.exe');
    const unrelated=path.join(root,'cli','bin','notes.txt');await fs.writeFile(unrelated,'preserve');
    for(const version of ['1.3.1','1.4.0','1.5.0']){
        await fs.writeFile(old,`old AIRK ${version}`);await fs.writeFile(shadow,`previous AIRK ${version}`);
        assert.deepEqual(await retireCliExecutables(root,'win32'),[]);
        await assert.rejects(fs.access(old),/ENOENT/);await assert.rejects(fs.access(shadow),/ENOENT/);
    }
    const wrapper=await fs.readFile(path.join(bin,'airk.cmd'),'utf8');assert.ok(wrapper.includes(launcher));
    assert.equal(await fs.readFile(unrelated,'utf8'),'preserve');
    await fs.writeFile(path.join(root,'cli','bin','airk'),'previous POSIX CLI');
    assert.deepEqual(await retireCliExecutables(root,'linux'),[]);
    await assert.rejects(fs.access(path.join(root,'cli','bin','airk')),/ENOENT/);
    assert.deepEqual(await retireCliExecutables(root,'win32'),[]);
});

test('locked CLI retirement reports failure, continues independent cleanup and succeeds on retry',async context=>{
    const root=await fixture(context),old=path.join(root,'cli','bin','airk.exe'),shadow=path.join(root,'service-cli','bin','airk.exe');
    await fs.mkdir(path.dirname(old),{recursive:true});await fs.mkdir(path.dirname(shadow),{recursive:true});
    await fs.writeFile(old,'locked');await fs.writeFile(shadow,'obsolete');
    const promises=require('node:fs/promises') as typeof fs,unlink=promises.unlink;
    let locked=true;
    context.mock.method(promises,'unlink',async(file:Parameters<typeof fs.unlink>[0])=>{
        if(file===old&&locked)throw Object.assign(new Error('Sharing violation'),{code:'EPERM'});
        return unlink(file);
    });
    const errors=await retireCliExecutables(root,'win32');assert.equal(errors.length,1);
    assert.match(errors[0],/Disconnect clients.*Install or Repair CLI/);
    assert.equal(await fs.readFile(old,'utf8'),'locked');await assert.rejects(fs.access(shadow),/ENOENT/);
    locked=false;assert.deepEqual(await retireCliExecutables(root,'win32'),[]);
    await assert.rejects(fs.access(old),/ENOENT/);
});

test('CLI retirement preserves redirected directories, hard links and non-file targets',async context=>{
    const root=await fixture(context),target=path.join(root,'unrelated');await fs.mkdir(target);
    await fs.mkdir(path.join(root,'cli'));await fs.symlink(target,path.join(root,'cli','bin'),process.platform==='win32'?'junction':'dir');
    await fs.writeFile(path.join(target,'airk.exe'),'preserve');
    assert.equal((await retireCliExecutables(root,'win32')).length,1);
    assert.equal(await fs.readFile(path.join(target,'airk.exe'),'utf8'),'preserve');
    await fs.unlink(path.join(root,'cli','bin'));await fs.mkdir(path.join(root,'cli','bin'));
    const old=path.join(root,'cli','bin','airk.exe');await fs.link(path.join(target,'airk.exe'),old);
    assert.equal((await retireCliExecutables(root,'win32')).length,1);await fs.unlink(old);
    await fs.mkdir(old);assert.equal((await retireCliExecutables(root,'win32')).length,1);
    assert.equal(await fs.readFile(path.join(target,'airk.exe'),'utf8'),'preserve');
});

test('service CLI install retries retirement even when its current wrapper is unchanged',{skip:process.platform!=='win32'},async context=>{
    const root=await fixture(context),old=path.join(root,'cli','bin','airk.exe');
    await fs.mkdir(path.dirname(old),{recursive:true});await fs.writeFile(old,'previous version');
    context.mock.method(native,'runText',async()=> '');
    const variables={persistent:false,description:'',prepend:()=>{}};
    const host={globalStorageUri:{fsPath:root},environmentVariableCollection:variables} as any;
    await publishLauncherCommand(process.execPath,root);
    assert.deepEqual(await installLauncherCli(process.execPath,host),[]);
    await assert.rejects(fs.access(old),/ENOENT/);
    await fs.writeFile(old,'another previous version');
    assert.deepEqual(await installLauncherCli(process.execPath,host),[]);
    await assert.rejects(fs.access(old),/ENOENT/);
});

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
    assert.equal(result.value, `${directory};%USERPROFILE%\\existing;C:\\Tools`);
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

test('Windows service command executes the selected launcher with original arguments and exit code', {skip:process.platform!=='win32'},async context=>{
    const root=await fixture(context),bin=await publishLauncherCommand(process.execPath,root);
    const powershell=path.join(process.env.SystemRoot!,'System32/WindowsPowerShell/v1.0/powershell.exe');
    const version=execFileSync(powershell,['-NoProfile','-NonInteractive','-Command','airk --version'],
        {encoding:'utf8',env:{...process.env,PATH:bin+path.delimiter+process.env.PATH}});
    assert.equal(version.trim(),process.version);
    const script=path.join(root,'arguments.js');await fs.writeFile(script,'console.log(JSON.stringify(process.argv.slice(2)));process.exit(7);');
    const result=require('node:child_process').spawnSync(powershell,['-NoProfile','-NonInteractive','-Command',`airk '${script.replaceAll("'","''")}' 'space argument' 'literal-value'; exit $LASTEXITCODE`],
        {encoding:'utf8',env:{...process.env,PATH:bin+path.delimiter+process.env.PATH}});
    assert.equal(result.status,7);assert.deepEqual(JSON.parse(result.stdout),['space argument','literal-value']);
});
