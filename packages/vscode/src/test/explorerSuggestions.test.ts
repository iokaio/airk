// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { test } from 'node:test';
import { appendTasks, MANAGEMENT_INTENTS, validateDraft } from '../explorerDrafts';
import { Plan, PLAN_SCHEMA, validatePlan } from '../explorerStrategies';
import { SUGGESTION_LIMITS, SuggestionStore, suggestionsView } from '../explorerSuggestions';

const CLIENT = { name: 'claude-code', version: '2.0' };
function store(options: { binding?: string; revisions?: { studio?: number; planner?: number }; publication?: string } = {}) {
    let now = 1_000_000;
    const value = new SuggestionStore({ binding: () => options.binding ?? 'ab12ab12ab12ab12', revisions: () => options.revisions ?? {}, publication: () => options.publication,
        ttlMs: () => 24 * 3600_000, now: () => now });
    return { store: value, advance: (ms: number) => { now += ms; } };
}
const search = (key = 'search-1', extra: Record<string, unknown> = {}) => ({ schema_version: '1.0', idempotency_key: key, title: 'Find session refresh',
    draft: { query: 'session refresh', mode: 'lexical', path_prefix: 'src/auth/', limit: 20 }, ...extra });

test('a valid search draft is admitted for review only: never accepted, applied or executed', () => {
    const { store: inbox } = store();
    const result = inbox.submit('search', search(), CLIENT);
    assert.ok(result.ok);
    assert.equal(result.receipt.status, 'pending_review');
    assert.equal(result.receipt.destination, 'search/query');
    assert.deepEqual([result.receipt.accepted, result.receipt.applied, result.receipt.executed], [false, false, false]);
    assert.match(result.receipt.proposal_id, /^ex-prop-[0-9a-f]{16}$/);
    assert.equal(result.receipt.workspace_binding, 'ab12ab12ab12ab12');
    assert.equal(inbox.list()[0].client.name, 'claude-code');
});

test('the same idempotency key and body replays the proposal; a different body conflicts (AT-14)', () => {
    const { store: inbox } = store();
    const first = inbox.submit('search', search(), CLIENT);
    const again = inbox.submit('search', { ...search(), draft: { limit: 20, path_prefix: 'src/auth/', mode: 'lexical', query: 'session refresh' } }, CLIENT);
    assert.ok(first.ok && again.ok && first.receipt.proposal_id === again.receipt.proposal_id, 'key order does not matter');
    assert.equal(inbox.list().length, 1, 'no duplicate cards');
    const changed = inbox.submit('search', search('search-1', { title: 'Different' }), CLIENT);
    assert.deepEqual(changed.ok ? undefined : changed.code, 'PROPOSAL_CONFLICT');
});

test('payload security: unknown fields, actions, paths, nesting, size and schema versions are refused (AT-17)', () => {
    const cases: [string, unknown, string][] = [
        ['unknown field', search('k1', { extra: true }), 'INVALID_SCHEMA'],
        ['save action', search('k2', { save: true }), 'POLICY_DENIED'],
        ['command action', search('k3', { command: 'workbench.action.files.save' }), 'POLICY_DENIED'],
        ['absolute path', search('k4', { draft: { query: 'x', path_prefix: '/etc/' } }), 'INVALID_SCHEMA'],
        ['traversal', search('k5', { draft: { query: 'x', path_prefix: 'src/../../' } }), 'INVALID_SCHEMA'],
        ['nested', search('k6', { evidence: { publication: 'p', citations: [{ path: 'a', start_line: 1, content_hash: { a: { b: { c: { d: { e: { f: 1 } } } } } } }] } }), 'INVALID_SCHEMA'],
        ['oversize', search('k7', { draft: { query: 'x'.repeat(4000), mode: 'lexical' }, title: 'y'.repeat(150), evidence: { publication: 'p', citations: Array.from({ length: 50 }, (_, i) => ({ path: `src/${'z'.repeat(300)}${i}.ts`, start_line: 1 })) } }), 'INVALID_SCHEMA'],
        ['schema version', search('k8', { schema_version: '2.0' }), 'SCHEMA_UNSUPPORTED'],
        ['bad key', search('bad key with spaces'), 'INVALID_SCHEMA'],
    ];
    const { store: inbox } = store();
    for (const [name, input, code] of cases) {
        const result = inbox.submit('search', input, CLIENT);
        assert.equal(result.ok ? 'ok' : result.code, code, name);
    }
    assert.equal(inbox.list().length, 0, 'nothing refused reaches the Inbox');
});

