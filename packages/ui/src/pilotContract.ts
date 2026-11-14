// SPDX-License-Identifier: Apache-2.0
// Local shared presentation transport types; native envelopes remain generated.
import type { Envelope } from '../../contracts-gen/i3';
import type { PilotRequest, PilotRefusal } from '../../vscode/src/uiPilotContract';
export { PILOT_VERSION, parsePilotRequest, admitPilotRead } from '../../vscode/src/uiPilotContract';
export type { PilotRead, PilotRequest, PilotContext, PilotRefusal, NativePilotHost,
  PilotI2Invocation as I2Invocation } from '../../vscode/src/uiPilotContract';
import type { PilotI2Invocation as I2Invocation } from '../../vscode/src/uiPilotContract';

/** Keep all native fields/channels. No hand-written native result DTO. */
export type PilotResult =
  | { kind: 'i2'; outcome: I2Invocation }
  | { kind: 'i3'; status: number; envelope: Envelope }
  | { kind: 'transport'; status: number | null; message: string }
  | { kind: 'refused'; code: PilotRefusal };

export type PilotObservation = {
  readonly request: PilotRequest;
  readonly result: PilotResult;
  /** Superseded evidence is retained, but never presented as the current scope. */
  readonly applicability: 'current' | 'superseded';
};
