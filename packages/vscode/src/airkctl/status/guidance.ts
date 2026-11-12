// SPDX-License-Identifier: Apache-2.0
//! Status and guidance per state (plan §8.1 "Status and guidance", §8.3). The
//! retained commands only guide: Open AIRK Admin runs `airkctl admin open`,
//! Start runs `airkctl service start`, Download and Installation instructions
//! open the download page on user action, Check again re-runs discovery.
//! Nothing here installs, repairs, updates or removes the service.
import type { AirkctlState } from '../index';
import { describe } from '../features/explorerHost';

export type GuidanceAction = 'airk.openAdmin' | 'airk.serviceStart' | 'airk.downloadService' | 'airk.installationInstructions'
    | 'airk.checkAgain' | 'airk.manageWorkspaceTrust' | 'airk.enableNativeExecution' | 'airk.showExtensionOutput';
export const ACTION_LABELS: Readonly<Record<GuidanceAction, string>> = Object.freeze({
    'airk.openAdmin': 'Open AIRK Admin',
    'airk.serviceStart': 'Start',
    'airk.downloadService': 'Download AIRK Service',
    'airk.installationInstructions': 'Installation instructions',
    'airk.checkAgain': 'Check again',
    'airk.manageWorkspaceTrust': 'Manage Workspace Trust',
    'airk.enableNativeExecution': 'Enable Native Execution',
    'airk.showExtensionOutput': 'Show Output',
});

export interface Guidance {
    readonly severity: 'info' | 'warning' | 'error' | 'none';
    readonly message: string;
    readonly status: string;
    readonly actions: readonly GuidanceAction[];
}

export function guidance(state: AirkctlState): Guidance {
    const message = describe(state);
    switch (state.kind) {
        case 'checking': return { severity: 'none', message, status: '$(sync~spin) AIRK', actions: [] };
        // No notification: the installer is on screen; the window reconnects by itself.
        case 'updating': return { severity: 'none', message, status: '$(sync~spin) AIRK Service updating', actions: [] };
        case 'gated': return {
            severity: 'none', message, status: '$(shield) AIRK',
            actions: state.reason === 'workspace_untrusted' ? ['airk.manageWorkspaceTrust'] : ['airk.enableNativeExecution'],
        };
        case 'not_installed': return { severity: 'info', message, status: '$(cloud-download) AIRK Service not installed', actions: ['airk.downloadService', 'airk.installationInstructions', 'airk.checkAgain'] };
        case 'unverified': return { severity: 'error', message, status: '$(error) AIRK Service unverified', actions: ['airk.downloadService', 'airk.checkAgain', 'airk.showExtensionOutput'] };
        case 'incompatible': return { severity: 'warning', message, status: '$(warning) AIRK incompatible', actions: ['airk.downloadService', 'airk.checkAgain'] };
        case 'stopped': return { severity: 'info', message, status: '$(debug-stop) AIRK Service stopped', actions: ['airk.serviceStart', 'airk.checkAgain'] };
        case 'refused': return { severity: 'warning', message, status: '$(warning) AIRK refused', actions: ['airk.checkAgain', 'airk.openAdmin', 'airk.showExtensionOutput'] };
        case 'ready': return {
            severity: 'none', message: state.disabled.length ? `${message} Not available with this service: ${state.disabled.map(item => item.feature).join(', ')}.` : message,
            status: state.signature === 'unsigned_development' ? '$(beaker) AIRK (unsigned development airkctl)' : '$(search) AIRK',
            actions: ['airk.openAdmin'],
        };
    }
}
