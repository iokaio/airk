// SPDX-License-Identifier: Apache-2.0
//! Find and verify the installed `airkctl` before its first execution (plan
//! §8.2, contracts §5.4). Steps: read the pointer at its fixed per-user path
//! and the record it names, require owner-private files and folders with no
//! links or redirects, then check the executable's size and SHA-256 against the
//! record and its platform signature (Windows Authenticode, macOS codesign;
//! Linux rests on the record digest, documented as weaker). Results are cached
//! by file identity and re-verified when that identity changes.
import * as fs from 'node:fs/promises';
import { createReadStream } from 'node:fs';
import { createHash } from 'node:crypto';
import * as os from 'node:os';
import * as path from 'node:path';
import {
    decodePointer, decodeRecord, executablePath, InstallRefusal, MAX_POINTER_BYTES, MAX_RECORD_BYTES, pointerPath, predecessorRoots,
    RECORD_FILE, type HostPlatform, type InstallPointer, type InstallRecord,
} from './install';
import { SIGNING_IDENTITY } from './signingIdentity';
import { spawnBounded, type Spawner } from './runner';

export interface FileFacts {
    readonly kind: 'file' | 'directory' | 'link' | 'other';
    readonly size: number; readonly nlink: number; readonly uid?: number; readonly mode?: number;
    readonly dev: number; readonly ino: number; readonly mtimeMs: number;
}
/** One Windows ACL observation: owner SID and every SID an allow ACE names. */
export interface AclObservation { readonly path: string; readonly owner: string; readonly allowed: readonly string[] }
export interface WindowsObservation {
    readonly user: string; readonly acl: readonly AclObservation[];
    readonly signature?: { readonly status: string; readonly subject: string; readonly ekus: readonly string[] };
}
/** The filesystem and platform facts verification reads. Tests inject fixtures. */
export interface VerifyHost {
    readonly platform: HostPlatform;
    readonly home: string;
    /** Effective uid on Unix. */
    readonly uid?: number;
    /** Unsigned `airkctl` is accepted only in development or test mode (plan §8.2). */
    readonly developmentMode: boolean;
    lstat(file: string): Promise<FileFacts | undefined>;
    realpath(file: string): Promise<string>;
    read(file: string, limit: number): Promise<Buffer>;
    sha256(file: string): Promise<string>;
    /** Windows: owner/ACL of `paths` and the Authenticode signature of `signed`. */
    windows?(paths: readonly string[], signed: string): Promise<WindowsObservation>;
    /** macOS: `codesign --verify --strict` against the team requirement. */
    codesign?(file: string, teamId: string): Promise<boolean>;
}

export type Verification =
    | { readonly state: 'not_found'; readonly predecessor: boolean; readonly pointer: string }
    | { readonly state: 'unverified'; readonly code: string; readonly detail: string; readonly pointer: string }
    | {
        readonly state: 'verified'; readonly pointer: InstallPointer; readonly record: InstallRecord;
        readonly airkctl: string; readonly sha256: string;
        /** `signed` or, in development/test mode only, `unsigned_development`; Linux is `digest_only`. */
        readonly signature: 'signed' | 'unsigned_development' | 'digest_only';
        readonly identity: string;
    };

const SYSTEM = 'S-1-5-18', ADMINISTRATORS = 'S-1-5-32-544';
class Unverified extends Error { constructor(readonly code: string, readonly detail = code) { super(detail); } }

const samePath = (a: string, b: string, platform: HostPlatform) => {
    const join = platform === 'win32' ? path.win32 : path.posix;
    const left = join.normalize(a), right = join.normalize(b);
    return platform === 'win32' ? left.toLowerCase() === right.toLowerCase() : left === right;
};
function ancestors(file: string, platform: HostPlatform): string[] {
    const api = platform === 'win32' ? path.win32 : path.posix;
    const result: string[] = [];
    for (let current = api.normalize(file); ; current = api.dirname(current)) {
        result.push(current);
        if (api.dirname(current) === current) { return result.reverse(); }
    }
}

/** Pure Windows ACL rule (airk-ipc `require_owner_private`): owner is the user
 * or Administrators; allow ACEs name only the user, SYSTEM or Administrators. */
export function windowsOwnerPrivate(observation: AclObservation, user: string): string | undefined {
    if (observation.owner !== user && observation.owner !== ADMINISTRATORS) { return 'owned by another account'; }
    const other = observation.allowed.find(sid => sid !== user && sid !== SYSTEM && sid !== ADMINISTRATORS);
    return other ? `access granted to ${other}` : undefined;
}
/** Pure Authenticode rule: a valid signature whose signer is the C10 publisher
 * and whose certificate carries the C10 identity EKU. */
