// SPDX-License-Identifier: Apache-2.0
import type { SetupReceiptObservation } from '../../generated/i2';

/** Explicit installation only. Activation and absence discovery never download. */
export type BootstrapChoice = 'automatic' | 'manual' | 'defer';
export interface BootstrapAdmission {
    readonly discovery: 'absent' | 'installed' | 'unknown';
    readonly supported: boolean;
    readonly trusted: boolean;
    readonly nativeAllowed: boolean;
    readonly setupActive: boolean;
}
export interface InstallerOffer {
    readonly version: string;
    readonly target: string;
    readonly name: string;
    readonly url: string;
    readonly sha256: string;
    readonly bytes: number;
    readonly signature: 'authenticode' | 'developer_id' | 'unsigned_digest';
    readonly headlessArguments: readonly ['--silent'];
}
export interface VerifiedInstaller {
    readonly offer: InstallerOffer;
    /** Opaque private staged image; the host re-verifies immediately before use. */
    readonly token: string;
    readonly images: {readonly serviceSha256:string;readonly cliSha256:string};
}
export interface BootstrapHost {
    admission(): Promise<BootstrapAdmission>;
    manual(): Promise<void>;
    discover(signal: AbortSignal): Promise<InstallerOffer>;
    review(offer: InstallerOffer): Promise<boolean>;
    downloadAndVerify(offer: InstallerOffer, signal: AbortSignal): Promise<VerifiedInstaller>;
    /** Must verify the same private file again and run with closed stdin, no shell. */
    execute(installer: VerifiedInstaller, signal: AbortSignal): Promise<{ exitCode: number; receipt?: string }>;
    /** Native-owned receipt observation bound to the selected verified pair. */
    confirmNativeSuccess(installer: VerifiedInstaller, signal: AbortSignal): Promise<boolean>;
    /** Fresh I4 verification plus native health; file placement alone is insufficient. */
    rediscoverAndHealth(signal: AbortSignal): Promise<boolean>;
    result(outcome: BootstrapOutcome): void;
    stage?(stage: string): void;
}
export type BootstrapOutcome =
    | { kind: 'manual' | 'deferred' | 'cancelled' | 'ready' }
    | { kind: 'refused' | 'failed'; detail: string; receipt?: string };

function refusal(value: BootstrapAdmission): string | undefined {
    if (!value.trusted) { return 'Trust this workspace before native installation.'; }
    if (!value.nativeAllowed) { return 'Native execution is disabled in user settings.'; }
    if (!value.supported) { return 'Automatic installation is unavailable on this host. Use manual instructions.'; }
    if (value.setupActive) { return 'Another AIRK setup operation is active.'; }
    if (value.discovery !== 'absent') { return 'AIRK absence is not confirmed. Check or recover the existing installation.'; }
    return undefined;
}

export class InstallerBootstrap {
    private active?: AbortController;
    constructor(private readonly host: BootstrapHost) {}
    cancel(): void { this.active?.abort(); }
    async choose(choice: BootstrapChoice): Promise<BootstrapOutcome> {
        if (choice === 'manual') { await this.host.manual(); return { kind: 'manual' }; }
        if (choice === 'defer') { return { kind: 'deferred' }; }
        if (this.active) { return { kind: 'refused', detail: 'This window already has an installation in progress.' }; }
        const controller = new AbortController();
        this.active = controller;
        const signal = controller.signal;
        let outcome: BootstrapOutcome;
        let receipt: string | undefined;
        try {
            const before = refusal(await this.host.admission());
            if (before) { outcome = { kind: 'refused', detail: before }; }
            else {
                this.host.stage?.('Discovering compatible release');
                const offer = await this.host.discover(signal);
                if (signal.aborted || !await this.host.review(offer)) { outcome = { kind: 'cancelled' }; }
                else {
                    this.host.stage?.('Downloading installer');
                    const installer = await this.host.downloadAndVerify(offer, signal);
                    const changed = refusal(await this.host.admission());
                    if (changed) { outcome = { kind: 'refused', detail: changed }; }
                    else if (signal.aborted) { outcome = { kind: 'cancelled' }; }
                    else {
                        this.host.stage?.('Installing service');
                        const result = await this.host.execute(installer, signal);
                        receipt = result.receipt;
                        if (result.exitCode !== 0) {
                            outcome = { kind: 'failed', detail: `Installer exited ${result.exitCode}. Preserve its recovery evidence.`, receipt };
                        } else if (!await this.host.confirmNativeSuccess(installer,signal)) {
                            outcome={kind:'failed',detail:'Native successful completion receipt is not confirmed for this installer.',receipt};
                        } else if ((this.host.stage?.('Checking service'), await this.host.rediscoverAndHealth(signal))) { outcome = { kind: 'ready' }; }
                        else { outcome = { kind: 'failed', detail: 'Installer finished, but verified installation and service health are not confirmed.', receipt }; }
                    }
                }
            }
        } catch (error) {
            // Cancellation after process launch is not proof of rollback or absence.
            outcome = { kind: 'failed', detail: signal.aborted ? 'Installation cancelled. Check AIRK status before trying again.' : String(error), receipt };
        } finally { this.active = undefined; }
        this.host.result(outcome);
        return outcome;
    }
}

/** Native owns Core receipt decoding/order/identity; the client consumes its generated summary. */
export function nativeCompletionMatches(value:unknown,expected:{operationId:string;installId:string}):value is SetupReceiptObservation{
    if(!value||typeof value!=='object'||Array.isArray(value)){return false;}
    const observation=value as Partial<SetupReceiptObservation>;
    return observation.schema==='airkctl-setup-receipt/1' && observation.closed===true && observation.successful===true &&
        observation.operation_id===expected.operationId && observation.install_id===expected.installId &&
        Object.hasOwn(observation,'receipt');
}
