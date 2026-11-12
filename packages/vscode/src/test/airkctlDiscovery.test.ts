// SPDX-License-Identifier: Apache-2.0
// Discovery and verification of the installed `airkctl` (plan §8.2, §8.3;
// contracts §3.5, §3.6, §5.2–§5.4). Expectations come from the shared,
// independently reviewed fixtures in fixtures/split/i2 and fixtures/split/i4,
// and from the rows of the contracts' tables, never from the code under test.
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { createHash } from 'node:crypto';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { canonical, decodePointer, decodeRecord, InstallRefusal, pointerPath, predecessorRoots } from '../airkctl/install';
import { AirkctlVerifier, authenticodeAccepted, windowsOwnerPrivate, type FileFacts, type VerifyHost, type WindowsObservation } from '../airkctl/verify';
import { compatibility, EXTENSION_REQUIREMENTS, parseVersionDocument, VersionDocumentInvalid } from '../airkctl/version';
import { deriveState, executable } from '../airkctl/states';
import { SIGNING_IDENTITY } from '../airkctl/signingIdentity';
import type { VersionDocument } from '../generated/i2';

const repo = path.resolve(__dirname, '../../../..');
const json = (relative: string) => JSON.parse(fs.readFileSync(path.join(repo, relative), 'utf8'));
const i4 = json('fixtures/split/i4/i4_cases.json') as { pointers: { name: string; text: string; expect: string }[]; records: { name: string; text: string; pointer: string; expect: string }[] };
const outcome = (run: () => unknown) => { try { run(); return 'ok'; } catch (error) { if (error instanceof InstallRefusal) { return error.code; } throw error; } };

test('I4 pointer cases: every reviewed pointer decodes or refuses as the fixture states', () => {
    assert.ok(i4.pointers.length >= 9, 'zero or missing pointer cases is a failure');
    for (const item of i4.pointers) { assert.equal(outcome(() => decodePointer(Buffer.from(item.text, 'utf8'))), item.expect, item.name); }
});

test('I4 record cases: every reviewed record decodes or refuses against its pointer', () => {
    assert.ok(i4.records.length >= 12, 'zero or missing record cases is a failure');
    for (const item of i4.records) {
        const pointer = decodePointer(Buffer.from(item.pointer, 'utf8'));
        assert.equal(outcome(() => decodeRecord(Buffer.from(item.text, 'utf8'), pointer)), item.expect, item.name);
    }
});

test('the pointer path is fixed per user and uses the passwd/profile home only (contracts §5.2)', () => {
    assert.equal(pointerPath('C:\\Users\\u', 'win32'), 'C:\\Users\\u\\AppData\\Local\\AIRK\\install-pointer\\current.json');
    assert.equal(pointerPath('/Users/u', 'darwin'), '/Users/u/Library/Application Support/AIRK/install-pointer/current.json');
    assert.equal(pointerPath('/home/u', 'linux'), '/home/u/.config/airk/install-pointer/current.json');
    const source = fs.readFileSync(path.join(repo, 'packages/vscode/src/airkctl/verify.ts'), 'utf8');
    assert.match(source, /os\.userInfo\(\)\.homedir/);
    assert.doesNotMatch(source, /os\.homedir\(\)|process\.env\.(HOME|LOCALAPPDATA|XDG_CONFIG_HOME)/);
    assert.ok(predecessorRoots('/home/u', 'linux').includes('/home/u/.local/lib/airk/runtime'));
    assert.ok(predecessorRoots('C:\\Users\\u', 'win32').includes('C:\\Users\\u\\AppData\\Local\\AIRK\\standalone'));
});

test('the extension signing identity equals the checked-in C10 file', () => {
    const identity = json('scripts/signing-identity.json');
    assert.equal(SIGNING_IDENTITY.windows.publisher, identity.windows.publisher);
    assert.equal(SIGNING_IDENTITY.windows.identity_eku, identity.windows.identity_eku);
    assert.equal(SIGNING_IDENTITY.macos.team_id, identity.macos.team_id);
});

// ---- Verification over an injected host ---------------------------------------