test('strategy and plan drafts are validated exactly like Studio and Planner (AT-07, AT-09)', () => {
    const strategy = (extra: Record<string, unknown>) => ({ schema_version: '1.0', idempotency_key: `s-${JSON.stringify(extra).length}`, title: 'Investigate renewal', objective: 'Find the expiry decision.', ...extra });
    assert.ok(validateDraft('strategy', strategy({ template_id: 'airk.core/bug', parameters: { query: 'session refresh', symbol: 'refreshSession' } })).ok);
    assert.match(JSON.stringify(validateDraft('strategy', strategy({ template_id: 'airk.core/nope' }))), /not in the core pack/);
    const steps = [{ id: 'status', op: 'airk.status', args: {} }, { id: 'search', op: 'airk.search', args: { query: '${params.query}' } }, { id: 'read', op: 'airk.readFile', args: {}, input: 'search' }];
    const custom = validateDraft('strategy', strategy({ steps, parameters: { query: 'renew' } }));
    assert.ok(custom.ok && custom.draft.body.kind === 'strategy' && custom.draft.body.strategy.id === 'user/suggested');
    assert.match(JSON.stringify(validateDraft('strategy', strategy({ steps: [{ id: 'x', op: 'host.exec', args: {} }] }))), /unknown operator/);
    assert.match(JSON.stringify(validateDraft('strategy', strategy({ steps: [{ id: 'read', op: 'airk.readFile', args: {}, input: 'later' }, { id: 'later', op: 'airk.search', args: { query: 'x' } }] }))), /earlier step/);

    const plan = (tasks: unknown[], extra: Record<string, unknown> = {}) => ({ schema_version: '1.0', idempotency_key: 'p-1', title: 'Auth reliability', tasks, ...extra });
    const three = [{ task_id: 'T1', title: 'Investigate', depends_on: [] }, { task_id: 'T2', title: 'Test', depends_on: ['T1'], acceptance: 'Fails before, passes after.' },
        { task_id: 'T3', title: 'Document', depends_on: ['T2'] }];
    const checked = validateDraft('plan', plan(three, { path_hint: 'docs/airk-plans/auth.md' }));
    assert.ok(checked.ok && checked.draft.body.kind === 'plan');
    assert.deepEqual(checked.ok && checked.draft.body.kind === 'plan' ? checked.draft.body.plan.tasks.map(task => [task.id, task.dependsOn, task.state]) : [],
        [['T1', [], 'draft'], ['T2', ['T1'], 'draft'], ['T3', ['T2'], 'draft']]);
    assert.match(JSON.stringify(validateDraft('plan', plan([{ task_id: 'T1', title: 'a', depends_on: ['T2'] }, { task_id: 'T2', title: 'b', depends_on: ['T1'] }]))), /cycle/);
    assert.equal((validateDraft('plan', plan(three, { save: false })) as { code?: string }).code, 'POLICY_DENIED', 'plans carry no save field (review C8)');
    assert.equal(validateDraft('plan', plan(three, { path_hint: '../outside.md' })).ok, false);
});