export function authenticodeAccepted(signature: WindowsObservation['signature']): string | undefined {
    if (!signature) { return 'no signature observation'; }
    if (signature.status !== 'Valid') { return `signature status ${signature.status}`; }
    if (signature.subject !== SIGNING_IDENTITY.windows.publisher) { return 'signer is not the AIRK publisher'; }
    if (!signature.ekus.includes(SIGNING_IDENTITY.windows.identity_eku)) { return 'signer lacks the AIRK identity EKU'; }
    return undefined;
}

export class AirkctlVerifier {
    private cache?: { key: string; result: Verification & { state: 'verified' } };
    constructor(private readonly host: VerifyHost) {}

    async verify(): Promise<Verification> {
        const pointerFile = pointerPath(this.host.home, this.host.platform);
        try {
            const pointerFacts = await this.host.lstat(pointerFile);
            if (!pointerFacts) {
                return { state: 'not_found', predecessor: await this.predecessor(), pointer: pointerFile };
            }
            return await this.verifyFrom(pointerFile);
        } catch (error) {
            if (error instanceof Unverified) { return { state: 'unverified', code: error.code, detail: error.detail, pointer: pointerFile }; }
            if (error instanceof InstallRefusal) { return { state: 'unverified', code: error.code, detail: error.message, pointer: pointerFile }; }
            return { state: 'unverified', code: 'installation_unreadable', detail: error instanceof Error ? error.message : String(error), pointer: pointerFile };
        }
    }

    private async predecessor(): Promise<boolean> {
        for (const root of predecessorRoots(this.host.home, this.host.platform)) {
            try { if (await this.host.lstat(root)) { return true; } } catch { /* Unknown is not presence. */ }
        }
        return false;
    }

    /** Every component exists, none is a link, and the real path is the path. */
    private async noLinks(file: string): Promise<FileFacts> {
        let facts: FileFacts | undefined;
        for (const component of ancestors(file, this.host.platform)) {
            facts = await this.host.lstat(component);
            if (!facts) { throw new Unverified('installation_incomplete', `${component} is missing`); }
            if (facts.kind === 'link') { throw new Unverified('installation_link', `${component} is a link or reparse point`); }
        }
        const real = await this.host.realpath(file);
        if (!samePath(real, file, this.host.platform)) { throw new Unverified('installation_redirected', `${file} resolves elsewhere`); }
        return facts!;
    }
    private unixPrivate(file: string, facts: FileFacts): void {
        if (this.host.platform === 'win32') { return; }
        if (facts.uid !== this.host.uid) { throw new Unverified('installation_not_owner_private', `${file} is owned by another account`); }
        if (((facts.mode ?? 0o777) & 0o022) !== 0) { throw new Unverified('installation_not_owner_private', `${file} is writable by others`); }
    }
    private async regular(file: string, limit?: number): Promise<FileFacts> {
        const facts = await this.noLinks(file);
        if (facts.kind !== 'file' || facts.nlink !== 1) { throw new Unverified('installation_not_owner_private', `${file} is not a single-link regular file`); }
        if (limit !== undefined && facts.size > limit) { throw new InstallRefusal('install_file_oversize'); }
        this.unixPrivate(file, facts);
        return facts;
    }
    private async folder(dir: string): Promise<FileFacts> {
        const facts = await this.noLinks(dir);
        if (facts.kind !== 'directory') { throw new Unverified('installation_incomplete', `${dir} is not a folder`); }
        this.unixPrivate(dir, facts);
        return facts;
    }

