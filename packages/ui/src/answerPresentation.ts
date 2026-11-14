// SPDX-License-Identifier: Apache-2.0
import type { ContextResponse } from '../../contracts-gen/arbiter';
export type AnswerNode = { tag: 'section' | 'h3' | 'h4' | 'h5' | 'p' | 'div' | 'button' | 'span' | 'ul' | 'li' | 'details' | 'summary' | 'article' | 'pre';
  children: readonly (AnswerNode | string)[]; id?: string; citationTarget?: string };
const node = (tag: AnswerNode['tag'], ...children: (AnswerNode | string)[]): AnswerNode => ({ tag, children });
/** Display-only citations name exact native records, never guessed paths or authority. */
export function answerPresentation(answer: ContextResponse, identity: string): AnswerNode {
  const ids = new Set(answer.records.map(record => record.id));
  const target = (id: string) => `answer-${identity}-${id}`;
  return node('section', node('h3', 'Evidence-grounded context'),
    node('p', `Stop: ${answer.stop}. Candidates: ${answer.candidate_count}; omitted: ${answer.omitted_count}. Evidence remains bounded by the original request.`),
    ...answer.paragraphs.map(paragraph => node('div', node('p', paragraph.text),
      node('p', paragraph.interpretation ? 'Interpretation of the cited evidence.' : 'Evidence-grounded statement.'),
      node('div', ...paragraph.evidence.map(id => ids.has(id)
        ? { ...node('button', `Read evidence ${id.slice(0, 12)}`), citationTarget: target(id) }
        : node('span', `Unresolved native evidence citation: ${id}`))))),
    node('h4', 'Gaps and limits'), answer.gaps.length ? node('ul', ...answer.gaps.map(gap => node('li', gap)))
      : node('p', 'No additional gap was reported. This does not establish exhaustive coverage.'),
    node('h4', 'Original evidence records'),
    ...answer.records.map(record => node('details', node('summary', `${record.operation} · ${record.role} · ${record.id.slice(0, 12)}`),
      { ...node('article', node('h5', record.id), node('pre', JSON.stringify(record, null, 2))), id: target(record.id) })),
    node('details', node('summary', 'Native work, application and advice observations'),
      node('pre', JSON.stringify({ work: answer.work, applications: answer.applications, advice: answer.advice }, null, 2))));
}