test('binding, target revision, evidence and expiry are enforced (AT-13, AT-15, AT-16)', () => {
    const bound = store({ revisions: { studio: 4 }, publication: 'airk-r1:p:2' });
    assert.equal((bound.store.submit('search', search('b1', { workspace_binding: 'ffffffffffffffff' }), CLIENT) as { code?: string }).code, 'STALE_BINDING');
    const strategy = { schema_version: '1.0', idempotency_key: 'r1', title: 't', template_id: 'airk.core/bug', expected_target_revision: 3 };
    assert.equal((bound.store.submit('strategy', strategy, CLIENT) as { code?: string }).code, 'PROPOSAL_CONFLICT');
    const ok = bound.store.submit('strategy', { ...strategy, expected_target_revision: 4 }, CLIENT);
    assert.ok(ok.ok && ok.receipt.target_draft_revision === 4);
    const cited = bound.store.submit('search', search('e1', { evidence: { publication: 'airk-r1:p:1', citations: [{ path: 'src/a.ts', start_line: 3 }] } }), CLIENT);
    assert.ok(cited.ok && cited.receipt.warnings.some(warning => warning.startsWith('EVIDENCE_MISMATCH')), 'a stale citation goes to review with a warning, not a hidden fallback');

    const ttl = store();
    const admitted = ttl.store.submit('search', search(), CLIENT);
    assert.ok(admitted.ok);
    ttl.advance(24 * 3600_000 + 1);
    assert.equal(ttl.store.status(admitted.receipt.proposal_id)?.status, 'expired');
    assert.equal(ttl.store.resolve(admitted.receipt.proposal_id, 'applied'), undefined, 'an expired draft cannot be applied');
});

test('rate, count and byte limits bound the Inbox; Discard all rejects every open draft', () => {
    const { store: inbox, advance } = store();
    for (let index = 0; index < SUGGESTION_LIMITS.perMinute; index++) { assert.ok(inbox.submit('search', search(`r${index}`), CLIENT).ok); }
    assert.equal((inbox.submit('search', search('r-over'), CLIENT) as { code?: string }).code, 'RATE_LIMITED');
    advance(61_000);
    for (let index = 30; index < SUGGESTION_LIMITS.pending; index++) { assert.ok(inbox.submit('search', search(`r${index}`), CLIENT).ok); advance(2_100); }
    assert.equal((inbox.submit('search', search('r-full'), CLIENT) as { code?: string }).code, 'RATE_LIMITED', `at most ${SUGGESTION_LIMITS.pending} open drafts`);
    assert.equal(inbox.openCount(), SUGGESTION_LIMITS.pending);
    const first = inbox.list().at(-1)!;
    assert.equal(inbox.resolve(first.id, 'deferred')?.status, 'deferred');
    assert.equal(inbox.discardAll(), SUGGESTION_LIMITS.pending);
    assert.equal(inbox.openCount(), 0);
    assert.equal(inbox.resolve(first.id, 'applied'), undefined, 'rejected drafts stay rejected');
});

test('Inbox cards collapse duplicates and cap what is shown, so a looping agent cannot bury other drafts', () => {
    const { store: inbox, advance } = store();
    for (let index = 0; index < 21; index++) { inbox.submit('search', search(`other-${index}`, { title: `Other ${index}` }), CLIENT); advance(3000); }
    for (let index = 0; index < 25; index++) { inbox.submit('search', search(`loop-${index}`, { draft: { query: `q${index}` } }), CLIENT); advance(3000); }
    const view = suggestionsView(inbox.list());
    assert.equal(view.open, 46);
    const loop = view.cards.find(card => card.title === 'Find session refresh')!;
    assert.equal(loop.duplicates, 25, 'one card for 25 drafts with the same destination and title');
    assert.equal(view.cards.length, 20);
    assert.equal(view.hidden, 2);
    assert.equal(loop.client, 'claude-code');
    assert.match(loop.summary, /^"q\d+" · lexical$/);
    assert.equal(view.cards[0], loop, 'the newest group comes first');
});

