// SPDX-License-Identifier: Apache-2.0
/** Reverse AIRK terminal-command integration without the editor API.
 * Only exact AIRK-written blocks, PATH entries and wrappers for this extension's
 * own storage directories are removed; anything modified or redirected is reported. */
import * as fs from 'node:fs/promises';
import { constants } from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { fishBlock, posixBlock, retireCliExecutables, ShellTouch } from './cliInstall';
import { runSystemText } from './native';

const marker = '# AIRK CLI PATH', profileLimit = 1_048_576;

/** Startup files setup may have edited, for installations made before receipts existed.
 * Only the AIRK-named fish file counts as created; other files are never deleted. */
export function defaultProfiles(home: string = os.homedir(), env: NodeJS.ProcessEnv = process.env): ShellTouch[] {
    const zsh = env.ZDOTDIR && path.isAbsolute(env.ZDOTDIR) ? env.ZDOTDIR : home;
    const config = env.XDG_CONFIG_HOME && path.isAbsolute(env.XDG_CONFIG_HOME) ? env.XDG_CONFIG_HOME : path.join(home, '.config');
    return [...['.profile', '.bashrc', '.bash_profile', '.bash_login'].map(name => ({ file: path.join(home, name), created: false })),
        ...['.zprofile', '.zshrc'].map(name => ({ file: path.join(zsh, name), created: false })),
        { file: path.join(config, 'fish', 'conf.d', 'airk-path.fish'), created: true }];
}

/** Current wrapper directory first, then the retired executable-copy layout. */
export function ownedCommandDirectories(storage: string): string[] {
    return [path.join(storage, 'service-cli', 'bin'), path.join(storage, 'cli', 'bin')];
}

/** Remove owned blocks through the verified handle; delete a file only if AIRK created it and nothing else remains. */
async function removeProfileBlocks(file: string, blocks: string[], created: boolean): Promise<{ changed: boolean; message?: string }> {
    let before;
    try { before = await fs.lstat(file); } catch (error) { if ((error as NodeJS.ErrnoException).code === 'ENOENT') { return { changed: false }; } throw error; }
    if (!before.isFile() || before.isSymbolicLink() || before.nlink !== 1 || before.size > profileLimit) {
        return { changed: false, message: `Preserved redirected, hard-linked, non-file or oversized shell profile; remove the "${marker}" block manually: ${file}` };
    }
    const handle = await fs.open(file, constants.O_RDWR | (constants.O_NOFOLLOW || 0));
    try {
        const opened = await handle.stat();
        if (opened.ino !== before.ino || opened.dev !== before.dev || opened.size !== before.size) { return { changed: false, message: `Shell profile changed during cleanup; retry: ${file}` }; }
        const text = new TextDecoder('utf-8', { fatal: true }).decode(await handle.readFile());
        let next = text;
        for (const block of blocks) {
            for (const form of [`\n${block}\n`, `${block}\n`, block]) { next = next.split(form).join(''); }
        }
        const remaining = next.includes(marker) ? `Preserved a modified or unrecognized "${marker}" block in ${file}; review it manually.` : undefined;
        if (next === text) { return { changed: false, message: remaining }; }
        if (created && next.trim() === '') {
            await handle.close();
            const current = await fs.lstat(file);
            if (current.ino !== opened.ino || current.dev !== opened.dev) { return { changed: false, message: `Shell profile changed during cleanup; retry: ${file}` }; }
            await fs.unlink(file);
            return { changed: true, message: remaining };
        }
        await handle.truncate(0);
        await handle.write(Buffer.from(next), 0, Buffer.byteLength(next), 0);
        await handle.sync();
        return { changed: true, message: remaining };
    } finally { await handle.close().catch(() => undefined); }
}

