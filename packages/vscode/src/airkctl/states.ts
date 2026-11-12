// SPDX-License-Identifier: Apache-2.0
//! The extension's view of the installed service (plan §8.3) as one typed
//! state, derived by a pure function from the discovery observations so every
//! row of the plan's table is testable without a process.
import type { ErrorLine, ServiceStatus } from '../generated/i2';
import type { Compatibility } from './version';
import type { Verification } from './verify';

export type AirkctlState =
    /** Workspace untrusted or native execution disabled: help and local UI only. */
    | { readonly kind: 'gated'; readonly reason: 'workspace_untrusted' | 'native_execution_disabled' }
    | { readonly kind: 'checking' }
    /** No install pointer. `predecessor`: only a pre-split installation exists. */
    | { readonly kind: 'not_installed'; readonly predecessor: boolean }
    /** Refuse to execute; explain and point to the installer. */
    | { readonly kind: 'unverified'; readonly code: string; readonly detail: string }
    /** Contract or pair mismatch: show both versions and the reason. */
    | {
        readonly kind: 'incompatible'; readonly reason: string; readonly missing?: readonly string[];
        readonly extensionVersion: string; readonly extensionContracts: readonly number[];
        readonly airkctlVersion: string; readonly airkctlContracts: readonly number[];
    }
    /** Installed, service not running; `explicitlyStopped` shows a saved Stop. */
    | { readonly kind: 'stopped'; readonly version: string; readonly explicitlyStopped: boolean }
    /** Reachable but refused: the exact native error and retry details. */
    | { readonly kind: 'refused'; readonly version: string; readonly error: ErrorLine | { readonly code: string; readonly message: string; readonly retryable: boolean } }
    /** Verified, compatible and running. */
    | {
        readonly kind: 'ready'; readonly version: string; readonly capabilities: readonly string[];
        readonly disabled: readonly { readonly capability: string; readonly feature: string }[];
        readonly signature: 'signed' | 'unsigned_development' | 'digest_only';
        readonly health?: unknown;
    };

export interface Observations {
    readonly gate?: 'workspace_untrusted' | 'native_execution_disabled';
    readonly verification?: Verification;
    readonly version?: { readonly document?: { readonly version: string; readonly capabilities: readonly string[] }; readonly compatibility?: Compatibility; readonly invalid?: string };
    readonly status?: { readonly value?: ServiceStatus; readonly error?: ErrorLine | { readonly code: string; readonly message: string; readonly retryable: boolean } };
    readonly extensionVersion: string;
}

/** Derive the state; observations are consulted in the order they are taken,
 * and a later step is never trusted when an earlier one failed. */
export function deriveState(observed: Observations): AirkctlState {
    if (observed.gate) { return { kind: 'gated', reason: observed.gate }; }
    const verification = observed.verification;
    if (!verification) { return { kind: 'checking' }; }
    if (verification.state === 'not_found') { return { kind: 'not_installed', predecessor: verification.predecessor }; }
    if (verification.state === 'unverified') { return { kind: 'unverified', code: verification.code, detail: verification.detail }; }
    const version = observed.version;
    if (!version) { return { kind: 'checking' }; }
    if (version.invalid || !version.document || !version.compatibility) {
        return { kind: 'unverified', code: version.invalid ?? 'version_document_missing', detail: 'airkctl did not report a valid version document.' };
    }
    const compatibility = version.compatibility;
    if (!compatibility.compatible) {
        if (compatibility.reason === 'not_installed') { return { kind: 'not_installed', predecessor: false }; }
        if (compatibility.reason === 'installation_unverified') { return { kind: 'unverified', code: 'installation_unverified', detail: 'airkctl could not verify its installation.' }; }
        return {
            kind: 'incompatible', reason: compatibility.reason, ...(compatibility.missing ? { missing: compatibility.missing } : {}),
            extensionVersion: observed.extensionVersion, extensionContracts: compatibility.extensionContracts,
            airkctlVersion: compatibility.airkctl.version, airkctlContracts: compatibility.airkctl.contracts,
        };
    }
    const status = observed.status;
    if (!status) { return { kind: 'checking' }; }
    if (status.error) { return { kind: 'refused', version: version.document.version, error: status.error }; }
    const service = status.value?.service;
    if (!service) { return { kind: 'checking' }; }
    switch (service.state) {
        case 'running':
            return {
                kind: 'ready', version: version.document.version, capabilities: version.document.capabilities, disabled: compatibility.disabled,
                signature: verification.signature, health: service.health,
            };
        case 'incompatible':
            return { kind: 'incompatible', reason: service.code, extensionVersion: observed.extensionVersion, extensionContracts: compatibility.compatible ? [compatibility.contract] : [],
                airkctlVersion: version.document.version, airkctlContracts: [] };
        case 'not_running':
        case 'unknown':
        default:
            return { kind: 'stopped', version: version.document.version, explicitlyStopped: false };
    }
}

/** Whether `airkctl` may be executed in this state (never before verification). */
export function executable(state: AirkctlState): boolean {
    return state.kind === 'ready' || state.kind === 'stopped' || state.kind === 'refused';
}