test('appending suggested tasks renumbers them after the existing plan and keeps their dependencies', () => {
    const existing: Plan = { schema: PLAN_SCHEMA, title: 'Mine', objective: '', tasks: [{ id: 'T1', title: 'a', instructions: '', dependsOn: [], state: 'completed' },
        { id: 'T2', title: 'b', instructions: '', dependsOn: ['T1'], state: 'ready' }] };
    const suggested = [{ id: 'T1', title: 'x', instructions: '', dependsOn: [], state: 'draft' as const }, { id: 'T2', title: 'y', instructions: '', dependsOn: ['T1'], state: 'draft' as const }];
    const merged = appendTasks(existing, suggested);
    assert.deepEqual(merged.tasks.map(task => [task.id, task.title, task.dependsOn]), [['T1', 'a', []], ['T2', 'b', ['T1']], ['T3', 'x', []], ['T4', 'y', ['T3']]]);
    assert.ok(validatePlan(merged).ok);
});

test('review and management drafts only open a screen; management never runs an operation (AT-12)', () => {
    const { store: inbox } = store();
    const review = inbox.submit('review', { schema_version: '1.0', idempotency_key: 'rv', title: 'Check impact', scope: 'impact', names: ['renew'], note: 'Renewal changed.' }, CLIENT);
    assert.ok(review.ok && review.receipt.destination === 'review/impact');
    const guidance = inbox.submit('management', { schema_version: '1.0', idempotency_key: 'mg', title: 'Verify retained data', intent: 'recovery.verify_retained_data', mode: 'show_guidance' }, CLIENT);
    assert.ok(guidance.ok && guidance.receipt.destination === 'manage/repair' && guidance.receipt.executed === false);
    for (const [input, code] of [
        [{ intent: 'recovery.verify_retained_data', mode: 'run_now' }, 'POLICY_DENIED'],
        [{ intent: 'recovery.rebuild' }, 'INVALID_SCHEMA'],
        [{ intent: 'storage.remove_project' }, 'INVALID_SCHEMA'],
        [{ intent: 'service.review', command: 'airk.repair' }, 'POLICY_DENIED'],
        [{ intent: 'service.review', execute: true }, 'POLICY_DENIED'],
    ] as const) {
        const result = validateDraft('management', { schema_version: '1.0', idempotency_key: 'x', title: 't', ...input });
        assert.equal(result.ok ? 'ok' : result.code, code, JSON.stringify(input));
    }
    for (const intent of Object.values(MANAGEMENT_INTENTS)) {
        assert.match(intent.route, /^manage\/[a-z]+$/);
        assert.doesNotMatch(intent.guidance, /\b(automatically|will run|runs now)\b/i);
    }
    assert.equal(validateDraft('review', { schema_version: '1.0', idempotency_key: 'r', title: 't', scope: 'run_tests' }).ok, false);
});

test('no draft path can reach a management action, a command or the shared console', () => {
    const root = path.resolve(__dirname, '..', '..', 'src');
    const files = ['explorerDrafts.ts', 'explorerSuggestions.ts', 'explorerAssistanceHost.ts', 'explorerWebview/inbox.tsx',
        ...fs.readdirSync(path.join(root, 'explorerAssistance')).map(name => `explorerAssistance/${name}`)];
    for (const file of files) {
        const text = fs.readFileSync(path.join(root, file), 'utf8');
        // The window endpoint writes only its own user-only descriptor; nothing else here may write files.
        for (const forbidden of [/manage\.(action|command)/, /executeCommand/, /\bcontrol\.run\b/, /child_process/, ...(file === 'explorerAssistanceHost.ts' ? [] : [/writeFile/])]) {
            assert.doesNotMatch(text, forbidden, `${file} must not reference ${forbidden}`);
        }
    }
    const controller = fs.readFileSync(path.join(root, 'explorerController.ts'), 'utf8');
    const handlers = controller.slice(controller.indexOf("case 'suggestions.get'"), controller.indexOf("case 'review.changes'"));
    assert.ok(handlers.length > 100, 'suggestion handlers found');
    assert.doesNotMatch(handlers, /control\.run|executeCommand|manage\.action/, 'resolving a suggestion only records the decision');
});
