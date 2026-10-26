// SPDX-License-Identifier: Apache-2.0
import { ROUTE_KEYS } from './explorerRoutes';

/**
 * implemented: shipped and backed by real AIRK operations.
 * prototype_only: designed (docs/explorer) but not yet built; shown labeled, never simulated.
 * unsupported: not offered by this build. disabled: turned off by settings or capability.
 */
export type Availability = 'implemented' | 'prototype_only' | 'unsupported' | 'disabled';

/** Change an entry only in the commit that ships the route, together with its help topic. */
const IMPLEMENTED: ReadonlySet<string> = new Set<string>(['overview', 'search/query']);

export const FEATURES: Readonly<Record<string, Availability>> = Object.freeze(Object.fromEntries(
    ROUTE_KEYS.map((key): [string, Availability] => [key, IMPLEMENTED.has(key) ? 'implemented' : 'prototype_only'])));

export function availability(key: string): Availability {
    return Object.hasOwn(FEATURES, key) ? FEATURES[key] : 'unsupported';
}