const USER = 'S-1-5-21-1-2-3-1001';
interface Fake { files: Map<string, { kind: FileFacts['kind']; bytes?: Buffer; size?: number; sha256?: string; mtime?: number; uid?: number; mode?: number }>; hashes: number; observation?: WindowsObservation }
function fakeHost(platform: 'win32' | 'linux', fake: Fake, developmentMode = false): VerifyHost {
    const api = platform === 'win32' ? path.win32 : path.posix;
    const norm = (file: string) => platform === 'win32' ? api.normalize(file).toLowerCase() : api.normalize(file);
    const get = (file: string) => fake.files.get(norm(file));
    return {
        platform, developmentMode, home: platform === 'win32' ? 'C:\\Users\\fixture' : '/home/fixture', uid: 1000,
        async lstat(file) {
            const item = get(file);
            if (item) { return { kind: item.kind, size: item.size ?? item.bytes?.length ?? 0, nlink: 1, uid: item.uid ?? 1000, mode: item.mode ?? 0o600, dev: 1, ino: 1, mtimeMs: item.mtime ?? 1 }; }
            // Ancestors of a known file are folders.
            const prefix = norm(file) + (platform === 'win32' ? '\\' : '/');
            return [...fake.files.keys()].some(key => key.startsWith(prefix) || norm(file) === norm(api.parse(file).root))
                ? { kind: 'directory', size: 0, nlink: 1, uid: 1000, mode: 0o700, dev: 1, ino: 1, mtimeMs: 1 } : undefined;
        },
        realpath: async file => file,
        read: async file => { const item = get(file); if (!item?.bytes) { throw new Error('ENOENT'); } return item.bytes; },
        sha256: async file => { fake.hashes++; return get(file)?.sha256 ?? ''; },
        windows: async paths => fake.observation ?? { user: USER, acl: paths.map(item => ({ path: item, owner: USER, allowed: [USER, 'S-1-5-18', 'S-1-5-32-544'] })),
            signature: { status: 'Valid', subject: SIGNING_IDENTITY.windows.publisher, ekus: ['1.3.6.1.5.5.7.3.3', SIGNING_IDENTITY.windows.identity_eku] } },
    };
}
function windowsInstall(): Fake {
    const valid = i4.records.find(item => item.name === 'valid record')!;
    const pointer = decodePointer(Buffer.from(valid.pointer, 'utf8'));
    const files: Fake['files'] = new Map();
    const add = (file: string, value: Fake['files'] extends Map<string, infer V> ? V : never) => files.set(path.win32.normalize(file).toLowerCase(), value);
    add(pointerPath('C:\\Users\\fixture', 'win32'), { kind: 'file', bytes: Buffer.from(valid.pointer, 'utf8') });
    add(path.win32.join(pointer.program, 'install-record.json'), { kind: 'file', bytes: Buffer.from(valid.text, 'utf8') });
    add(path.win32.join(pointer.program, 'bin', 'airkctl.exe'), { kind: 'file', size: 4_000_000, sha256: '1'.repeat(64) });
    return { files, hashes: 0 };
}

test('no pointer is "not installed"; a pre-split root is reported, never used', async () => {
    const fake: Fake = { files: new Map(), hashes: 0 };
    assert.deepEqual(await new AirkctlVerifier(fakeHost('win32', fake)).verify(), { state: 'not_found', predecessor: false, pointer: pointerPath('C:\\Users\\fixture', 'win32') });
    fake.files.set('c:\\users\\fixture\\appdata\\local\\airk\\runtime', { kind: 'directory' });
    const found = await new AirkctlVerifier(fakeHost('win32', fake)).verify();
    assert.equal(found.state, 'not_found'); assert.equal(found.state === 'not_found' && found.predecessor, true);
});

test('a signed Windows installation verifies, and the result is cached by file identity', async () => {
    const fake = windowsInstall(), verifier = new AirkctlVerifier(fakeHost('win32', fake));
    const first = await verifier.verify();
    assert.equal(first.state, 'verified');
    assert.equal(first.state === 'verified' && first.signature, 'signed');
    assert.equal(first.state === 'verified' && first.airkctl.toLowerCase(), 'c:\\users\\fixture\\appdata\\local\\programs\\airk\\bin\\airkctl.exe');
    await verifier.verify();
    assert.equal(fake.hashes, 1, 'an unchanged file identity is not hashed again');
    fake.files.get('c:\\users\\fixture\\appdata\\local\\programs\\airk\\bin\\airkctl.exe')!.mtime = 2;
    await verifier.verify();
    assert.equal(fake.hashes, 2, 'a changed file identity is verified again');
});

