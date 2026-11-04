// SPDX-License-Identifier: Apache-2.0
// Where each former Service Console command now lands in the AIRK Explorer
// (design Appendix B). An optional console action still runs first, without
// moving focus, through the shared controller's own validation.

export interface CommandRoute {
    readonly route: string;
    readonly action?: string;
    readonly fields?: Readonly<Record<string, unknown>>;
}
export const MANAGE_WORKSPACE_TRUST_COMMAND = 'workbench.trust.manage';
/** Commands that can start native work; navigation, help, local logs and trust review remain usable. */
export const NATIVE_COMMANDS: readonly string[] = [
    'installCli','setUp','showLastUninstallResult','reviewData','openTools','treeIndex','treeReindex','treePause',
    'index','reindex','indexMode','pause','resume','chooseStorage','status','usage','setupAgent','verifyMcp','manageMcp','manageVscodeMcp',
    'diagnose','currentMaintenance','serviceConnect','serviceStart','serviceRestart','serviceStop','serviceResources',
    'workbench.searchSelection','explorer.search',
];
export const NATIVE_COMMAND_ENABLEMENT = 'isWorkspaceTrusted && config.airk.runtime.nativeExecution && airk.nativeCommandsAvailable';

export const COMMAND_ROUTES: Readonly<Record<string, CommandRoute>> = {
    menu: { route: '#overview' },
    collections: { route: '#manage/projects' },
    removeCollection: { route: '#manage/storage' },
    treeRemove: { route: '#manage/storage' },
    storageLimits: { route: '#manage/storage' },
    extractionPolicy: { route: '#manage/indexing' },
    configureExtractionPolicy: { route: '#manage/indexing' },
    ingestionLimits: { route: '#manage/indexing' },
    configureIngestionLimits: { route: '#manage/indexing' },
    logs: { route: '#manage/logs', action: 'logs' },
    serviceConnect: { route: '#manage/service', action: 'connect' },
    serviceStart: { route: '#manage/service', action: 'start' },
    serviceRestart: { route: '#manage/service', action: 'restart' },
    serviceStop: { route: '#manage/service', action: 'stop' },
    serviceLogs: { route: '#manage/logs', action: 'logs' },
    serviceResources: { route: '#manage/service', action: 'resources' },
    serviceDiagnostics: { route: '#manage/logs', action: 'previewReport', fields: { includePaths: false } },
};
