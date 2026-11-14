// SPDX-License-Identifier: Apache-2.0
//! I4 reader: the install pointer and install record (docs/split/contracts.md
//! §5.2–§5.4; plan §8.2). Pure decoding mirrors
//! crates/airk-core/src/install_record/mod.rs and is tested against the shared
//! fixtures in fixtures/split/i4/i4_cases.json. Filesystem, ownership and
//! signature checks live in ./verify.ts.
import { createHash } from 'node:crypto';
import * as path from 'node:path';

export const POINTER_SCHEMA = 'airk-install-pointer/1';
export const RECORD_SCHEMA = 'airk-install-record/1';
export const MAX_POINTER_BYTES = 4 * 1024;
export const MAX_RECORD_BYTES = 64 * 1024;
export const RECORD_FILE = 'install-record.json';

/** Refusal codes, identical to `Refusal::code` in the Rust owner. */
export type InstallRefusalCode =
    | 'install_file_oversize' | 'install_file_malformed' | 'install_file_unknown_schema' | 'install_file_non_canonical'
    | 'install_record_digest_mismatch' | 'install_id_mismatch' | 'install_version_mismatch'
    | 'install_location_relative' | 'install_executable_outside_program'
    | 'replacement_notice_expired' | 'replacement_notice_lifetime_too_long';

export class InstallRefusal extends Error {
    constructor(readonly code: InstallRefusalCode | string, detail?: string) {
        super(detail ? `${code}: ${detail}` : code);
        this.name = 'InstallRefusal';
    }
}

export interface InstallPointer {
    schema: string; install_id: string; program: string; record: string;
    record_sha256: string; version: string; written_unix_ms: number;
}
export interface Executable { path: string; sha256: string; bytes: number }
export type Signing =
    | { platform: 'windows'; publisher: string; identity_eku: string }
    | { platform: 'macos'; team_id: string }
    | { platform: 'linux'; signed: boolean; integrity: string };
export interface InstallRecord {
    schema: string; install_id: string; version: string; channel: string; target: string;
    contracts: { i1_protocol: number; i2_contract: number; i3_admin: number; i4_record: number; state: string };
    executables: { airkctl: Executable; airk: Executable };
    admin_assets: { inventory_sha256: string; bundle_sha256: string };
    locations: { program: string; data: string; state: string; cache: string; logs: string };
    signing: Signing;
    installed_by: { operation_id: string; installer: 'inno' | 'dmg' | 'app_first_launch' | 'install_sh' | 'portable'; installed_unix_ms: number };
}

const INCARNATION = /^[0-9a-f]{32}$/;
const SHA256 = /^[0-9a-f]{64}$/;
type Json = Record<string, unknown>;
type Field = (value: unknown) => unknown;

// Field readers. Each returns the canonical value or throws `malformed`, the
// same outcome serde gives for a type error, a missing or an unknown field.
const malformed = (detail: string) => new InstallRefusal('install_file_malformed', detail);
const str: Field = value => { if (typeof value !== 'string') { throw malformed('string'); } return value; };
const u64: Field = value => { if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0) { throw malformed('integer'); } return value; };
const u32: Field = value => { const n = u64(value) as number; if (n > 0xffff_ffff) { throw malformed('u32'); } return n; };
const bool: Field = value => { if (typeof value !== 'boolean') { throw malformed('boolean'); } return value; };
const incarnation: Field = value => { if (typeof value !== 'string' || !INCARNATION.test(value)) { throw malformed('incarnation'); } return value; };
const sha256: Field = value => { if (typeof value !== 'string' || !SHA256.test(value)) { throw malformed('sha256'); } return value; };
const oneOf = (...values: string[]): Field => value => { if (typeof value !== 'string' || !values.includes(value)) { throw malformed('variant'); } return value; };

/** A struct with `deny_unknown_fields`; returns an object in declaration order. */
function struct(fields: [string, Field][]): Field {
    return value => {
        if (!value || typeof value !== 'object' || Array.isArray(value)) { throw malformed('object'); }
        const input = value as Json, names = fields.map(([name]) => name);
        for (const key of Object.keys(input)) { if (!names.includes(key)) { throw malformed(`unknown field ${key}`); } }
        const output: Json = {};
        for (const [name, read] of fields) {
            if (!Object.hasOwn(input, name)) { throw malformed(`missing field ${name}`); }
            output[name] = read(input[name]);
        }
        return output;
    };
}
/** An internally tagged enum (`#[serde(tag = "platform")]`); the tag serializes first. */
function tagged(tag: string, variants: Record<string, [string, Field][]>): Field {
    return value => {
        if (!value || typeof value !== 'object' || Array.isArray(value)) { throw malformed('object'); }
        const name = (value as Json)[tag];
        if (typeof name !== 'string' || !Object.hasOwn(variants, name)) { throw malformed('variant'); }
        const rest = { ...(value as Json) }; delete rest[tag];
        return { [tag]: name, ...(struct(variants[name])(rest) as Json) };
    };
}

