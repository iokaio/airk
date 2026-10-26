// SPDX-License-Identifier: Apache-2.0
// DOM-free display helpers. Unknown stays "not observed"; counts are never invented.

export function observed(value: number | string | boolean | null | undefined): string {
    if (value === undefined || value === null) { return 'not observed'; }
    if (typeof value === 'boolean') { return value ? 'yes' : 'no'; }
    if (typeof value === 'number') { return Number.isSafeInteger(value) ? value.toLocaleString() : 'unknown'; }
    return value;
}

export function ago(ms: number | undefined, now = Date.now()): string {
    if (ms === undefined) { return 'not observed'; }
    const seconds = Math.max(0, Math.round((now - ms) / 1000));
    if (seconds < 60) { return `${seconds} s ago`; }
    const minutes = Math.round(seconds / 60);
    return minutes < 60 ? `${minutes} min ago` : `${Math.round(minutes / 60)} h ago`;
}

/** Publication tokens are long; show a stable short form with the full value in a title. */
export function shortToken(token: string | undefined): string {
    if (!token) { return 'none'; }
    return token.length > 24 ? `${token.slice(0, 12)}…${token.slice(-8)}` : token;
}

/** "N returned · limit L · more may exist" — the only count form used for capped results. */
export function countLine(returned: number, limit: number, truncated: boolean): string {
    return `${returned} returned · limit ${limit}${truncated || returned >= limit ? ' · more may exist' : ''}`;
}
