// SPDX-License-Identifier: Apache-2.0
import type { AssessmentApplicability, AssessmentEvent, ContextActionAdvice, ContextResponseV2, EvidenceRef, Gap, Work } from '../../contracts-gen/arbiter';
export type AnswerNode = { tag: 'section' | 'h3' | 'h4' | 'h5' | 'p' | 'div' | 'button' | 'span' | 'ul' | 'li' | 'details' | 'summary' | 'article' | 'pre';
  children: readonly (AnswerNode | string)[]; id?: string; citationTarget?: string };
const node = (tag: AnswerNode['tag'], ...children: (AnswerNode | string)[]): AnswerNode => ({ tag, children });
const words = (value: string): string => value.replaceAll('_', ' ');
const gapLabel = (gap: Gap): string => gap === 'unsupported' ? 'Unclassified need (native unsupported)' : words(gap);
const assessmentApplicability: Record<AssessmentApplicability, string> = {
  current: 'Current local assessment',
  historical_after_read: 'Historical after a later read; no current coverage claim',
  historical_after_configuration_change: 'Historical after configuration changed; no current coverage claim',
  basis_omitted: 'Basis omitted from this packet; no current coverage claim',
};
/** A current local-partition view. Native whole-card hypotheses never become source facts or global coverage. */
export function answerPresentation(answer: ContextResponseV2, identity: string): AnswerNode {
  const ids = new Set(answer.records.map(record => record.id));
  const target = (id: string) => `answer-${identity}-${id}`;
  const citation = (id: string): AnswerNode => ids.has(id)
    ? { ...node('button', `Read evidence ${id.slice(0, 12)}`), citationTarget: target(id) }
    : node('span', `Unresolved native evidence citation: ${id}`);
  const unit = (reference: EvidenceRef): AnswerNode => node('div', citation(reference.record),
    node('span', `Native ${reference.kind} ${reference.result_pointer}; UTF-8 bytes ${reference.utf8_start}–${reference.utf8_end}.`));
  const measuredWork = (work: Work): AnswerNode => node('div',
    node('p', `Recorded input-token lower bound: ${work.input_tokens}. Nested work belongs to this assessment attempt and is already accounted in the native response totals.`),
    node('p', `Measured phase times (microseconds): queue ${work.queue_us == null ? 'unobserved' : work.queue_us}; compile ${work.compile_us == null ? 'unobserved' : work.compile_us}; inference ${work.inference_us == null ? 'unobserved' : work.inference_us}; policy ${work.policy_us == null ? 'unobserved' : work.policy_us}.`));
  const adviceReceipt = (advice: ContextActionAdvice): AnswerNode => node('details',
    node('summary', `Native advice receipt · reserved ${advice.reserved_input_tokens} tokens · observed ${advice.observed_input_tokens === null ? 'unknown' : advice.observed_input_tokens}`),
    node('p', `Reserved input tokens: ${advice.reserved_input_tokens}. Observed input tokens: ${advice.observed_input_tokens === null ? 'unknown; the known lower bound does not establish zero actual use' : advice.observed_input_tokens}.`),
    measuredWork(advice.work),
    node('p', `Profile policy: ${advice.policy}. Task policy: ${advice.task_policy}. Compiler: ${advice.compiler}. Calibration: ${advice.calibration}. Input: ${advice.input_digest}.`),
    node('pre', JSON.stringify(advice, null, 2)));
  const event = (observed: AssessmentEvent): AnswerNode => {
    const result = observed.result;
    const advice = result.advice;
    const unknownWork = result.status === 'unavailable' && result.work_unknown || advice?.observed_input_tokens === null;
    const applicability = assessmentApplicability[observed.applicability];
    const summary = result.status === 'selected' ? `Selected ${words(result.card.action.kind)} · gap ${gapLabel(result.card.gap)}`
      : result.status === 'unavailable' ? `Unavailable: ${words(result.reason)}${unknownWork ? ' · work unknown' : ''}`
        : 'Abstained · no selected card';
    return node('details', node('summary', `Assessment ${observed.ordinal} · ${applicability} · ${summary}`),
      node('p', `Partition ${observed.partition.index_id}; publication ${observed.partition.publication.token}.`),
      node('p', `Source units: ${observed.basis.source_units_presented.length} presented of ${observed.basis.source_units_available}; ${observed.basis.source_units_partial} partial; ${observed.basis.source_units_omitted} omitted. Read choices: ${observed.basis.read_choices_scored.length} scored of ${observed.basis.read_choices_available}; ${observed.basis.read_choices_omitted} omitted.`),
      node('p', 'These counts describe this bounded native recipe, not all facts in the index.'),
      ...(result.status === 'selected' ? [
        node('p', `Recorded action: ${words(result.card.action.kind)}${result.card.action.kind === 'read' ? ` · native choice ${result.card.action.choice}` : ''}. Gap: ${gapLabel(result.card.gap)}. Reason: ${words(result.card.reason)}.`),
        node('p', 'Roles and relevance below are whole-card hypotheses about the question, not new source facts or exhaustive coverage.'),
        ...result.card.assessments.map(assessment => node('div', node('p', `Role hypothesis: ${words(assessment.role)}. Relevance hypothesis: ${words(assessment.relevance)}.`), unit(assessment.evidence))),
      ] : result.status === 'unavailable' ? [
        node('p', `Assessment not applied: ${words(result.reason)}. ${unknownWork ? 'Actual optional work may be unknown; no zero-cost or replay claim.' : 'Known measured optional work is retained below.'}`),
      ] : [node('p', 'Native assessment abstained; it supplied no selected whole-card claim.')]),
      ...(advice !== null ? [adviceReceipt(advice)] : [
        node('p', 'No actual native advice reply was retained. Token reservation and observed-token receipt were not returned; no learning result is claimed.'),
        ...(result.status === 'unavailable' ? [measuredWork(result.work)] : []),
      ]),
      node('details', node('summary', 'Complete assessment and provenance'), node('pre', JSON.stringify(observed, null, 2))));
  };
  const coverage = answer.assessment.coverage;
  return node('section', node('h3', 'Evidence-grounded context'),
    node('p', `Local stop: ${words(answer.stop)}. Candidates: ${answer.candidate_count}; omitted: ${answer.omitted_count}. Evidence remains bounded by the original request.`),
    node('h4', 'Local partition assessment'),
    node('p', `Index ${answer.assessment.scope.index_id}; ${answer.assessment.scope.publication.collection}; publication ${answer.assessment.scope.publication.token}.`),
    node('p', `Termination: ${words(answer.assessment.termination)}. ${coverage.status === 'assessed' ? `Native coverage assessment refers to local ordinal ${coverage.ordinal}.` : `Coverage unassessed: ${words(coverage.reason)}.`}`),
    node('p', 'This local partition does not establish coverage for other authorized indexes. A sufficiency assessment is fallible, not exhaustive coverage.'),
    ...answer.paragraphs.map(paragraph => node('div', node('p', paragraph.text),
      node('p', paragraph.interpretation ? 'Interpretation of the cited evidence.' : 'Evidence-grounded statement.'),
      node('div', ...paragraph.evidence.map(citation)))),
    node('h4', 'Gaps and limits'), answer.gaps.length ? node('ul', ...answer.gaps.map(gap => node('li', gapLabel(gap))))
      : node('p', 'No additional gap was reported. This does not establish exhaustive coverage.'),
    node('h4', 'Assessment history'), ...answer.assessment_history.map(event),
    node('h4', 'Original evidence records'),
    ...answer.records.map(record => node('details', node('summary', `${record.operation} · ${record.role} · ${record.id.slice(0, 12)}`),
      { ...node('article', node('h5', record.id), node('pre', JSON.stringify(record, null, 2))), id: target(record.id) })),
    node('details', node('summary', 'Native work, application and assessment observations'),
      node('pre', JSON.stringify({ work: answer.work, applications: answer.applications, assessment: answer.assessment, assessment_history: answer.assessment_history }, null, 2))));
}
