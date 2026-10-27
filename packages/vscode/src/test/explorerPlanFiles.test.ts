// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { parsePlanFile } from '../explorerPlanFiles';
import { Plan, PLAN_SCHEMA, renderPlan } from '../explorerStrategies';

const plan: Plan = { schema: PLAN_SCHEMA, title: 'A saved plan', objective: 'Keep the original instructions.', project: 'ab'.repeat(32), tasks: [
    { id: 'T1', title: 'Inspect', instructions: 'Read the implementation.\n\nKeep this paragraph.', dependsOn: [], state: 'completed',
        evidence: { template: 'airk.core/bug@1.0.0', publication: 'airk-r1:fixture', digest: 'cd'.repeat(32), outcome: 'verified_snapshot' } },
    { id: 'T2', title: 'Verify', instructions: 'Run the actual checks.', dependsOn: ['T1'], state: 'ready' },
] };
test('JSON import preserves complete plan state and provenance without executing anything', () => {
    const imported = parsePlanFile('\uFEFF' + JSON.stringify(plan, null, 2)); assert.equal(imported.format, 'json'); assert.deepEqual(imported.plan, plan);
    assert.ok(imported.warnings.length);
});
test('AIRK Markdown reloads all task instructions and dependencies as explicitly unverified drafts', () => {
    const text = renderPlan(plan, { workspace: 'fixture', extensionVersion: '1.5.0', currentPublication: 'airk-r1:new' });
    const imported = parsePlanFile(text.replaceAll('\n', '\r\n')); assert.equal(imported.format, 'markdown');
    assert.equal(imported.plan.title, plan.title); assert.equal(imported.plan.objective, plan.objective); assert.equal(imported.plan.project, plan.project);
    assert.deepEqual(imported.plan.tasks.map(task => [task.id, task.title, task.instructions, task.dependsOn]), plan.tasks.map(task => [task.id, task.title, task.instructions, task.dependsOn]));
    assert.ok(imported.plan.tasks.every(task => task.state === 'draft' && !task.evidence)); assert.match(imported.warnings.join(' '), /without verified evidence/);
});
test('ambiguous Markdown, contradictory dependencies, invalid JSON and oversized plans fail instead of clipping or guessing', () => {
    const text = renderPlan(plan, { workspace: 'fixture', extensionVersion: '1.5.0' });
    for (const value of ['# arbitrary Markdown', text.replace('# A saved plan', '# Edited heading'), text.replace('Depends on: T1', 'Depends on: T2'),
        text + '\n## T2. Duplicate task heading\n', JSON.stringify({ ...plan, command: 'run' }), '{not json', '漢'.repeat(100000)]) {
        assert.throws(() => parsePlanFile(value));
    }
});
