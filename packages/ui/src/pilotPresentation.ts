// SPDX-License-Identifier: Apache-2.0
// Pure presentation tree. Each host renders it using its own single Preact instance.
import type { PilotObservation } from './pilotContract';
import { projectPilotObservation } from './pilotProjection';

export type PilotNode = { tag: 'section' | 'h2' | 'h3' | 'p' | 'pre' | 'details' | 'summary';
  children: readonly (PilotNode | string)[] };
const node = (tag: PilotNode['tag'], ...children: (PilotNode | string)[]): PilotNode => ({ tag, children });

function complete(observation: PilotObservation): PilotNode {
  const result = observation.result;
  return node('details', node('summary', 'Complete read outcome'),
    node('pre', JSON.stringify(result, null, 2)));
}

/** No native result schema is duplicated or guessed by the pilot shell. */
export function pilotPresentation(observation: PilotObservation, hostLabel: string): PilotNode {
  const read = observation.request.read;
  const title = read.kind === 'usage-json' ? 'Usage report preview' : 'Help article preview';
  const input = read.kind === 'usage-json'
    ? `Index ${read.binding} · requested period ${read.days} days · JSON report only.`
    : `Native topic ${read.topic || '(Help home)'} · requested start line ${read.startLine} · maximum 32768 bytes.`;
  const subset = read.kind === 'usage-json'
    ? 'This preview shares the read outcome. Policy, settings and CSV/Markdown export remain in the existing Usage view. Estimates and recording limits must be read from the native document.'
    : 'This preview shares the native article outcome. Task navigation, formatted articles, tool reference and offline guidance remain in the existing Help view.';
  const applicability = observation.applicability === 'superseded'
    ? 'This result belongs to an earlier context. It is retained for inspection and does not describe the current selection.'
    : 'This observation belongs to the submitted host context; it does not prove source freshness or complete history.';
  const outcome = observation.result.kind === 'i2'
    ? `Native invocation: ${observation.result.outcome.class} (exit ${observation.result.outcome.exit_code}).`
    : observation.result.kind === 'i3'
      ? `HTTP ${observation.result.status}: ${observation.result.envelope.ok ? 'native reply received' : 'request refused or unavailable'}.`
      : observation.result.kind === 'refused'
        ? `Read not dispatched: ${observation.result.code}.`
        : `Read unavailable${observation.result.status === null ? '' : ` (HTTP ${observation.result.status})`}: ${observation.result.message}`;
  const projected = projectPilotObservation(observation);
  const content = projected.kind === 'failure'
    ? node('p', `The read did not produce a displayable report/article: ${projected.code}. Complete native evidence is retained below.`)
    : projected.kind === 'usage'
      ? node('details', node('summary', 'Complete native Usage JSON document'), node('pre', JSON.stringify(projected.payload, null, 2)))
      : node('section', node('h3', typeof projected.article.title === 'string' ? projected.article.title : 'Native Help article'),
        node('p', `Section lines ${String(projected.article.start_line ?? '?')}–${String(projected.article.end_line ?? '?')} of ${String(projected.article.total_lines ?? '?')}. ${projected.article.truncated === true ? 'More sections remain.' : 'Native reply is not truncated.'}`),
        node('pre', projected.content));
  return node('section', node('h2', title), node('p', hostLabel), node('p', input), node('p', applicability),
    node('p', subset), node('p', outcome), content, complete(observation));
}

/** Only fixed tags and text children; no HTML, event handler or URL sink. */
export function renderPilotNode<T>(create: (tag: PilotNode['tag'], children: readonly (T | string)[]) => T, tree: PilotNode): T {
  return create(tree.tag, tree.children.map(child => typeof child === 'string' ? child : renderPilotNode(create, child)));
}
