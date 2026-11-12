// SPDX-License-Identifier: Apache-2.0
//! The typed `airkctl` facade: the extension's only runtime channel to the
//! AIRK Service (I2) and its only process starter (plan §4.4, §8, §10.3).
//! Features receive this facade; none of them spawns, opens a pipe, socket or
//! HTTPS connection, or reads service credentials or state.
import * as path from 'node:path';
import type { ErrorLine, ServiceStatus } from '../generated/i2';
import { airkctlEnvironment, exitClass, spawnBounded, splitErrorLine, type Spawner } from './runner';
import { AirkctlCommandError, AirkctlSession, type InvokeResult, type SessionOptions } from './session';
import { deriveState, executable, type AirkctlState, type Observations } from './states';
import { compatibility, EXTENSION_REQUIREMENTS, parseVersionDocument, VersionDocumentInvalid } from './version';
import { AirkctlVerifier, type Verification, type VerifyHost } from './verify';
import { decodeReplacementNotice, MAX_REPLACEMENT_NOTICE_BYTES, replacementNoticePath } from './install';

export { AirkctlCommandError, AirkctlSession } from './session';
export type { AirkctlState } from './states';

/** Download page and release list (U59); the "not installed" state, the
 * Download command and the Marketplace README use these exact links. The
 * site's downloads section is the `#downloads` anchor of its home page. */
export const DOWNLOAD_URL = 'https://airk.ioka.io/#downloads';
export const RELEASES_URL = 'https://github.com/iokaio/airk/releases';

/** Commands the session refuses (contracts §3.8): one-shots only. */
const ONE_SHOT_ONLY = [['agents', 'credential'], ['admin', 'url'], ['admin', 'open'], ['service', 'uninstall'], ['tools', 'shell'], ['service', 'rpc']];
export function sessionRefuses(argv: readonly string[]): boolean {
    return ONE_SHOT_ONLY.some(([first, second]) => argv[0] === first && argv.slice(1).includes(second));
}

export interface AirkctlOptions {
    readonly clientVersion: string;
    readonly verifyHost: VerifyHost;
    readonly spawner?: Spawner;
    /** Workspace trust and the native-execution setting (the existing gate). */
    readonly gate: () => 'workspace_untrusted' | 'native_execution_disabled' | undefined;
    readonly session?: Omit<SessionOptions, 'clientVersion'>;
    readonly log?: (line: string) => void;
    /** Replacement-notice poll interval; 0 turns polling off (tests drive `poll`). Default 1 s. */
    readonly noticePollMs?: number;
    /** Unix milliseconds; tests inject a clock. */
    readonly now?: () => number;
}

export class Airkctl {
    private readonly verifier: AirkctlVerifier;
    private readonly spawner: Spawner;
    private verified?: Verification & { state: 'verified' };
    private current: AirkctlState = { kind: 'checking' };
    private readonly listeners = new Set<(state: AirkctlState) => void>();
    private checking?: Promise<AirkctlState>;
    /** The program folder of the replacement notice in force, if any. */
    private replacing?: string;
    private noticeTimer?: ReturnType<typeof setInterval>;
    private polling?: Promise<void>;
    readonly session: AirkctlSession;

    constructor(private readonly options: AirkctlOptions) {
        this.verifier = new AirkctlVerifier(options.verifyHost);
        this.spawner = options.spawner ?? spawnBounded;
        this.session = new AirkctlSession({ ...options.session, clientVersion: options.clientVersion, contracts: EXTENSION_REQUIREMENTS.supported }, () => {
            const gate = this.options.gate();
            if (gate) { throw new AirkctlUnavailable(gate); }
            if (this.replacing !== undefined) { throw new AirkctlUnavailable('updating'); }
            if (!executable(this.current)) { throw new AirkctlUnavailable(this.current.kind); }
        });
        const every = options.noticePollMs ?? 1000;
        if (every > 0) {
            this.noticeTimer = setInterval(() => { void this.poll(); }, every);
            this.noticeTimer.unref?.();
        }
    }

