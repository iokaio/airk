// SPDX-License-Identifier: Apache-2.0
/** Native retention status is observational; rendering it never runs maintenance. */
export interface RetentionPolicy { version: number; mode: 'manual' | 'automatic'; searchable_predecessors: number }
export interface LegacyRevision { revision: string; bytes: number | null; reasons: string[] }
export interface RetentionStatus {
    collection: string;
    policy: { policy: RetentionPolicy; origin: string; digest: string };
    protected_bytes: number | null;
    pending_intents: number;
    pending_bytes: number | null;
    last_maintenance: { state?: string; pending?: boolean; error?: string | null } | null;
    pressure?: { required_bytes: number; available_bytes: number } | null;
    legacy_protected?: LegacyRevision[];
}
export interface RetentionPreview { status: RetentionStatus; candidates: unknown[]; legacy_candidates?: LegacyRevision[]; candidates_truncated: boolean; estimated_reclaimable_bytes: number | null }

export function retentionSummary(status?: RetentionStatus): string {
    if (!status?.policy?.policy) { return 'Retention: not inspected'; }
    const bytes = status.protected_bytes == null ? 'unknown protected bytes' : `${status.protected_bytes} protected bytes`;
    const pending = status.pending_intents ? `; ${status.pending_intents} pending maintenance intents` : status.last_maintenance?.pending ? '; maintenance pending' : '';
    const error = status.last_maintenance?.error ? '; maintenance error (see retention status)' : '';
    const pressure = status.pressure ? `; quota pressure (${status.pressure.required_bytes} needed, ${status.pressure.available_bytes} available)` : '';
    const observation = status.last_maintenance ? `; last pass ${status.last_maintenance.state ?? 'unknown'}` : '; maintenance not observed';
    const legacy = status.legacy_protected?.length ? `; ${status.legacy_protected.length} protected legacy revisions` : '';
    return `Retention: ${status.policy.policy.mode} (${status.policy.origin}); target ${status.policy.policy.searchable_predecessors + 1} completed generation(s); ${bytes}${legacy}${pending}${error}${pressure}${observation}`;
}

export function retentionPolicyForMode(status: RetentionStatus, mode: 'manual' | 'automatic'): RetentionPolicy {
    return { ...status.policy.policy, mode };
}
