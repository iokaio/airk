// SPDX-License-Identifier: Apache-2.0
// Draft native adapter: the owning extension supplies its verified session/gates.
import { admitPilotRead, parsePilotRequest, type NativePilotHost,
  type PilotObservation, type PilotRequest, type PilotResult } from './pilotContract';

/** Only these two reads can reach the caller; renderer input never supplies argv. */
function argvFor(request: PilotRequest): readonly string[] {
  switch (request.read.kind) {
    case 'usage-json': return ['usage', 'report', '--days', String(request.read.days), '--format', 'json',
      '--service-project', request.read.binding, '--output', 'json'];
    case 'help-topic': return ['tools', 'call', 'airk_help', '--args', JSON.stringify({
      ...(request.read.topic === '' ? {} : { topic: request.read.topic }),
      start_line: request.read.startLine, max_bytes: 32768,
    }), '--output', 'json'];
  }
}

export function createNativePilot(host: NativePilotHost) {
  let sequence = 0;
  return async (input: unknown, signal?: AbortSignal): Promise<PilotObservation | null> => {
    const request = parsePilotRequest(input);
    if (request === null) return null;
    const ticket = ++sequence;
    const session = host.session();
    const refusal = admitPilotRead(request, host.context());
    if (refusal !== null || session === null) return { request, result: { kind: 'refused', code: refusal ?? 'pilot_not_ready' }, applicability: 'current' };
    let result: PilotResult;
    if (signal?.aborted) {
      result = { kind: 'transport', status: null, message: 'The read was canceled before dispatch.' };
    } else {
      try {
        result = { kind: 'i2', outcome: await host.invoke(argvFor(request), { deadlineMs: 60000, ...(signal ? { signal } : {}) }) };
      } catch {
        // A nonzero native invocation must be returned whole by invoke, not
        // thrown. Only failures without a native outcome use this local text.
        result = { kind: 'transport', status: null, message: 'The native read did not return a complete outcome.' };
      }
    }
    const current = ticket === sequence && session === host.session() && admitPilotRead(request, host.context()) === null;
    return { request, result, applicability: current ? 'current' : 'superseded' };
  };
}
