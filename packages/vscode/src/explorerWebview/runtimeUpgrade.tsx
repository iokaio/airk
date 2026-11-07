// SPDX-License-Identifier: Apache-2.0
import { useEffect, useState } from 'preact/hooks';
import type { ExplorerViewState } from '../explorerContracts';
import type { CatalogView } from '../indexingCatalog';
import { request } from './api';
import { Notice, noticeKey } from './notices';

type Upgrade = NonNullable<ExplorerViewState['runtimeUpgrade']>;
const STAGE_LABELS: Record<Upgrade['stage'], string> = {
    checking: 'Checking the installed runtime',
    staging: 'Preparing the upgraded runtime',
    draining: 'Settling previous runtime cleanup',
    activating: 'Activating the upgraded runtime',
    reconnecting: 'Reconnecting and checking index capabilities',
    ready: 'Upgrade complete',
    failed: 'Upgrade needs attention',
};

export function upgradePending(upgrade?: Upgrade): boolean {
    return !!upgrade && upgrade.stage !== 'ready';
}

/** Native transition observations supply stages; there is no estimated percentage. */
export function RuntimeUpgradeProgress({ upgrade }: { upgrade: Upgrade }) {
    const [now, setNow] = useState(Date.now());
    const active = upgrade.stage !== 'ready' && upgrade.stage !== 'failed';
    useEffect(() => {
        if (!active) { return; }
        const timer = setInterval(() => setNow(Date.now()), 1000);
        return () => clearInterval(timer);
    }, [active, upgrade.startedAt]);
    const elapsed = Math.max(0, Math.floor(((active ? now : upgrade.updatedAt) - upgrade.startedAt) / 1000));
    return <Notice global noticeKey={noticeKey('runtime-upgrade', upgrade.fromVersion, upgrade.toVersion, active ? 'active' : upgrade.stage, upgrade.stage === 'failed' ? upgrade.message : undefined)} class="callout runtime-upgrade" role={upgrade.stage === 'failed' ? 'alert' : 'status'}>
        <strong>{upgrade.stage === 'ready' ? 'AIRK update complete' : upgrade.stage === 'failed' ? 'AIRK update needs attention' : 'Updating AIRK for your indexes'}</strong>
        <p>AIRK {upgrade.fromVersion} → {upgrade.toVersion}. {upgrade.message}</p>
        <p>{active && <progress aria-label="AIRK upgrade in progress" />} Current stage: {STAGE_LABELS[upgrade.stage]}. Elapsed: {elapsed} seconds.</p>
        <p>{upgrade.stage === 'failed'
            ? 'The upgrade has not completed. Review Management → Service and the installation log for the recorded failure.'
            : upgrade.stage === 'ready'
                ? 'The runtime update finished. Split availability is checked from the connected service.'
                : 'Existing index data and source policies are retained. Splitting becomes available after the upgraded service reports support.'}</p>
        <button class="ghost" onClick={() => { void request('manage.command', { command: 'airk.showExtensionOutput' }).catch(() => undefined); }}>Show installation log</button>
    </Notice>;
}

export function SplitAvailability({ view, upgrade }: { view: CatalogView; upgrade?: Upgrade }) {
    if (upgradePending(upgrade)) {
        return <p role={upgrade?.stage === 'failed' ? 'alert' : 'status'}>{upgrade?.stage === 'failed'
            ? `The AIRK ${upgrade.toVersion} upgrade needs attention. Splitting is unavailable until the upgrade completes. Review Management → Service and the installation log.`
            : `AIRK is upgrading the runtime for this index to ${upgrade?.toVersion}. ${STAGE_LABELS[upgrade!.stage]}. Splitting will become available automatically when the service is ready; details are available in Management → Service and the installation log.`}</p>;
    }
    if (view.capabilities === undefined) {
        return <p role="status">Split availability for this index has not yet been confirmed by the connected service. AIRK checks capabilities automatically after installation and connection.</p>;
    }
    if (typeof view.capabilities.policy_contract === 'number' && view.capabilities.policy_contract < 2) {
        return <p>The connected AIRK service does not support splitting. Eligible local runtimes upgrade automatically with the extension; review the installation status in Management → Service.</p>;
    }
    return <p>Split controls are unavailable on this connection. Review the connection details in Management → Service.</p>;
}