test('verification refuses before any execution: digest, ACL, link, signature', async () => {
    const digest = windowsInstall(); digest.files.get('c:\\users\\fixture\\appdata\\local\\programs\\airk\\bin\\airkctl.exe')!.sha256 = '9'.repeat(64);
    assert.deepEqual(await stateOf(digest), ['unverified', 'installation_digest_mismatch']);
    const size = windowsInstall(); size.files.get('c:\\users\\fixture\\appdata\\local\\programs\\airk\\bin\\airkctl.exe')!.size = 5;
    assert.deepEqual(await stateOf(size), ['unverified', 'installation_digest_mismatch']);
    const shared = windowsInstall();
    shared.observation = { user: USER, acl: [], signature: { status: 'Valid', subject: SIGNING_IDENTITY.windows.publisher, ekus: [SIGNING_IDENTITY.windows.identity_eku] } };
    assert.deepEqual(await stateOf(shared), ['unverified', 'installation_not_owner_private']);
    const linked = windowsInstall(); linked.files.set('c:\\users\\fixture\\appdata\\local\\airk', { kind: 'link' });
    assert.deepEqual(await stateOf(linked), ['unverified', 'installation_link']);
    const notSigned = async (owned: readonly string[]): Promise<WindowsObservation> => ({ user: USER, acl: owned.map(file => ({ path: file, owner: USER, allowed: [USER] })), signature: { status: 'NotSigned', subject: '', ekus: [] } });
    const paths = await new AirkctlVerifier({ ...fakeHost('win32', windowsInstall()), windows: notSigned }).verify();
    assert.equal(paths.state === 'unverified' && paths.code, 'installation_signature', 'an unsigned airkctl is refused in production');
    const development = await new AirkctlVerifier({ ...fakeHost('win32', windowsInstall(), true), windows: notSigned }).verify();
    assert.equal(development.state === 'verified' && development.signature, 'unsigned_development', 'accepted only in development or test mode');
    async function stateOf(fake: Fake) { const result = await new AirkctlVerifier(fakeHost('win32', fake)).verify(); return [result.state, result.state === 'unverified' ? result.code : undefined]; }
});

test('the Windows ACL and Authenticode rules match the reviewed policy', () => {
    assert.equal(windowsOwnerPrivate({ path: 'x', owner: USER, allowed: [USER, 'S-1-5-18', 'S-1-5-32-544'] }, USER), undefined);
    assert.equal(windowsOwnerPrivate({ path: 'x', owner: 'S-1-5-32-544', allowed: [USER] }, USER), undefined);
    assert.match(windowsOwnerPrivate({ path: 'x', owner: 'S-1-5-21-9', allowed: [USER] }, USER)!, /another account/);
    assert.match(windowsOwnerPrivate({ path: 'x', owner: USER, allowed: [USER, 'S-1-1-0'] }, USER)!, /S-1-1-0/);
    assert.equal(authenticodeAccepted({ status: 'Valid', subject: SIGNING_IDENTITY.windows.publisher, ekus: [SIGNING_IDENTITY.windows.identity_eku] }), undefined);
    assert.match(authenticodeAccepted({ status: 'HashMismatch', subject: SIGNING_IDENTITY.windows.publisher, ekus: [SIGNING_IDENTITY.windows.identity_eku] })!, /HashMismatch/);
    assert.match(authenticodeAccepted({ status: 'Valid', subject: 'CN=Someone Else', ekus: [SIGNING_IDENTITY.windows.identity_eku] })!, /publisher/);
    assert.match(authenticodeAccepted({ status: 'Valid', subject: SIGNING_IDENTITY.windows.publisher, ekus: [] })!, /EKU/);
});

