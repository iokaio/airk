// SPDX-License-Identifier: Apache-2.0
// Browser-only adapter. Pure/shared presentation never imports this module.
import { call } from '../../admin/src/api';
import { currentSession } from '../../admin/src/session';
import { body, OPS } from '../../admin/src/operations';
import { admitPilotRead, parsePilotRequest, type PilotContext, type PilotObservation, type PilotResult } from './pilotContract';

export function createBrowserPilot(context: () => PilotContext) {
  let sequence = 0;
  return async (input: unknown): Promise<PilotObservation | null> => {
    const request = parsePilotRequest(input);
    if (request === null) return null;
    const ticket = ++sequence, session = currentSession();
    const refusal = admitPilotRead(request, context());
    if (refusal !== null || session === null) return { request, result: { kind: 'refused', code: refusal ?? 'pilot_not_ready' }, applicability: 'current' };
    const method = request.read.kind === 'usage-json'
      ? body.usageDocument(request.read.binding, { report: { days: request.read.days, format: 'json' } })
      : { ...body.helpTopic(request.read.topic), tools_call: {
        ...body.helpTopic(request.read.topic).tools_call,
        tool: 'airk_help',
        arguments: { ...(request.read.topic === '' ? {} : { topic: request.read.topic }), start_line: request.read.startLine, max_bytes: 32768 },
      } };
    const outcome = await call(request.read.kind === 'usage-json' ? OPS.usageDocument : OPS.searchCall, method);
    let result: PilotResult;
    switch (outcome.kind) {
      case 'ok': case 'failed': result = { kind: 'i3', status: outcome.status, envelope: outcome.envelope }; break;
      case 'transport': result = outcome; break;
      case 'no_session': result = { kind: 'refused', code: 'pilot_not_ready' }; break;
    }
    const current = ticket === sequence && session === currentSession() && admitPilotRead(request, context()) === null;
    return { request, result, applicability: current ? 'current' : 'superseded' };
  };
}