const POINTER = struct([
    ['schema', str], ['install_id', incarnation], ['program', str], ['record', str],
    ['record_sha256', sha256], ['version', str], ['written_unix_ms', u64],
]);
const EXECUTABLE: [string, Field][] = [['path', str], ['sha256', sha256], ['bytes', u64]];
const LOCATIONS = struct([['program', str], ['data', str], ['state', str], ['cache', str], ['logs', str]]);
const RECORD = struct([
    ['schema', str], ['install_id', incarnation], ['version', str], ['channel', str], ['target', str],
    ['contracts', struct([['i1_protocol', u32], ['i2_contract', u32], ['i3_admin', u32], ['i4_record', u32], ['state', str]])],
    ['executables', struct([['airkctl', struct(EXECUTABLE)], ['airk', struct(EXECUTABLE)]])],
    ['admin_assets', struct([['inventory_sha256', sha256], ['bundle_sha256', sha256]])],
    ['locations', LOCATIONS],
    ['signing', tagged('platform', {
        windows: [['publisher', str], ['identity_eku', str]],
        macos: [['team_id', str]],
        linux: [['signed', bool], ['integrity', str]],
    })],
    ['installed_by', struct([['operation_id', incarnation], ['installer', oneOf('inno', 'dmg', 'app_first_launch', 'install_sh', 'portable')], ['installed_unix_ms', u64]])],
]);

/** Persisted bytes are one compact JSON line in declaration order, then a newline. */
export function canonical(value: unknown): string { return JSON.stringify(value) + '\n'; }

function decode<T>(bytes: Buffer, limit: number, schema: string, read: Field): T {
    if (bytes.length > limit) { throw new InstallRefusal('install_file_oversize'); }
    let text: string, parsed: unknown;
    try { text = new TextDecoder('utf-8', { fatal: true }).decode(bytes); parsed = JSON.parse(text); }
    catch { throw malformed('json'); }
    const value = read(parsed) as Json;
    if (value.schema !== schema) { throw new InstallRefusal('install_file_unknown_schema'); }
    if (canonical(value) !== text) { throw new InstallRefusal('install_file_non_canonical'); }
    return value as T;
}

/** Same rule as the Rust owner: `/…` or `X:\…` (with at least one more byte). */
export function absoluteLocation(location: string): boolean {
    return location.startsWith('/')
        || Buffer.byteLength(location, 'utf8') > 3 && /^[A-Za-z]:\\/.test(location);
}
/** A record executable path: relative, `/`-separated, no empty, `.` or `..` part. */
export function insideProgram(relative: string): boolean {
    return relative.length > 0 && !absoluteLocation(relative) && !relative.includes('\\')
        && relative.split('/').every(part => part.length > 0 && part !== '.' && part !== '..');
}

export function decodePointer(bytes: Buffer): InstallPointer {
    const pointer = decode<InstallPointer>(bytes, MAX_POINTER_BYTES, POINTER_SCHEMA, POINTER);
    if (!absoluteLocation(pointer.program)) { throw new InstallRefusal('install_location_relative'); }
    if (pointer.record !== RECORD_FILE) { throw malformed('record file name'); }
    return pointer;
}

