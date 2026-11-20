// SPDX-License-Identifier: Apache-2.0
'use strict';

const { isDeepStrictEqual: equal } = require('node:util');
const {windowStatus} = require('./window_scope.cjs');

function requireFact(condition, message) {
    if (!condition) throw new Error(message);
}

function phaseRows(questions, ids, phase) {
    requireFact(Number.isInteger(phase) && phase >= 0 && phase < 13, 'invalid phase ordinal');
    requireFact(ids.length === 100 && new Set(ids).size === 100 && questions.length === 100 &&
        questions.every((row, i) => row.id === ids[i] && row.split === 'calibration' &&
            typeof row.question === 'string' && row.question.length > 0), 'CAL order or partition differs');
    return questions.slice(phase * 8, (phase + 1) * 8);
}

function selected(body, binding) {
    const obs = body.scope_observation;
    requireFact(!Object.hasOwn(body,'read_scope') && obs?.selection?.owner === 'window' &&
        equal(obs, binding.scope_observation),
        'canonical result lost fixed U110 selection');
}

function associate(record, answer, question, binding) {
    const request = record.request;
    requireFact(record.schema === 'airk.private-selected-Context-native-admission-observation/1' &&
        record.Context_NN_dispatch === false && record.Focus_configuration === 'ON' &&
        record.actual_Context_profile?.manifest === binding.context_manifest &&
        record.actual_Focus_profile === binding.focus_manifest && record.MiniLM_pipeline === binding.pipeline &&
        request.question === question && /^[a-f0-9]{32}$/.test(request.read_attempt.request_id) &&
        /^[a-f0-9]{32}$/.test(answer.request_id) &&
        equal(request.scope, answer.assessment.scope) && equal(request.scope.publication, binding.publication),
        'admission profile, question, publication or scope differs');
    // The delivered MCP envelope carries the telemetry span ID. The service's
    // read-attempt ID has a separate issuer. Join the exact native source records
    // from the admission to the delivered answer and its original assessment.
    requireFact(Array.isArray(request.records) && request.records.length > 0 && Array.isArray(answer.records) &&
        equal(request.records, answer.records.slice(0, request.records.length)), 'source record association differs');
    const matching = answer.assessment_history.filter(event => event.ordinal === request.ordinal);
    const advice = matching[0]?.result?.advice;
    requireFact(matching.length === 1 && equal(matching[0].basis, request.roster.basis) &&
        advice?.input_digest === record.planned_input_digest && advice?.observed_input_tokens === 0 &&
        advice?.work?.input_tokens === 0 && advice?.outcome?.status === 'unavailable',
        'admission association or Context non-dispatch differs');
    const probes = record.verified_probe_requests;
    requireFact(Array.isArray(probes) && probes.length <= 5 &&
        new Set(probes.map(p => p.choice.id)).size === probes.length &&
        probes.every(p => ['search', 'context'].includes(p.tool) &&
            equal(p.publication, binding.publication) && p.arguments?.response_mode === 'full' &&
            p.arguments.include_status === true), 'invalid native probe roster');
    return probes;
}

function scopeBinding(status, binding, grant) {
    return { ...binding, scope_observation: windowStatus(status,binding,grant) };
}

// The outer worker keeps its original absolute phase/fixture clocks and owns the
// SDK child, fresh service, native observer directory and append-only receipt.
// Require the full 120 s call window plus the native 5 s delivery grace.
async function invoke(owner, binding, tool, args) {
    owner.checkRemaining(125000);
    const argv = ['tools', 'call', tool, '--args', JSON.stringify(args), '--output', 'json'];
    const result = await owner.session.invoke(argv, { deadlineMs: 125000 });
    await owner.retainExchange(argv, result); // retain even unparseable or refused calls
    owner.checkRemaining(0);
    requireFact(owner.session.hello?.serviceInstance === binding.instance, 'original service instance changed');
    const envelope = JSON.parse(result.stdout);
    let body = envelope.structuredContent;
    if (!body) {
        requireFact(envelope.content?.length === 1 && envelope.content[0].type === 'text',
            'missing full canonical result');
        body = JSON.parse(envelope.content[0].text);
    }
    if (result.exitCode === 1 && result.class === 'refused' && envelope.isError === true &&
        body.error?.code === 'verification_reply_budget' && body.error.retryable === false &&
        body.error.verification_performed === false) {
        return { refused: true, body };
    }
    requireFact(result.exitCode === 0 && result.class === 'success' && envelope.isError !== true,
        'native call did not deliver its complete result');
    return { refused: false, body };
}

function verifySearch(body, binding) {
    requireFact(equal(body.publication, binding.publication) &&
        body.semantic_readiness?.state === 'ready' &&
        body.semantic_readiness.pipeline_digest === binding.pipeline &&
        ['hybrid', 'vector'].includes(body.execution?.served_mode) && body.execution.incomplete === false,
        'delivered Search did not serve the original complete MiniLM pipeline');
}

async function capturePhase(owner, binding, inputs, phase) {
    const rows = phaseRows(inputs.questions, inputs.ids, phase);
    const template = inputs.answer;
    requireFact(template.budget?.input_tokens === 8192 && template.budget.deadline_ms === 120000 &&
        template.response_mode === 'full' && template.include_status === true, 'incorrect native Answer budget');
    // The owner checks process identity, fresh observer mode and warmed stages
    // before any dispatch. It refuses retries of a started phase.
    await owner.beginPhase(phase, binding);
    const fingerprints = new Set();
    for (const row of rows) {
        const grant = await owner.beginQuestion(row.id);
        const status = await invoke(owner, binding, 'status', { response_mode: 'full', include_status: true });
        requireFact(!status.refused, 'scope status refused');
        const pinned = scopeBinding(status.body, binding, grant);
        requireFact(!fingerprints.has(pinned.scope_observation.fingerprint), 'previous native question grant was reused');
        fingerprints.add(pinned.scope_observation.fingerprint);
        const before = await owner.observationCursor();
        const answer = await invoke(owner, binding, 'answer', { ...template, question: row.question });
        const captured = await owner.observationsAfter(before);
        requireFact(captured.length <= 1, 'unexpected extra native assessment');
        const evidence = { id: row.id, scope: status.body, answer, captured, probes: [], pipeline_searches: 0 };
        if (!answer.refused) {
            selected(answer.body, pinned);
            requireFact(answer.body.question === row.question, 'wrong delivered question');
            for (const record of answer.body.records) {
                if (record.operation === 'search') {
                    verifySearch(record.canonical_result, pinned);
                    evidence.pipeline_searches++;
                }
            }
            if (captured.length) {
                for (const probe of associate(captured[0], answer.body, row.question, pinned)) {
                    const result = await invoke(owner, binding, probe.tool, probe.arguments);
                    if (!result.refused) {
                        selected(result.body, pinned);
                        if (probe.tool === 'search') {
                            verifySearch(result.body, pinned);
                            evidence.pipeline_searches++;
                        }
                        requireFact(equal(result.body.publication, binding.publication), 'probe publication changed');
                    }
                    evidence.probes.push({ probe, result });
                }
            }
        }
        await owner.completeQuestion(evidence);
        await owner.endQuestion(row.id);
    }
    await owner.closePhase(phase, rows.map(q => q.id));
}

module.exports = { phaseRows, selected, associate, scopeBinding, invoke, verifySearch, capturePhase };