    private async verifyFrom(pointerFile: string): Promise<Verification> {
        const api = this.host.platform === 'win32' ? path.win32 : path.posix;
        const pointerFacts = await this.regular(pointerFile, MAX_POINTER_BYTES);
        await this.folder(api.dirname(pointerFile));
        const pointer = decodePointer(await this.host.read(pointerFile, MAX_POINTER_BYTES));
        if (this.host.platform !== 'win32' && pointer.program.includes('\\') || this.host.platform === 'win32' && pointer.program.startsWith('/')) {
            throw new Unverified('install_location_relative', 'the program folder is not a path on this platform');
        }
        const recordFile = api.join(pointer.program, RECORD_FILE);
        const recordFacts = await this.regular(recordFile, MAX_RECORD_BYTES);
        await this.folder(pointer.program);
        const record = decodeRecord(await this.host.read(recordFile, MAX_RECORD_BYTES), pointer);
        const airkctl = executablePath(record, 'airkctl', this.host.platform);
        const exe = await this.regular(airkctl);
        const key = JSON.stringify([pointerFacts, recordFacts, exe, pointer.record_sha256]);
        if (this.cache?.key === key) { return this.cache.result; }
        if (exe.size !== record.executables.airkctl.bytes) { throw new Unverified('installation_digest_mismatch', 'airkctl size differs from the install record'); }
        const sha256 = await this.host.sha256(airkctl);
        if (sha256 !== record.executables.airkctl.sha256) { throw new Unverified('installation_digest_mismatch', 'airkctl SHA-256 differs from the install record'); }
        const signature = await this.signature(record, airkctl, [pointerFile, api.dirname(pointerFile), recordFile, pointer.program, airkctl]);
        const result = { state: 'verified' as const, pointer, record, airkctl, sha256, signature, identity: key };
        this.cache = { key, result };
        return result;
    }

    private async signature(record: InstallRecord, airkctl: string, owned: string[]): Promise<'signed' | 'unsigned_development' | 'digest_only'> {
        const unsigned = (reason: string): 'unsigned_development' => {
            if (this.host.developmentMode) { return 'unsigned_development'; }
            throw new Unverified('installation_signature', reason);
        };
        switch (this.host.platform) {
            case 'win32': {
                if (!this.host.windows) { throw new Unverified('installation_signature', 'no Windows verification adapter'); }
                const observation = await this.host.windows(owned, airkctl);
                for (const file of owned) {
                    const acl = observation.acl.find(entry => samePath(entry.path, file, 'win32'));
                    const failure = acl ? windowsOwnerPrivate(acl, observation.user) : 'no ACL observation';
                    if (failure) { throw new Unverified('installation_not_owner_private', `${file}: ${failure}`); }
                }
                if (record.signing.platform !== 'windows' || record.signing.publisher !== SIGNING_IDENTITY.windows.publisher
                    || record.signing.identity_eku !== SIGNING_IDENTITY.windows.identity_eku) {
                    return unsigned('the install record names another signing identity');
                }
                const failure = authenticodeAccepted(observation.signature);
                return failure ? unsigned(failure) : 'signed';
            }
            case 'darwin': {
                if (record.signing.platform !== 'macos' || record.signing.team_id !== SIGNING_IDENTITY.macos.team_id) {
                    return unsigned('the install record names another signing identity');
                }
                if (!this.host.codesign) { throw new Unverified('installation_signature', 'no codesign adapter'); }
                return await this.host.codesign(airkctl, SIGNING_IDENTITY.macos.team_id) ? 'signed' : unsigned('codesign verification failed');
            }
            default:
                // Linux packages are unsigned: integrity is the owner-private record digest (weaker, documented).
                if (record.signing.platform !== 'linux' || record.signing.integrity !== 'record-sha256') {
                    throw new Unverified('installation_signature', 'the install record names another signing platform');
                }
                return 'digest_only';
        }
    }
}

// ---- Production host -------------------------------------------------------

const WINDOWS_SCRIPT = `
$ErrorActionPreference = 'Stop'
$request = $env:AIRK_VERIFY_REQUEST | ConvertFrom-Json
$sid = [System.Security.Principal.SecurityIdentifier]
$result = @{ user = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value; acl = @() }
foreach ($item in $request.paths) {
  $acl = Get-Acl -LiteralPath $item
  $allowed = @($acl.GetAccessRules($true, $true, $sid) | Where-Object { $_.AccessControlType -eq 'Allow' } | ForEach-Object { $_.IdentityReference.Value })
  $result.acl += @{ path = $item; owner = $acl.GetOwner($sid).Value; allowed = $allowed }
}
$signature = Get-AuthenticodeSignature -LiteralPath $request.signed
$ekus = @()
if ($signature.SignerCertificate) {
  foreach ($extension in $signature.SignerCertificate.Extensions) {
    if ($extension -is [System.Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension]) {
      foreach ($usage in $extension.EnhancedKeyUsages) { $ekus += $usage.Value }
    }
  }
}
$result.signature = @{ status = [string]$signature.Status; subject = [string]$signature.SignerCertificate.Subject; ekus = $ekus }
$result | ConvertTo-Json -Depth 5 -Compress
`;

