// SPDX-License-Identifier: Apache-2.0
import * as fs from 'node:fs/promises';
import { constants, Stats } from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { randomUUID } from 'node:crypto';
import type { ExtensionContext } from 'vscode';
import { runText } from './native';

const marker = '# AIRK CLI PATH';
const quote = (value: string): string => `'${value.replace(/'/g, `'"'"'`)}'`;

function validatePath(directory: string, platform: NodeJS.Platform): void {
    if (!path.isAbsolute(directory) || /[\r\n\0]/.test(directory) || directory.includes(platform === 'win32' ? ';' : ':')) {
        throw new Error(`AIRK CLI directory cannot be represented in PATH: ${directory}`);
    }
}

/** Only called with the digest/version-verified helper returned by prepareHelper. */
export async function publishCli(helper: string, storage: string, platform: NodeJS.Platform = process.platform): Promise<string> {
    const directory = path.join(storage, 'cli', 'bin');
    validatePath(directory, platform);
    await fs.mkdir(directory, { recursive: true });
    const executable = path.join(directory, platform === 'win32' ? 'airk.exe' : 'airk');
    const bytes = await fs.readFile(helper);
    try {
        const info = await fs.lstat(executable);
        if (!info.isFile() || info.isSymbolicLink()) { throw new Error(`AIRK CLI destination is not a regular file: ${executable}`); }
        if (bytes.equals(await fs.readFile(executable))) {
            if (platform !== 'win32') { await fs.chmod(executable, 0o755); }
            return directory;
        }
    } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') { throw error; } }
    const temporary = path.join(directory, `${randomUUID()}.tmp`);
    try {
        await fs.writeFile(temporary, bytes, { flag: 'wx', mode: 0o755 });
        // Atomic replacement keeps the previous CLI intact if a running Windows process locks it.
        await fs.rename(temporary, executable);
    } finally { await fs.unlink(temporary).catch(() => undefined); }
    return directory;
}

const profileLimit = 1_048_576;

function checkProfile(info: Stats, file: string): void {
    if (!info.isFile() || info.isSymbolicLink() || info.nlink !== 1) {
        throw new Error(`Configure PATH manually in redirected, hard-linked or non-file shell profile: ${file}`);
    }
    if (info.size > profileLimit) { throw new Error(`Shell profile exceeds the 1 MiB editing limit: ${file}`); }
}

function sameProfile(before: Stats, after: Stats): boolean {
    return before.dev === after.dev && before.ino === after.ino && before.size === after.size
        && before.mtimeMs === after.mtimeMs && before.ctimeMs === after.ctimeMs;
}

async function profileDirectory(directory: string): Promise<void> {
    const parent = path.dirname(directory);
    if (parent !== directory) { await profileDirectory(parent); }
    if (parent !== directory) {
        try { await fs.mkdir(directory); }
        catch (error) { if ((error as NodeJS.ErrnoException).code !== 'EEXIST') { throw error; } }
    }
    const info = await fs.lstat(directory);
    if (!info.isDirectory() || info.isSymbolicLink()) {
        throw new Error(`Configure PATH manually in redirected shell profile directory: ${directory}`);
    }
}

/** Append through the verified handle; never reopen an inspected pathname for writing. */
async function appendProfile(file: string, block: string): Promise<void> {
    await profileDirectory(path.dirname(file));
    let before: Stats | undefined;
    try {
        before = await fs.lstat(file);
        checkProfile(before, file);
    } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') { throw error; } }
    // Exclusive creation protects the absent-file case; O_NOFOLLOW protects the final
    // component on platforms that expose it. Identity checks also run on Windows.
    const flags = constants.O_RDWR | constants.O_APPEND | (constants.O_NOFOLLOW || 0)
        | (before ? 0 : constants.O_CREAT | constants.O_EXCL);
    const handle = await fs.open(file, flags, 0o600);
    try {
        const opened = await handle.stat();
        checkProfile(opened, file);
        if (before && !sameProfile(before, opened)) { throw new Error(`Shell profile changed; retry setup: ${file}`); }
        const buffer = Buffer.alloc(profileLimit + 1);
        let length = 0;
        while (length < buffer.length) {
            const { bytesRead } = await handle.read(buffer, length, buffer.length - length, length);
            if (!bytesRead) { break; }
            length += bytesRead;
        }
        if (length > profileLimit) { throw new Error(`Shell profile exceeds the 1 MiB editing limit: ${file}`); }
        const existing = new TextDecoder('utf-8', { fatal: true }).decode(buffer.subarray(0, length));
        const current = await fs.lstat(file), held = await handle.stat();
        await profileDirectory(path.dirname(file));
        checkProfile(current, file); checkProfile(held, file);
        if (!sameProfile(opened, held) || !sameProfile(held, current)) {
            throw new Error(`Shell profile changed; retry setup: ${file}`);
        }
        if (existing.includes(block)) { return; }
        const addition = Buffer.from(`\n${block}\n`);
        if (length + addition.length > profileLimit) { throw new Error(`Shell profile exceeds the 1 MiB editing limit: ${file}`); }
        await handle.writeFile(addition);
        await handle.sync();
    } finally { await handle.close(); }
}

