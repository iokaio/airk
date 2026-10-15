// SPDX-License-Identifier: Apache-2.0
/** Operator controls for committed local evidence, independent of client configuration. */
export interface HistoryPolicy { version: number; enabled: boolean; recent: number; hourly_hours: number; max_packages: number; max_bytes: number; quota_basis_points: number }
export interface HistoryPolicyReceipt { policy: HistoryPolicy; sequence: number; digest: string; explicit: boolean }
export interface HistoryAvailability { revisions: string[]; representation_verification: string; next_cursor?: string | null; complete: boolean }
export interface Checkpoint { label: string; publication: { token: string }; admitted_dependency_bytes: number }
export interface PurgeRequest { revisions: string[]; checkpoints: { label: string; expected_revision: string }[]; clear_lineage: boolean }
export interface PurgePlan { digest: string; targets: { revision: string; protected: string[] }[]; clear_lineage: boolean }
export interface HistoryPlan { complete: boolean; projected_history_bytes: number | null; byte_limit: number; over_target_protected: boolean; retained: { protected: string[] }[] }
export interface Choice { label: string; description?: string; value: string }
export interface HistoryControls {
    read<T>(args: string[]): Promise<T>;
    write<T>(args: string[]): Promise<T>;
    show(title: string, value: unknown): Promise<void>;
    confirm(message: string, action: string): Promise<boolean>;
    pick(choices: Choice[], title: string): Promise<string | undefined>;
    input(prompt: string, value?: string): Promise<string | undefined>;
    collection: string;
}
export type HistoryAction = 'show' | 'policy' | 'run' | 'available' | 'checkpoints' | 'create' | 'remove' | 'purge';

export function historySummary(plan: HistoryPlan): string {
    const bytes = plan.projected_history_bytes === null ? 'projected history bytes unknown' : `${plan.projected_history_bytes} projected history bytes`;
    return `${bytes}; limit ${plan.byte_limit} bytes; ${plan.complete ? 'complete inspection' : 'partial inspection'}; ${plan.retained.filter(entry => entry.protected.length).length} protected revisions${plan.over_target_protected ? '; protected evidence exceeds target' : ''}`;
}

export function parsePurgeRequest(json: string): PurgeRequest {
    if (Buffer.byteLength(json, 'utf8') > 65536) { throw new Error('Purge request exceeds 64 KiB.'); }
    const value: unknown = JSON.parse(json);
    if (!value || typeof value !== 'object' || Array.isArray(value)) { throw new Error('Purge request must be an object.'); }
    const request = value as Record<string, unknown>;
    if (Object.keys(request).some(key => !['revisions', 'checkpoints', 'clear_lineage'].includes(key))
        || !Array.isArray(request.revisions) || request.revisions.length > 128 || request.revisions.some(token => typeof token !== 'string')
        || (request.clear_lineage !== undefined && typeof request.clear_lineage !== 'boolean')) { throw new Error('Invalid purge request fields or bounds.'); }
    const checkpoints = request.checkpoints ?? [];
    if (!Array.isArray(checkpoints) || checkpoints.length > 4 || checkpoints.some(pin => !pin || typeof pin !== 'object'
        || Object.keys(pin).some(key => !['label', 'expected_revision'].includes(key)) || typeof pin.label !== 'string' || typeof pin.expected_revision !== 'string')) { throw new Error('Checkpoint removals need explicit labels and expected revisions.'); }
    return { revisions: request.revisions as string[], checkpoints, clear_lineage: request.clear_lineage === true };
}

