// SPDX-License-Identifier: Apache-2.0
// Core emits both runtime values and semantic cases; this consumer adds no native DTO mirror.
import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as path from 'node:path';
import test from 'node:test';
import * as ts from 'typescript';
import { DEFAULT_CONTEXT_BUDGET, READ_OPERATION_DEADLINE_MS, READ_TRANSPORT_OVERHEAD_MS, READ_TRANSPORT_DEADLINE_MS } from '../generated/arbiterBudget';
import { WORKBENCH_CONTEXT_BUDGET } from '../workbenchContracts';
const fixture = JSON.parse(fs.readFileSync(path.resolve(__dirname, '../../../../fixtures/split/arbiter/budget_cases.json'), 'utf8'));

test('Workbench consumes the exact Rust-emitted8192/120000 default without widening Focus pair width', () => {
    assert.equal(fixture.schema, 'airk.arbiter.budget-cases/1');
    assert.equal(fixture.qualification, false);
    assert.deepEqual(DEFAULT_CONTEXT_BUDGET, { calls: 8, rounds: 3, fanout: 8, input_tokens: 8192, candidates: 32, max_bytes: 262144, deadline_ms: 120000 });
    assert.deepEqual(WORKBENCH_CONTEXT_BUDGET, fixture.default_budget);
    assert.deepEqual(DEFAULT_CONTEXT_BUDGET, fixture.default_budget);
    assert.equal(fixture.historical_control.input_tokens, 2048);
    assert.equal(fixture.historical_control.deadline_ms, 30000);
    assert.equal(fixture.selected_focus_pair_tokens, 512);
});
test('delivery overhead is exactly5000 beyond the120000 operation without granting more operation time', () => {
    assert.equal(READ_OPERATION_DEADLINE_MS, 120000);
    assert.equal(READ_TRANSPORT_OVERHEAD_MS, 5000);
    assert.equal(READ_TRANSPORT_DEADLINE_MS, 125000);
    assert.equal(READ_TRANSPORT_DEADLINE_MS, READ_OPERATION_DEADLINE_MS + READ_TRANSPORT_OVERHEAD_MS);
    assert.equal(READ_OPERATION_DEADLINE_MS, fixture.operation_deadline_ms);
    assert.equal(READ_TRANSPORT_DEADLINE_MS, fixture.transport_deadline_ms);
});
test('the generated runtime budget module contains only the four declared literal data exports', () => {
    const file = path.resolve(__dirname, '../../src/generated/arbiterBudget.ts');
    const tree = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
    const names: string[] = [];
    for (const statement of tree.statements) {
        assert.ok(ts.isVariableStatement(statement), 'no imports, functions or startup statements');
        for (const declaration of statement.declarationList.declarations) {
            assert.ok(ts.isIdentifier(declaration.name)); names.push(declaration.name.text);
            let value = declaration.initializer; if (value && ts.isAsExpression(value)) value = value.expression;
            assert.ok(value && (ts.isNumericLiteral(value) || ts.isObjectLiteralExpression(value)), 'literal data only');
            if (ts.isObjectLiteralExpression(value)) for (const property of value.properties) {
                assert.ok(ts.isPropertyAssignment(property) && ts.isNumericLiteral(property.initializer));
            }
        }
    }
    assert.deepEqual(names.sort(), ['DEFAULT_CONTEXT_BUDGET', 'READ_OPERATION_DEADLINE_MS', 'READ_TRANSPORT_DEADLINE_MS', 'READ_TRANSPORT_OVERHEAD_MS'].sort());
});