test('Linux rests on the owner-private record digest; group- or other-writable files are refused', async () => {
    const home = '/home/fixture', program = '/home/fixture/.local/lib/airk-service';
    const record = { schema: 'airk-install-record/1', install_id: '0d'.repeat(16), version: '1.9.0', channel: 'stable', target: 'x86_64-unknown-linux-gnu',
        contracts: { i1_protocol: 1, i2_contract: 1, i3_admin: 1, i4_record: 1, state: 'airk-state/1' },
        executables: { airkctl: { path: 'bin/airkctl', sha256: '1'.repeat(64), bytes: 10 }, airk: { path: 'service/airk', sha256: '2'.repeat(64), bytes: 20 } },
        admin_assets: { inventory_sha256: '3'.repeat(64), bundle_sha256: '4'.repeat(64) },
        locations: { program, data: '/home/fixture/.local/share/airk-service', state: '/home/fixture/.local/state/airk-service', cache: '/home/fixture/.cache/airk-service', logs: '/home/fixture/.local/state/airk-service/logs' },
        signing: { platform: 'linux', signed: false, integrity: 'record-sha256' },
        installed_by: { operation_id: '0e'.repeat(16), installer: 'install_sh', installed_unix_ms: 1 } };
    const recordText = canonical(record);
    const pointerText = canonical({ schema: 'airk-install-pointer/1', install_id: '0d'.repeat(16), program, record: 'install-record.json',
        record_sha256: createHash('sha256').update(recordText).digest('hex'), version: '1.9.0', written_unix_ms: 2 });
    const make = (mode = 0o600): Fake => ({ hashes: 0, files: new Map([
        [pointerPath(home, 'linux'), { kind: 'file' as const, bytes: Buffer.from(pointerText), mode }],
        [`${program}/install-record.json`, { kind: 'file' as const, bytes: Buffer.from(recordText) }],
        [`${program}/bin/airkctl`, { kind: 'file' as const, size: 10, sha256: '1'.repeat(64) }],
    ]) });
    const verified = await new AirkctlVerifier(fakeHost('linux', make())).verify();
    assert.equal(verified.state === 'verified' && verified.signature, 'digest_only');
    const writable = await new AirkctlVerifier(fakeHost('linux', make(0o620))).verify();
    assert.equal(writable.state === 'unverified' && writable.code, 'installation_not_owner_private');
});

// ---- Version handshake (contracts §3.5, §3.6) ------------------------------------

const versions = json('fixtures/split/i2/version.json') as { documents: { name: string; json: unknown }[]; forward_compatible: { name: string; json: unknown }[] };
test('version documents: every reviewed document parses; unknown fields are ignored', () => {
    for (const item of [...versions.documents, ...versions.forward_compatible]) { assert.doesNotThrow(() => parseVersionDocument(JSON.stringify(item.json)), item.name); }
    assert.throws(() => parseVersionDocument('{'), VersionDocumentInvalid);
    assert.throws(() => parseVersionDocument(JSON.stringify({ ...(versions.documents[0].json as object), schema: 'airkctl-version/2' })), /unknown_schema/);
    assert.throws(() => parseVersionDocument(' '.repeat(64 * 1024 + 1)), /oversize/, 'matrix row 9: an oversized document is refused');
});