    /** The replacement notice in force (I4 `replacing.json` beside the pointer), or undefined. */
    private async notice(): Promise<string | undefined> {
        const host = this.options.verifyHost;
        const file = replacementNoticePath(host.home, host.platform);
        try {
            const facts = await host.lstat(file);
            if (!facts || facts.kind !== 'file') { return undefined; }
            return decodeReplacementNotice(await host.read(file, MAX_REPLACEMENT_NOTICE_BYTES), (this.options.now ?? Date.now)()).program;
        } catch { return undefined; }
    }
    /**
     * One observation of the replacement notice. While setup replaces or
     * removes the installed files, release the installed `airkctl` (end the
     * session; no command runs) so the installer never waits on this window;
     * once the notice is gone or expired, verify and reconnect.
     */
    poll(): Promise<void> {
        this.polling ??= (async () => {
            const program = await this.notice();
            if (program !== undefined && this.replacing === undefined) {
                this.replacing = program;
                this.options.log?.(`AIRK Service is being updated (${program}); releasing airkctl.`);
                this.forget();
                this.publish({ extensionVersion: this.options.clientVersion, replacing: program });
            } else if (program === undefined && this.replacing !== undefined) {
                this.replacing = undefined;
                this.options.log?.('AIRK Service update finished; reconnecting.');
                await this.check();
            }
        })().finally(() => { this.polling = undefined; });
        return this.polling;
    }
    get state(): AirkctlState { return this.current; }
    /** The verified installation, after a successful check. */
    get installation(): (Verification & { state: 'verified' }) | undefined { return this.verified; }
    onDidChangeState(listener: (state: AirkctlState) => void): { dispose(): void } {
        this.listeners.add(listener); return { dispose: () => { this.listeners.delete(listener); } };
    }

    /** Re-run discovery: gate, verification, version handshake and service
     * status. On request ("Check again") and on bounded recovery. */
    check(): Promise<AirkctlState> {
        this.checking ??= this.observe()
            .catch(error => {
                // The notice appeared during this check: settle on `updating`.
                if (error instanceof AirkctlUnavailable && error.reason === 'updating') { return this.publish({ extensionVersion: this.options.clientVersion }); }
                throw error;
            })
            .finally(() => { this.checking = undefined; });
        return this.checking;
    }
    private async observe(): Promise<AirkctlState> {
        const observed: { -readonly [K in keyof Observations]: Observations[K] } = { extensionVersion: this.options.clientVersion };
        const gate = this.options.gate();
        if (gate) { return this.publish({ ...observed, gate }); }
        const replacing = await this.notice();
        if (replacing !== undefined) {
            this.replacing = replacing; this.forget();
            return this.publish({ ...observed, replacing });
        }
        this.replacing = undefined;
        const verification = await this.verifier.verify();
        observed.verification = verification;
        if (verification.state !== 'verified') { this.forget(); return this.publish(observed); }
        if (this.verified && this.verified.sha256 !== verification.sha256) { this.forget(); }
        this.verified = verification;
        const version = await this.direct(['version', '--output', 'json'], { timeoutMs: 15_000 });
        if (version.class !== 'success') {
            observed.version = { invalid: version.error?.code ?? `airkctl_version_${version.class}` };
            return this.publish(observed);
        }
        try {
            const document = parseVersionDocument(version.stdout);
            if (document.executable_sha256 && document.executable_sha256 !== verification.sha256) {
                observed.version = { invalid: 'version_document_digest_mismatch' };
                return this.publish(observed);
            }
            if (document.installation.install_id && document.installation.install_id !== verification.pointer.install_id) {
                observed.version = { invalid: 'version_document_install_mismatch' };
                return this.publish(observed);
            }
            observed.version = { document, compatibility: compatibility(document) };
        } catch (error) {
            observed.version = { invalid: error instanceof VersionDocumentInvalid ? error.code : 'version_document_malformed' };
            return this.publish(observed);
        }
        if (!observed.version.compatibility?.compatible) { return this.publish(observed); }
        this.session.bind({ path: verification.airkctl, sha256: verification.sha256, version: verification.record.version });
        const status = await this.direct(['service', 'status', '--output', 'json'], { timeoutMs: 15_000 });
        if (status.class === 'success') {
            try { observed.status = { value: JSON.parse(status.stdout) as ServiceStatus }; }
            catch { observed.status = { error: { code: 'service_status_malformed', message: 'airkctl service status returned malformed JSON.', retryable: false } }; }
        } else if (status.class === 'service_missing') {
            observed.status = { value: { schema: 'airkctl-service-status/1', installation: { state: 'complete' }, service: { state: 'not_running' } } };
        } else {
            observed.status = { error: status.error ?? { code: status.class, message: status.stderr.slice(-4096) || `airkctl exited with ${status.class}`, retryable: false } };
        }
        return this.publish(observed);
    }
    private forget(): void { this.verified = undefined; this.session.unbind(); }
    private publish(observed: Observations): AirkctlState {
        // A check that started before the notice appeared cannot clear it.
        const next = deriveState(this.replacing !== undefined && observed.replacing === undefined ? { ...observed, replacing: this.replacing } : observed);
        this.current = next;
        for (const listener of [...this.listeners]) { try { listener(next); } catch { /* Listener failures stay local. */ } }
        return next;
    }