export async function configureShellPath(directory: string, home: string, env: NodeJS.ProcessEnv): Promise<string[]> {
    validatePath(directory, 'linux');
    const literal = quote(directory);
    const block = `${marker}\nif [ -d ${literal} ]; then\n    case ":$PATH:" in\n        *:${literal}:*) ;;\n        *) export PATH=${literal}:"$PATH" ;;\n    esac\nfi\n# End AIRK CLI PATH`;
    const profiles = new Set([path.join(home, '.profile'), path.join(home, '.bashrc')]);
    // Bash reads only the first existing login profile. Do not create one that masks user setup.
    for (const name of ['.bash_profile', '.bash_login']) {
        const file = path.join(home, name);
        try { await fs.lstat(file); profiles.add(file); break; }
        catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') { throw error; } }
    }
    const zsh = env.ZDOTDIR || home;
    const config = env.XDG_CONFIG_HOME || path.join(home, '.config');
    if (!path.isAbsolute(zsh) || !path.isAbsolute(config)) { throw new Error('ZDOTDIR and XDG_CONFIG_HOME must be absolute to install AIRK CLI shell integration.'); }
    profiles.add(path.join(zsh, '.zprofile')); profiles.add(path.join(zsh, '.zshrc'));
    const errors: string[] = [];
    for (const file of profiles) {
        try { await appendProfile(file, block); } catch (error) { errors.push(String(error)); }
    }
    // fish single quotes escape backslashes and apostrophes, unlike POSIX shell quotes.
    const fishLiteral = `'${directory.replace(/\\/g, '\\\\').replace(/'/g, "\\'")}'`;
    const fish = `${marker}\nif test -d ${fishLiteral}\n    if not contains -- ${fishLiteral} $PATH\n        set -gx PATH ${fishLiteral} $PATH\n    end\nend\n# End AIRK CLI PATH`;
    try { await appendProfile(path.join(config, 'fish', 'conf.d', 'airk-path.fish'), fish); }
    catch (error) { errors.push(String(error)); }
    return errors;
}

/** Preserve the raw user PATH and registry value kind; never copy the merged process PATH. */
export function windowsPathScript(directory: string): string {
    const literal = `'${directory.replace(/'/g, "''")}'`;
    return `$ErrorActionPreference = 'Stop'
$airkBin = ${literal}
$airkKey = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
try {
    $airkOld = [string]$airkKey.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    $airkKind = if ($airkKey.GetValueNames() -contains 'Path') { $airkKey.GetValueKind('Path') } else { [Microsoft.Win32.RegistryValueKind]::ExpandString }
    if ($airkKind -ne [Microsoft.Win32.RegistryValueKind]::String -and $airkKind -ne [Microsoft.Win32.RegistryValueKind]::ExpandString) { throw 'User PATH has an unsupported registry type.' }
    $airkFound = @($airkOld.Split(';') | Where-Object { [Environment]::ExpandEnvironmentVariables($_).Trim().Trim('"').TrimEnd('\\') -ieq $airkBin.TrimEnd('\\') }).Count -gt 0
    if (-not $airkFound) {
        $airkSeparator = if ($airkOld) { ';' } else { '' }
        $airkKey.SetValue('Path', $airkBin + $airkSeparator + $airkOld, $airkKind)
    }
} finally { $airkKey.Dispose() }
if (-not $airkFound) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class AirkPathNotification {
    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern IntPtr SendMessageTimeout(IntPtr window, uint message, UIntPtr wparam, string lparam, uint flags, uint timeout, out UIntPtr result);
}
'@
    $airkResult = [UIntPtr]::Zero
    [void][AirkPathNotification]::SendMessageTimeout([IntPtr]0xffff, 0x001a, [UIntPtr]::Zero, 'Environment', 2, 3000, [ref]$airkResult)
}`;
}