const base = (versions.documents[0].json as VersionDocument);
const doc = (changes: Partial<VersionDocument>): VersionDocument => ({ ...structuredClone(base), ...changes });
const superset = ['session/2', 'tools/1', 'events/1', 'window-scope/1', 'assistance/1', 'handoff/1', 'agents/1', 'admin-launch/1'];
test('compatibility matrix rows decide from the contract and capabilities, never product order', () => {
    // Row 1: product versions differ; same contract, superset of capabilities.
    assert.deepEqual(compatibility(doc({ version: '1.9.1', capabilities: superset })), { compatible: true, contract: 1, disabled: [] });
    // Row 2: contract 2 only → incompatible, both versions shown, no session.
    const row2 = compatibility(doc({ i2_contract: 2, i2_contracts_supported: [2], capabilities: superset }));
    assert.equal(row2.compatible, false);
    assert.equal(!row2.compatible && row2.reason, 'unsupported_contract');
    assert.deepEqual(!row2.compatible && row2.airkctl, { version: '1.9.0', contracts: [2] });
    // Row 3: extension {1,2}, airkctl 1 → compatible on 1.
    assert.deepEqual(compatibility(doc({ capabilities: superset }), { ...EXTENSION_REQUIREMENTS, supported: [1, 2] }), { compatible: true, contract: 1, disabled: [] });
    // Row 4: a required capability is missing.
    const row4 = compatibility(doc({ capabilities: superset.filter(item => item !== 'window-scope/1') }), { ...EXTENSION_REQUIREMENTS, required: ['session/2', 'window-scope/1'] });
    assert.equal(!row4.compatible && row4.reason, 'missing_capability'); assert.deepEqual(!row4.compatible && row4.missing, ['window-scope/1']);
    // Row 5: an optional capability is missing → compatible, feature disabled with a reason.
    const row5 = compatibility(doc({ capabilities: superset.filter(item => item !== 'assistance/1') }));
    assert.deepEqual(row5.compatible && row5.disabled, [{ capability: 'assistance/1', feature: 'Explorer Assistance drafts' }]);
    // Row 6: unknown capability strings are ignored.
    assert.deepEqual(compatibility(doc({ capabilities: [...superset, 'future/9'] })), { compatible: true, contract: 1, disabled: [] });
    // Rows 7–8: a mismatched pair or an incomplete installation never runs.
    assert.equal((compatibility(doc({ capabilities: superset, installation: { state: 'complete', pair: 'mismatch' } })) as { reason?: string }).reason, 'pair_mismatch');
    assert.equal((compatibility(doc({ capabilities: superset, installation: { state: 'incomplete' } })) as { reason?: string }).reason, 'installation_incomplete');
    assert.equal((compatibility(versions.documents[1].json as VersionDocument) as { reason?: string }).reason, 'not_installed');
});

// ---- States (plan §8.3) -----------------------------------------------------------

test('every plan §8.3 state is derived, and only verified states execute airkctl', () => {
    const verified = { state: 'verified', signature: 'signed' } as never;
    const document = { version: '1.9.0', capabilities: superset };
    const ok = { compatible: true as const, contract: 1, disabled: [] };
    const cases: [string, Parameters<typeof deriveState>[0]][] = [
        ['gated', { extensionVersion: '1.9.0', gate: 'workspace_untrusted' }],
        ['not_installed', { extensionVersion: '1.9.0', verification: { state: 'not_found', predecessor: true, pointer: 'p' } }],
        ['unverified', { extensionVersion: '1.9.0', verification: { state: 'unverified', code: 'installation_link', detail: 'd', pointer: 'p' } }],
        ['unverified', { extensionVersion: '1.9.0', verification: verified, version: { invalid: 'version_document_oversize' } }],
        ['incompatible', { extensionVersion: '1.9.0', verification: verified, version: { document, compatibility: compatibility(doc({ i2_contract: 2, i2_contracts_supported: [2] })) } }],
        ['stopped', { extensionVersion: '1.9.0', verification: verified, version: { document, compatibility: ok }, status: { value: { schema: 's', installation: { state: 'complete' }, service: { state: 'not_running' } } } }],
        ['refused', { extensionVersion: '1.9.0', verification: verified, version: { document, compatibility: ok }, status: { error: { airkctl_error: 1, class: 'refused', code: 'registry_busy', retryable: true, retry_after_ms: 250, message: 'busy' } } }],
        ['ready', { extensionVersion: '1.9.0', verification: verified, version: { document, compatibility: ok }, status: { value: { schema: 's', installation: { state: 'complete' }, service: { state: 'running', health: {}, explicitly_stopped: false, pending_lifecycle: null } } } }],
    ];
    for (const [kind, observed] of cases) { assert.equal(deriveState(observed).kind, kind, kind); }
    const predecessor = deriveState(cases[1][1]);
    assert.equal(predecessor.kind === 'not_installed' && predecessor.predecessor, true);
    const refused = deriveState(cases[6][1]);
    assert.deepEqual(refused.kind === 'refused' && refused.error, (cases[6][1].status as { error: unknown }).error, 'the exact native error and retry details are kept');
    assert.deepEqual(cases.map(([, observed]) => executable(deriveState(observed))), [false, false, false, false, false, true, true, true]);
});