const wrapperShape = /^(?:#!\/bin\/sh\nexec '[^\n]*' "\$@"\n|@echo off\r\n"[^\r\n]*" %\*\r\nexit \/b %errorlevel%\r\n)$/;
async function removeWrapper(directory: string): Promise<string | undefined> {
    for (const name of ['airk', 'airk.cmd']) {
        const file = path.join(directory, name);
        try {
            const info = await fs.lstat(file);
            if (!info.isFile() || info.isSymbolicLink() || info.nlink !== 1 || info.size > 8192 || !wrapperShape.test(await fs.readFile(file, 'utf8'))) {
                return `Preserved an unrecognized terminal command; remove it manually if unwanted: ${file}`;
            }
            await fs.unlink(file);
        } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') { throw error; } }
    }
    return undefined;
}

/** Removes only exact entries for `directories`; preserves value kind and every other entry. */
export function windowsPathRemovalScript(directories: string[]): string {
    const literals = directories.map(directory => `'${directory.replace(/'/g, "''").replace(/\\+$/, '')}'`).join(', ');
    return `$ErrorActionPreference = 'Stop'
$airkOwned = @(${literals})
$airkKey = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
$airkRemoved = 0
try {
    if ($airkKey.GetValueNames() -contains 'Path') {
        $airkOld = [string]$airkKey.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        $airkKind = $airkKey.GetValueKind('Path')
        if ($airkKind -ne [Microsoft.Win32.RegistryValueKind]::String -and $airkKind -ne [Microsoft.Win32.RegistryValueKind]::ExpandString) { throw 'User PATH has an unsupported registry type.' }
        $airkParts = @($airkOld.Split(';'))
        $airkKept = @($airkParts | Where-Object {
            $airkEntry = [Environment]::ExpandEnvironmentVariables($_).Trim().Trim('"').TrimEnd('\\')
            @($airkOwned | Where-Object { $_ -ieq $airkEntry }).Count -eq 0
        })
        $airkRemoved = $airkParts.Count - $airkKept.Count
        if ($airkRemoved -gt 0) { $airkKey.SetValue('Path', ($airkKept -join ';'), $airkKind) }
    }
} finally { $airkKey.Dispose() }
if ($airkRemoved -gt 0) {
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
}
Write-Output (ConvertTo-Json @{ removed = $airkRemoved } -Compress)`;
}

export interface ShellRemoval { removed: string[]; preserved: string[] }

/** Safe to repeat. `profiles` lists startup files recorded at installation. */
export async function removeShellIntegration(storage: string, profiles: readonly ShellTouch[], platform: NodeJS.Platform = process.platform,
    options: { wrappers?: boolean } = {}): Promise<ShellRemoval> {
    if (!path.isAbsolute(storage) || /[\r\n\0]/.test(storage)) { throw new Error('Invalid AIRK extension storage path'); }
    const result: ShellRemoval = { removed: [], preserved: [] };
    const directories = ownedCommandDirectories(storage);
    if (platform === 'win32') {
        const powershell = path.join(process.env.SystemRoot || 'C:\\Windows', 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe');
        const output = await runSystemText(powershell, ['-NoLogo', '-NoProfile', '-NonInteractive', '-EncodedCommand',
            Buffer.from(windowsPathRemovalScript(directories), 'utf16le').toString('base64')], undefined, 15000);
        const removed = Number(JSON.parse(output.trim() || '{"removed":0}').removed);
        if (removed > 0) { result.removed.push(`Removed ${removed} AIRK entr${removed === 1 ? 'y' : 'ies'} from the user PATH.`); }
    } else {
        const blocks = directories.flatMap(directory => [posixBlock(directory), fishBlock(directory)]);
        for (const { file, created } of profiles) {
            try {
                const { changed, message } = await removeProfileBlocks(file, blocks, created);
                if (changed) { result.removed.push(`Removed AIRK PATH block from ${file}`); }
                if (message) { result.preserved.push(message); }
            } catch (error) { result.preserved.push(`Could not update ${file}: ${String(error)}`); }
        }
    }
    // VS Code terminals keep using the wrapper while the extension stays installed.
    if (options.wrappers === false) { return result; }
    for (const directory of directories) {
        const message = await removeWrapper(directory);
        if (message) { result.preserved.push(message); }
    }
    result.preserved.push(...await retireCliExecutables(storage, platform));
    // Remove now-empty owned directories, innermost first; anything left is preserved.
    for (const directory of directories.flatMap(directory => [directory, path.dirname(directory)])) {
        try { await fs.rmdir(directory); } catch (error) { if (!['ENOENT', 'ENOTEMPTY', 'EEXIST'].includes((error as NodeJS.ErrnoException).code ?? '')) { result.preserved.push(`Could not remove ${directory}: ${String(error)}`); } }
    }
    return result;
}