export async function installCli(helper: string, context: Pick<ExtensionContext, 'globalStorageUri' | 'environmentVariableCollection'>): Promise<string[]> {
    const directory = await publishCli(helper, context.globalStorageUri.fsPath);
    return installPath(directory,context);
}

/** The verified native installation owns this launcher; never copy an extension helper over it. */
export async function installLauncherCli(launcher:string,context:Pick<ExtensionContext,'environmentVariableCollection'|'globalStorageUri'>):Promise<string[]>{
    const directory=await publishLauncherCommand(launcher,context.globalStorageUri.fsPath);
    const errors=await installPath(directory,context);
    errors.push(...await retireCliExecutables(context.globalStorageUri.fsPath));
    return errors;
}

/** Retire only obsolete binaries at extension-owned CLI locations, regardless of version.
 * Never execute them, traverse redirected directories, stop clients or delete native runtime slots.
 * Missing files are complete; a locked file remains an explicit failure for the next install attempt.
 */
export async function retireCliExecutables(storage:string,platform:NodeJS.Platform=process.platform):Promise<string[]>{
    validatePath(storage,process.platform);
    const targets=platform==='win32'
        ?[['cli','bin','airk.exe'],['service-cli','bin','airk.exe']]
        :[['cli','bin','airk']];
    const errors:string[]=[];
    for(const parts of targets){
        const executable=path.join(storage,...parts);
        try{
            let directory=storage;
            for(const part of ['',...parts.slice(0,-1)]){
                directory=path.join(directory,part);
                const info=await fs.lstat(directory);
                if(!info.isDirectory()||info.isSymbolicLink())throw new Error('Redirected or non-directory CLI location preserved');
            }
            const info=await fs.lstat(executable);
            if(!info.isFile()||info.isSymbolicLink()||info.nlink!==1)throw new Error('Non-regular, redirected or hard-linked CLI executable preserved');
            await fs.unlink(executable);
        }catch(error){
            if((error as NodeJS.ErrnoException).code==='ENOENT')continue;
            errors.push(`Could not remove previous AIRK CLI ${executable}: ${String(error)}. Disconnect clients using this executable, close old terminals, then run AIRK: Install or Repair CLI. Automatic installation retries on the next trusted activation.`);
        }
    }
    return errors;
}

/** Keep the stable launcher's original path and native installation identity. */
export async function publishLauncherCommand(launcher:string,storage:string,platform:NodeJS.Platform=process.platform):Promise<string>{
    validatePath(launcher,platform);
    if(platform==='win32'&&/["%!]/.test(launcher))throw new Error('AIRK launcher path cannot be represented in a Windows command wrapper.');
    const directory=path.join(storage,'service-cli','bin');validatePath(directory,platform);
    await fs.mkdir(directory,{recursive:true});
    const command=path.join(directory,platform==='win32'?'airk.cmd':'airk');
    const content=platform==='win32'?`@echo off\r\n"${launcher}" %*\r\nexit /b %errorlevel%\r\n`:`#!/bin/sh\nexec ${quote(launcher)} "$@"\n`;
    try{
        const info=await fs.lstat(command);
        if(!info.isFile()||info.isSymbolicLink()||info.nlink!==1)throw new Error('AIRK command destination must be a regular unlinked file.');
        if(await fs.readFile(command,'utf8')===content)return directory;
    }catch(error){if((error as NodeJS.ErrnoException).code!=='ENOENT')throw error;}
    const temporary=path.join(directory,`${randomUUID()}.tmp`);
    try{await fs.writeFile(temporary,content,{flag:'wx',mode:0o755});await fs.rename(temporary,command);}
    finally{await fs.unlink(temporary).catch(()=>undefined);}
    return directory;
}

async function installPath(directory:string,context:Pick<ExtensionContext,'environmentVariableCollection'>):Promise<string[]>{
    const variables = context.environmentVariableCollection;
    variables.persistent = true;
    variables.description = 'Makes the installed AIRK CLI available in new terminals.';
    variables.prepend('PATH', directory + path.delimiter);
    if (process.platform === 'win32') {
        const powershell = path.join(process.env.SystemRoot || 'C:\\Windows', 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe');
        await runText(powershell, ['-NoLogo', '-NoProfile', '-NonInteractive', '-EncodedCommand',
            Buffer.from(windowsPathScript(directory), 'utf16le').toString('base64')], undefined, 15000);
        return [];
    }
    if (process.platform !== 'darwin' && process.platform !== 'linux') { throw new Error(`AIRK CLI PATH setup is unsupported on ${process.platform}.`); }
    return configureShellPath(directory, os.homedir(), process.env);
}