function windowsAdapter(spawner: Spawner): VerifyHost['windows'] {
    return async (paths, signed) => {
        const root = process.env.SystemRoot ?? 'C:\\Windows';
        const shell = path.win32.join(root, 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe');
        // Inputs travel in one environment value; the script text is fixed. The
        // module path is pinned to the system modules so a profile or user
        // module cannot replace Get-Acl or Get-AuthenticodeSignature.
        const result = await spawner(shell, ['-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-Command', WINDOWS_SCRIPT], {
            env: { SystemRoot: root, PSModulePath: path.win32.join(root, 'System32', 'WindowsPowerShell', 'v1.0', 'Modules'),
                AIRK_VERIFY_REQUEST: JSON.stringify({ paths, signed }) },
            timeoutMs: 30_000, maxStdout: 256 * 1024, maxStderr: 64 * 1024,
        });
        if (result.exitCode !== 0 || result.timedOut || result.overflow) { throw new Unverified('installation_signature', 'the Windows verification step failed'); }
        const value = JSON.parse(result.stdout.toString('utf8')) as Record<string, unknown>;
        const list = (input: unknown) => (Array.isArray(input) ? input : input === undefined || input === null ? [] : [input]).map(String);
        const signature = value.signature as Record<string, unknown> | undefined;
        return {
            user: String(value.user),
            acl: (Array.isArray(value.acl) ? value.acl : value.acl ? [value.acl] : []).map(entry => {
                const item = entry as Record<string, unknown>;
                return { path: String(item.path), owner: String(item.owner), allowed: list(item.allowed) };
            }),
            ...(signature ? { signature: { status: String(signature.status), subject: String(signature.subject ?? ''), ekus: list(signature.ekus) } } : {}),
        };
    };
}

function codesignAdapter(spawner: Spawner): VerifyHost['codesign'] {
    return async (file, teamId) => {
        if (!/^[A-Z0-9]{10}$/.test(teamId)) { return false; }
        const requirement = `=anchor apple generic and certificate leaf[subject.OU] = "${teamId}"`;
        const result = await spawner('/usr/bin/codesign', ['--verify', '--strict', '-R', requirement, file],
            { env: {}, timeoutMs: 30_000, maxStdout: 64 * 1024, maxStderr: 64 * 1024 });
        return result.exitCode === 0 && !result.timedOut;
    };
}

async function streamSha256(file: string): Promise<string> {
    const hash = createHash('sha256');
    for await (const block of createReadStream(file)) { hash.update(block as Buffer); }
    return hash.digest('hex');
}
async function boundedRead(file: string, limit: number): Promise<Buffer> {
    const handle = await fs.open(file, 'r');
    try {
        const buffer = Buffer.alloc(limit + 1);
        let used = 0;
        while (used < buffer.length) {
            const { bytesRead } = await handle.read(buffer, used, buffer.length - used, null);
            if (!bytesRead) { break; } used += bytesRead;
        }
        if (used > limit) { throw new InstallRefusal('install_file_oversize'); }
        return buffer.subarray(0, used);
    } finally { await handle.close(); }
}

export function systemVerifyHost(developmentMode: boolean, spawner: Spawner = spawnBounded): VerifyHost {
    const platform = (process.platform === 'win32' || process.platform === 'darwin' ? process.platform : 'linux') as HostPlatform;
    return {
        platform, developmentMode,
        // passwd home on Unix, the token profile on Windows (contracts §5.2).
        home: os.userInfo().homedir,
        ...(typeof process.geteuid === 'function' ? { uid: process.geteuid() } : {}),
        async lstat(file) {
            let info;
            try { info = await fs.lstat(file); } catch (error) {
                if ((error as NodeJS.ErrnoException).code === 'ENOENT') { return undefined; }
                throw error;
            }
            return {
                kind: info.isSymbolicLink() ? 'link' : info.isFile() ? 'file' : info.isDirectory() ? 'directory' : 'other',
                size: info.size, nlink: info.nlink, uid: info.uid, mode: info.mode, dev: info.dev, ino: info.ino, mtimeMs: info.mtimeMs,
            };
        },
        realpath: file => fs.realpath(file),
        read: boundedRead,
        sha256: streamSha256,
        ...(platform === 'win32' ? { windows: windowsAdapter(spawner) } : {}),
        ...(platform === 'darwin' ? { codesign: codesignAdapter(spawner) } : {}),
    };
}
