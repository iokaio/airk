// SPDX-License-Identifier: Apache-2.0
// Windows owner-only directory ACLs for editor-written descriptors. Runs only the
// fixed system tools whoami.exe and icacls.exe with fixed arguments; never AIRK,
// a shell or anything derived from drafts or requests.
import * as childProcess from 'node:child_process';
import * as path from 'node:path';

const restricted = new Set<string>();
/** Windows ignores POSIX modes and profile ACLs may grant extra groups (for example
 * sandbox or backup accounts). Replace inherited entries with the current user,
 * SYSTEM and Administrators so descriptors and credentials stay private. */
export function restrictWindowsDirectory(directory: string, run: typeof childProcess.execFileSync = childProcess.execFileSync): void {
    if (process.platform !== 'win32' || restricted.has(directory)) { return; }
    const system32 = path.join(process.env.SystemRoot || 'C:\\Windows', 'System32');
    const user = /"(S-1-5-[0-9-]+)"/.exec(String(run(path.join(system32, 'whoami.exe'), ['/user', '/fo', 'csv', '/nh'], { encoding: 'utf8', windowsHide: true })))?.[1];
    if (!user) { throw new Error('Could not resolve the current Windows account for the Assistance descriptor ACL.'); }
    run(path.join(system32, 'icacls.exe'), [directory, '/inheritance:r', '/grant:r', `*${user}:(OI)(CI)F`, '*S-1-5-18:(OI)(CI)F', '*S-1-5-32-544:(OI)(CI)F'],
        { windowsHide: true, stdio: 'ignore' });
    restricted.add(directory);
}