export const REPLACEMENT_NOTICE_SCHEMA = 'airk-install-replacing/1';
export const REPLACEMENT_NOTICE_FILE = 'replacing.json';
export const MAX_REPLACEMENT_NOTICE_BYTES = 4 * 1024;
export const MAX_REPLACEMENT_NOTICE_LIFETIME_MS = 10 * 60 * 1000;
/** Setup is replacing or removing the installed files (`ReplacementNotice`). */
export interface ReplacementNotice { schema: string; operation_id: string; program: string; created_unix_ms: number; expires_unix_ms: number }
const NOTICE = struct([['schema', str], ['operation_id', incarnation], ['program', str], ['created_unix_ms', u64], ['expires_unix_ms', u64]]);
/** Decode a notice against the caller's clock; only a returned notice is in force. */
export function decodeReplacementNotice(bytes: Buffer, nowUnixMs: number): ReplacementNotice {
    const notice = decode<ReplacementNotice>(bytes, MAX_REPLACEMENT_NOTICE_BYTES, REPLACEMENT_NOTICE_SCHEMA, NOTICE);
    if (!absoluteLocation(notice.program)) { throw new InstallRefusal('install_location_relative'); }
    if (notice.expires_unix_ms <= notice.created_unix_ms || notice.expires_unix_ms - notice.created_unix_ms > MAX_REPLACEMENT_NOTICE_LIFETIME_MS) {
        throw new InstallRefusal('replacement_notice_lifetime_too_long');
    }
    if (nowUnixMs >= notice.expires_unix_ms || nowUnixMs < notice.created_unix_ms) { throw new InstallRefusal('replacement_notice_expired'); }
    return notice;
}
/** `replacing.json` beside the pointer. */
export function replacementNoticePath(home: string, platform: HostPlatform): string {
    const pointer = pointerPath(home, platform);
    return (platform === 'win32' ? path.win32 : path.posix).join((platform === 'win32' ? path.win32 : path.posix).dirname(pointer), REPLACEMENT_NOTICE_FILE);
}

/** Decode a record and check it against the pointer that named it. */
export function decodeRecord(bytes: Buffer, pointer: InstallPointer): InstallRecord {
    if (createHash('sha256').update(bytes).digest('hex') !== pointer.record_sha256) { throw new InstallRefusal('install_record_digest_mismatch'); }
    const record = decode<InstallRecord>(bytes, MAX_RECORD_BYTES, RECORD_SCHEMA, RECORD);
    if (record.install_id !== pointer.install_id) { throw new InstallRefusal('install_id_mismatch'); }
    if (record.version !== pointer.version) { throw new InstallRefusal('install_version_mismatch'); }
    const locations = record.locations;
    if ([locations.program, locations.data, locations.state, locations.cache, locations.logs].some(location => !absoluteLocation(location))
        || locations.program !== pointer.program) { throw new InstallRefusal('install_location_relative'); }
    if (!insideProgram(record.executables.airk.path) || !insideProgram(record.executables.airkctl.path)) {
        throw new InstallRefusal('install_executable_outside_program');
    }
    return record;
}

export type HostPlatform = 'win32' | 'darwin' | 'linux';
/**
 * The fixed per-user pointer path (contracts §5.2). The base is
 * `os.userInfo().homedir` (passwd on Unix, the token profile on Windows), never
 * `HOME`, `LOCALAPPDATA` or `XDG_CONFIG_HOME`. Windows Known Folder redirection
 * of LocalAppData is open question Q5 and is not followed here.
 */
export function pointerPath(home: string, platform: HostPlatform): string {
    switch (platform) {
        case 'win32': return path.win32.join(home, 'AppData', 'Local', 'AIRK', 'install-pointer', 'current.json');
        case 'darwin': return path.posix.join(home, 'Library', 'Application Support', 'AIRK', 'install-pointer', 'current.json');
        default: return path.posix.join(home, '.config', 'airk', 'install-pointer', 'current.json');
    }
}

/**
 * Pre-split install roots (docs/split/inventory/predecessors.md §1). Their
 * presence is reported only: the extension never uses or changes them (U12).
 */
export function predecessorRoots(home: string, platform: HostPlatform): string[] {
    const leaves = ['service-1.4', 'service-1.4-preview', 'runtime', 'runtime-preview', 'launcher', 'launcher-preview'];
    switch (platform) {
        case 'win32': {
            const base = path.win32.join(home, 'AppData', 'Local', 'AIRK');
            return [...leaves, 'standalone'].map(leaf => path.win32.join(base, leaf));
        }
        case 'darwin': return leaves.map(leaf => path.posix.join(home, 'Library', 'Application Support', 'AIRK', leaf));
        default: return leaves.map(leaf => path.posix.join(home, '.local', 'lib', 'airk', leaf));
    }
}

/** The absolute path of a record executable inside the program folder. */
export function executablePath(record: InstallRecord, which: 'airkctl' | 'airk', platform: HostPlatform): string {
    const join = platform === 'win32' ? path.win32.join : path.posix.join;
    return join(record.locations.program, ...record.executables[which].path.split('/'));
}
