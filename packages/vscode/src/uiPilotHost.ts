// SPDX-License-Identifier: Apache-2.0
// The pure adapter is bundled separately to preserve existing tsc host paths.
import type { NativePilotHost } from './uiPilotContract';

export function createUiPilotReader(host: NativePilotHost): (input: unknown, signal?: AbortSignal) => Promise<unknown> {
    const runtime = require('./uiPilotRuntime') as {
        createNativePilot(owner: NativePilotHost): (input: unknown, signal?: AbortSignal) => Promise<unknown>;
    };
    return runtime.createNativePilot(host);
}