    /**
     * One `airkctl` one-shot on the verified image: the token-returning and
     * pipe-only commands the session refuses (`agents credential`, `admin
     * open|url`), the version handshake and status. The image is re-verified
     * (cached by file identity) before every execution.
     */
    async oneShot(argv: readonly string[], options: { input?: string; timeoutMs?: number } = {}): Promise<InvokeResult> {
        const gate = this.options.gate();
        if (gate) { throw new AirkctlUnavailable(gate); }
        if (this.replacing !== undefined) { throw new AirkctlUnavailable('updating'); }
        if (!executable(this.current)) { throw new AirkctlUnavailable(this.current.kind); }
        return this.direct(argv, options);
    }
    private async direct(argv: readonly string[], options: { input?: string; timeoutMs?: number }): Promise<InvokeResult> {
        if (this.replacing !== undefined) { throw new AirkctlUnavailable('updating'); }
        const verification = await this.verifier.verify();
        if (this.replacing !== undefined) { throw new AirkctlUnavailable('updating'); }
        if (verification.state !== 'verified' || (this.verified && verification.sha256 !== this.verified.sha256)) {
            void this.check();
            throw new AirkctlUnavailable('installation_changed');
        }
        const result = await this.spawner(verification.airkctl, argv, {
            env: airkctlEnvironment(), cwd: path.dirname(verification.airkctl), ...(options.input === undefined ? {} : { input: options.input }),
            timeoutMs: options.timeoutMs ?? 60_000, maxStdout: 16 * 1024 * 1024, maxStderr: 64 * 1024,
        });
        const klass = result.timedOut || result.overflow ? 'internal' : exitClass(result.exitCode);
        const { stderr, error } = splitErrorLine(result.stderr.toString('utf8'));
        this.options.log?.(`airkctl ${argv.slice(0, 2).join(' ')}: ${klass}`);
        return { exitCode: result.exitCode ?? 8, class: klass, stdout: result.stdout.toString('utf8'), stderr, ...(error ? { error } : {}) };
    }
    /** A one-shot with `--output json` whose non-success class throws the complete result. */
    async oneShotJson(argv: readonly string[], options: { input?: string; timeoutMs?: number } = {}): Promise<unknown> {
        const full = [...argv, '--output', 'json'];
        const result = await this.oneShot(full, options);
        if (result.class !== 'success') { throw new AirkctlCommandError(full, result); }
        try { return JSON.parse(result.stdout); } catch { throw new AirkctlCommandError(full, { ...result, class: 'internal' }); }
    }

    /** `airkctl service start`: the explicit Start that clears a saved Stop. */
    async start(): Promise<InvokeResult> {
        const result = await this.oneShot(['service', 'start']);
        await this.check();
        return result;
    }
    /** `airkctl admin open`: the sign-in link opens in the browser; the code never reaches the extension. */
    openAdmin(): Promise<InvokeResult> { return this.oneShot(['admin', 'open']); }

    dispose(): void {
        if (this.noticeTimer !== undefined) { clearInterval(this.noticeTimer); this.noticeTimer = undefined; }
        this.session.dispose(); this.listeners.clear();
    }
}

/** `airkctl` is not executed in this state; `reason` is the state kind or gate. */
export class AirkctlUnavailable extends Error {
    constructor(readonly reason: string) { super(`AIRK Service is unavailable: ${reason}`); this.name = 'AirkctlUnavailable'; }
}
/** The complete native error of a refused command, for display and copy. */
export function errorOf(error: unknown): ErrorLine | { code: string; message: string; retryable: boolean } {
    if (error instanceof AirkctlCommandError) {
        return error.result.error ?? { code: error.result.class, message: error.result.stderr.slice(-4096) || error.message, retryable: false };
    }
    if (error instanceof AirkctlUnavailable) { return { code: error.reason, message: error.message, retryable: false }; }
    const code = (error as { code?: unknown })?.code;
    return { code: typeof code === 'string' ? code : 'internal', message: error instanceof Error ? error.message : String(error), retryable: (error as { retryable?: unknown })?.retryable === true };
}