export async function historyControl(ui: HistoryControls, action: HistoryAction): Promise<void> {
    if (action === 'available') {
        let cursor: string | undefined;
        do {
            const page: HistoryAvailability = await ui.read(['history', 'available', '--limit', '64', '--max-bytes', '65536', ...(cursor ? ['--cursor', cursor] : [])]);
            await ui.show('AIRK retained publication identities', { ...page, explanation: 'A listed receipt identifies a committed publication. Exact revision requests verify retained evidence; this listing is not historical search.' });
            if (!page.next_cursor || await ui.pick([{ label: 'Next bounded page', value: 'next' }, { label: 'Done', value: 'done' }], 'Retained publication identities') !== 'next') { return; }
            cursor = page.next_cursor;
        } while (cursor);
        return;
    }
    if (action === 'show' || action === 'policy' || action === 'run') {
        const policy = await ui.read<HistoryPolicyReceipt>(['history', 'show']);
        const plan = await ui.read<HistoryPlan>(['history', 'preview']);
        const retention = await ui.read<unknown>(['retention', 'show']);
        await ui.show('AIRK history status and protection', { policy, plan, retention, summary: historySummary(plan), explanation: 'Source files are untouched. Active, predecessor, reader-leased and checkpoint evidence may remain protected. Unknown byte totals are not zero.' });
        if (action === 'show') { return; }
        if (action === 'policy') {
            const selected = await ui.pick([{ label: 'Enable bounded history', value: 'on' }, { label: 'Disable automatic history selection', value: 'off' }], `History policy for ${ui.collection}`);
            if (!selected) { return; }
            const proposed = { ...policy.policy, enabled: selected === 'on' };
            const json = JSON.stringify(proposed);
            await ui.show('AIRK proposed history policy', { proposed, currentPlan: plan, explanation: 'The plan describes the current policy. Saving this proposal does not run cleanup or erase existing history/checkpoints.' });
            if (await ui.confirm(`Save this history policy for ${ui.collection}? Existing history is not immediately removed.`, 'Save history policy')) {
                await ui.show('AIRK saved history policy', await ui.write(['history', 'set', '--json', json]));
            }
            return;
        }
        if (await ui.confirm(`Run one bounded history maintenance pass for ${ui.collection}? ${historySummary(plan)}. Protected or unfinished work may remain pending.`, 'Run history maintenance')) {
            await ui.show('AIRK history maintenance result', await ui.write(['history', 'run', '--apply']));
        }
        return;
    }
    const checkpoints = await ui.read<Checkpoint[]>(['checkpoint', 'list']);
    const retention = await ui.read<unknown>(['retention', 'show']);
    await ui.show('AIRK checkpoints', { checkpoints, retention, explanation: 'Checkpoints pin already committed evidence, not live source files. Removing a checkpoint releases a pin; protected copies may remain. Unknown protected byte totals are not zero.' });
    if (action === 'checkpoints') { return; }
    if (action === 'create') {
        const page = await ui.read<HistoryAvailability>(['history', 'available', '--limit', '64', '--max-bytes', '65536']);
        await ui.show('AIRK checkpoint source identities', page);
        const revision = await ui.pick(page.revisions.map(token => ({ label: token, value: token })), 'Choose an already committed revision (first bounded page)');
        if (!revision) { return; }
        const verified = await ui.read<HistoryAvailability>(['history', 'available', '--revision', revision, '--limit', '64', '--max-bytes', '65536']);
        if (!verified.complete || !verified.revisions.includes(revision)) { throw new Error('Selected committed evidence is unavailable; no checkpoint was created.'); }
        const label = await ui.input('Checkpoint label (1–128 UTF-8 bytes, no controls)');
        if (label === undefined) { return; }
        if (!label.length || Buffer.byteLength(label, 'utf8') > 128 || /[\u0000-\u001f\u007f-\u009f]/u.test(label)) { throw new Error('Invalid checkpoint label.'); }
        await ui.show('AIRK proposed checkpoint', { label, revision, verified, explanation: 'The helper revalidates committed dependencies and quota before pinning. No source snapshot is taken from live files.' });
        if (await ui.confirm(`Pin committed revision ${revision} as ${JSON.stringify(label)} in ${ui.collection}?`, 'Create checkpoint')) {
            await ui.show('AIRK checkpoint created', await ui.write(['checkpoint', 'create', '--revision', revision, '--', label]));
        }
        return;
    }
    if (action === 'remove') {
        const label = await ui.pick(checkpoints.map(pin => ({ label: pin.label, description: `${pin.admitted_dependency_bytes} admitted dependency bytes · ${pin.publication.token}`, value: pin.label })), 'Choose checkpoint to remove');
        const selected = checkpoints.find(pin => pin.label === label); if (!selected) { return; }
        await ui.show('AIRK checkpoint removal preview', { checkpoint: selected, explanation: 'Only this exact checkpoint binding will be removed. This is not evidence erasure.' });
        if (await ui.confirm(`Remove checkpoint ${JSON.stringify(selected.label)} from ${ui.collection}? Source files remain unchanged.`, 'Remove checkpoint')) {
            await ui.show('AIRK checkpoint removal result', await ui.write(['checkpoint', 'remove', '--revision', selected.publication.token, '--', selected.label]));
        }
        return;
    }
    const input = await ui.input('Logical purge request JSON: revisions, explicit checkpoint removals, clear_lineage', JSON.stringify({ revisions: [], checkpoints: [], clear_lineage: false }));
    if (input === undefined) { return; }
    const request = parsePurgeRequest(input);
    const json = JSON.stringify(request);
    const plan = await ui.read<PurgePlan>(['history', 'purge', '--json', json]);
    if (!/^[a-f0-9]{64}$/.test(plan.digest)) { throw new Error('Purge preview omitted a valid plan digest; no changes applied.'); }
    await ui.show('AIRK logical purge preview', { request, plan, explanation: 'Protected evidence may remain. Logical purge is not secure erasure: backups, relocated roots and storage-device remnants are outside this operation. Source files are untouched.' });
    if (await ui.confirm(`Apply this exact logical purge plan to ${ui.collection}? ${plan.targets.filter(target => target.protected.length).length} targets have protection. Clear lineage: ${request.clear_lineage}. Backups and storage-device remnants are not erased.`, 'Apply logical purge')) {
        await ui.show('AIRK logical purge result', await ui.write(['history', 'purge', '--json', json, '--apply', '--expected-plan', plan.digest]));
    }
}
