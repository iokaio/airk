// SPDX-License-Identifier: Apache-2.0
// Actual API, envelope, Outcome and component callbacks; no DOM qualification.
import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';

const here = dirname(fileURLToPath(import.meta.url));
const sourceDir = resolve(here, '../src');
const repo = resolve(here, '../../..');
const recoveryCases = JSON.parse(await readFile(resolve(repo, 'fixtures/split/i3/recovery_reviews.json'), 'utf8'));
const nativeCases = JSON.parse(await readFile(resolve(repo, 'fixtures/split/i3/native_outcomes.json'), 'utf8'));
function receiptCase(name) {
  const found = nativeCases.recovery_receipts?.find(row => row.name === name);
  assert.ok(found, `independent native receipt case ${name}`);
  return found;
}
const token = '5f'.repeat(16);
const indexId = `idx_${'01'.repeat(16)}`;
const operationId = '02'.repeat(16);

const bundled = await build({
  stdin: { contents: `export * from './api'; export * from './envelope'; export { Outcome } from './render';
    export { ApplyCatalog, ApplySetup, MakeReadable, PolicyPanel, SplitStatus, ReviewHint } from './indexLibrary';`,
    resolveDir: sourceDir, sourcefile: 'r04-observation.ts' },
  bundle: true, write: false, format: 'esm', platform: 'node', target: 'node24',
  jsx: 'automatic', jsxImportSource: 'preact',
  plugins: [{ name: 'r04-observation', setup(plugin) {
    plugin.onResolve({ filter: /^preact\/hooks$/ }, () => ({ path: 'hooks', namespace: 'r04-hooks' }));
    plugin.onLoad({ filter: /.*/, namespace: 'r04-hooks' }, () => ({ contents: `
      const host=()=>globalThis.__airkR04Hooks;
      export function useState(initial){const h=host(),i=h.cursor++; if(!(i in h.slots))h.slots[i]=typeof initial==='function'?initial():initial;
        return[h.slots[i],next=>{h.slots[i]=typeof next==='function'?next(h.slots[i]):next;h.schedule?.();}];}
      export function useRef(initial){const h=host(),i=h.cursor++;return h.slots[i]??=( {current:initial} );}
      function effect(make,deps){const h=host(),i=h.cursor++,previous=h.effects.get(i);
        if(!previous||!deps||!previous.deps||deps.length!==previous.deps.length||deps.some((d,n)=>!Object.is(d,previous.deps[n])))
          h.pendingEffects.push(()=>{previous?.cleanup?.();h.effects.set(i,{deps,cleanup:make()});});}
      export function useEffect(make,deps){effect(make,deps);}
      export function useLayoutEffect(make,deps){effect(make,deps);}
      export function useMemo(make){host().cursor++;return make();}
      export function useCallback(callback){host().cursor++;return callback;}
    ` }));
    plugin.onResolve({ filter: /^\.\/session$/ }, args => (args.importer.endsWith('api.ts') || args.importer.endsWith('indexLibrary.tsx'))
      ? { path: 'session', namespace: 'r04-session' } : undefined);
    plugin.onLoad({ filter: /.*/, namespace: 'r04-session' }, () => ({ contents: `
      export function currentSession(){return globalThis.__airkR04Session;}
      export function dropSession(){globalThis.__airkR04Session=null;}
    ` }));
    plugin.onLoad({ filter: /indexLibrary\.tsx$/ }, async args => ({
      contents: `${await readFile(args.path, 'utf8')}\nexport { ApplyCatalog, ApplySetup, MakeReadable, PolicyPanel, SplitStatus, ReviewHint };`,
      loader: 'tsx', resolveDir: dirname(args.path),
    }));
  } }],
});
const actual = await import(`data:text/javascript;base64,${Buffer.from(bundled.outputFiles[0].text).toString('base64')}`);

function failed(code, { handle = false, native = { status: 'retained-native', future: { detail: ['complete', code] } }, status = 503 } = {}) {
  return { status, body: { schema: 'airk-admin/1', ok: false,
    error: { code, class: status === 409 ? 'refused' : 'unavailable', retryable: false, native },
    ...(handle ? { review: token } : {}) } };
}
function succeeded(operation, result, review = undefined) {
  return { status: 200, body: { schema: 'airk-admin/1', ok: true, operation, result, ...(review ? { review } : {}) } };
}
function observed(reply) { return actual.parseEnvelope(reply.status, JSON.stringify(reply.body)); }
function host(session = { csrf: 'fixture-only' }) {
  const hooks = { cursor: 0, slots: [], effects: new Map(), pendingEffects: [] };
  globalThis.__airkR04Hooks = hooks;
  globalThis.__airkR04Session = session;
  const calls = [];
  const replies = [];
  let component = null, props = {}, tree = null, closed = false, scheduled = false;
  const render = (nextComponent, nextProps = {}) => {
    component = nextComponent; props = nextProps;
    hooks.cursor = 0; globalThis.__airkR04Hooks = hooks;
    tree = component(props);
    // Only the exported component runs here; nested VNodes do not mount. Its
    // real subscription and cleanup effects commit after rendering, allowing
    // pending replies to notify a successor without invoking nested auto reads.
    const effects = hooks.pendingEffects.splice(0);
    for (const effect of effects) effect();
    return tree;
  };
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async (url, init) => {
    const operation = String(url).split('/').at(-1);
    const body = JSON.parse(init.body);
    calls.push({ operation, body });
    const reply = replies.shift();
    assert.ok(reply, `unexpected native call ${operation}`);
    assert.equal(operation, reply.operation);
    if (reply.wait) await reply.wait;
    return { status: reply.status, text: async () => JSON.stringify(reply.body) };
  };
  return {
    hooks, calls,
    queue(operation, reply) { replies.push({ operation, ...reply }); },
    render,
    automaticRenders() {
      hooks.schedule = () => {
        if (scheduled || closed) return;
        scheduled = true;
        queueMicrotask(() => {
          scheduled = false;
          if (!closed && component) render(component, props);
        });
      };
    },
    latestTree() { return tree; },
    async idle() {
      const deadline = Date.now() + 5000;
      do {
        await new Promise(resolve => setImmediate(resolve));
        if (!hooks.slots.some(slot => slot?.phase === 'pending')) return;
      } while (Date.now() < deadline);
      assert.fail('actual callback did not finish under observation deadline');
    },
    close() {
      closed = true;
      for (const effect of hooks.effects.values()) effect.cleanup?.();
      hooks.effects.clear();
      globalThis.fetch = originalFetch; delete globalThis.__airkR04Hooks; delete globalThis.__airkR04Session;
    },
  };
}
function nodes(value) {
  if (value == null || typeof value === 'boolean') return [];
  if (Array.isArray(value)) return value.flatMap(nodes);
  if (typeof value !== 'object') return [value];
  return [value, ...nodes(value.props?.children)];
}
function text(value) {
  return nodes(value).map(node => typeof node === 'string' || typeof node === 'number' ? String(node)
    : node.type?.name === 'Text' ? node.props.value : '').join(' ');
}
function button(tree, label) {
  const found = nodes(tree).find(node => node?.type === 'button' && text(node.props.children).trim() === label);
  assert.ok(found, `actual button ${label}`);
  return found;
}
function done(operation, result) { return { phase: 'done', operation, result, at: 0 }; }
function click(tree, label) {
  const control = button(tree, label);
  assert.notEqual(control.props.disabled, true, `${label} is available`);
  control.props.onClick();
}

test('actual API preserves unknown503/native409 evidence, and failure recovery never becomes an apply review', async () => {
  const f = host();
  try {
    const parsedUnknown = [];
    for (const code of recoveryCases.unknown_codes) {
      const reply = failed(code, { handle: true });
      f.queue('catalog.policy_resume', reply);
      const result = await actual.call('catalog.policy_resume', { catalog_policy_resume: { index_id: indexId, operation_id: operationId } });
      assert.equal(result.kind, 'failed');
      assert.deepEqual(result.envelope, reply.body, code);
      assert.equal(actual.reviewOf(result), null, 'failure handle never authorizes generic apply');
      parsedUnknown.push([code, result]);
    }
    assert.equal(typeof actual.recoveryOf, 'function', 'dedicated receipt-only recovery accessor');
    assert.equal(typeof actual.isUnknownOutcome, 'function', 'shared-fixture-tested semantic predicate');
    for (const [code, result] of parsedUnknown) {
      assert.equal(actual.recoveryOf(result), token, code);
      assert.equal(actual.isUnknownOutcome(result), true, code);
    }
    for (const change of [
      reply => { delete reply.body.review; },
      reply => { reply.body.review = 'short'; },
      reply => { reply.body.review = 'AB'.repeat(16); },
      reply => { reply.body.error.retryable = true; },
      reply => { reply.body.error.class = 'refused'; },
      reply => { reply.status = 409; },
    ]) {
      const reply = failed('catalog_apply_outcome_unknown', { handle: true });
      change(reply);
      assert.equal(actual.recoveryOf(observed(reply)), null, 'invalid/missing failure handle is never promoted');
    }
    for (const code of recoveryCases.not_unknown_codes) {
      const result = observed(failed(code, { handle: true, status: 409 }));
      assert.equal(actual.recoveryOf(result), null, code);
      assert.equal(actual.isUnknownOutcome(result), false, code);
    }
    const capacity = observed(failed('admin_review_capacity', { status: 409 }));
    assert.equal(capacity.kind, 'failed');
    assert.equal(actual.reviewOf(capacity), null);
    assert.equal(actual.recoveryOf(capacity), null);
    const success = observed(succeeded('catalog.preview', { retained: 'complete' }, token));
    assert.equal(actual.reviewOf(success), token);
    assert.equal(actual.recoveryOf(success), null);
  } finally { f.close(); }
});

test('actual Outcome explains unknown recovery without completed progress and retains complete native evidence', () => {
  const f = host();
  try {
    const reply = failed('catalog_apply_outcome_unknown', { handle: true });
    const tree = actual.Outcome({ state: done('catalog.policy_resume', observed(reply)) });
    assert.match(text(tree), /unknown/i);
    assert.match(text(tree), /receipt|saved requests/i, 'actionable unknown guidance');
    assert.doesNotMatch(text(tree), /Request .*completed|Completed\./);
    const evidence = nodes(tree).find(node => node?.type?.name === 'JsonBlock');
    assert.deepEqual(evidence?.props.value, reply.body.error.native);
  } finally { f.close(); }
});

for (const [name, component, props, operation, label] of [
  ['catalog', 'ApplyCatalog', { token, staged: { request: { independent: true }, previewDigest: '03'.repeat(32) }, confirm: null, label: 'Apply', onReviewAgain() {}, onApplied() { assert.fail('unknown catalog apply progressed'); } }, 'catalog.apply', 'Apply'],
  ['setup', 'ApplySetup', { token, draft: {}, onRefresh() {}, onActivity() {}, onReviewAgain() {} }, 'catalog.index_setup_apply', 'Create index'],
]) {
  test(`actual ${name} apply exposes original receipt for every typed unknown and never treats it accepted`, async () => {
    for (const code of recoveryCases.unknown_codes) {
      const f = host();
      try {
        f.queue(operation, failed(code));
        let tree = f.render(actual[component], props);
        click(tree, label);
        await f.idle();
        tree = f.render(actual[component], props);
        const receiptOperation = name === 'setup' ? 'catalog.index_setup_receipt' : 'catalog.receipt';
        f.queue(receiptOperation, succeeded(receiptOperation, { complete: 'receipt' }));
        click(tree, 'Read receipt');
        await f.idle();
        assert.equal(f.calls.length, 2, code);
        assert.equal(f.calls[1].operation, receiptOperation);
        assert.equal(f.calls[1].body.review, token, 'original reviewed identity');
        assert.doesNotMatch(text(tree), /was accepted/);
      } finally { f.close(); }
    }
  });
}

for (const row of recoveryCases.applicability.filter(row => row.receipt !== null)) {
  test(`actual ${row.operation} unknown offers only ${row.receipt} using its recovery handle`, async () => {
    const f = host();
    try {
      const component = row.operation === 'catalog.policy_resume' ? actual.PolicyPanel : actual.SplitStatus;
      const props = { indexId };
      let tree = f.render(component, props);
      const fields = nodes(tree).filter(node => node?.type?.name === 'Field');
      fields.find(field => /operation id/.test(field.props.label)).props.onInput(operationId);
      if (row.operation === 'catalog.split_recover') {
        fields.find(field => field.props.label === 'Request digest').props.onInput('03'.repeat(32));
        fields.find(field => field.props.label === 'Expected receipt digest').props.onInput('04'.repeat(32));
      }
      tree = f.render(component, props);
      f.queue(row.operation, failed('catalog_apply_outcome_unknown', { handle: true }));
      if (row.operation === 'catalog.policy_resume') click(tree, 'Resume policy change');
      else nodes(tree).find(node => node?.type?.name === 'Confirm').props.onConfirm();
      await f.idle();
      tree = f.render(component, props);
      f.queue(row.receipt, succeeded(row.receipt, { independent: 'receipt' }));
      click(tree, row.operation === 'catalog.policy_resume' ? 'Read resume receipt' : 'Read recovery receipt');
      await f.idle();
      assert.deepEqual(f.calls.map(call => call.operation), [row.operation, row.receipt]);
      assert.equal(f.calls[1].body.review, token);
      assert.equal(actual.reviewOf(observed(failed('catalog_apply_outcome_unknown', { handle: true }))), null);
    } finally { f.close(); }
  });
}

for (const stage of ['create', 'member']) {
  test(`actual Make readable stops at typed unknown ${stage} apply and keeps its original receipt`, async () => {
    for (const code of recoveryCases.unknown_codes) {
      const f = host();
      try {
        const props = { index: { id: indexId, name: 'Docs' }, memberCount: 0,
          onChanged() { assert.fail('unknown Make readable progressed'); } };
        const page = items => ({ status: 'catalog_page', page: { revision: 3, items, continuation: null } });
        const projectId = `prj_${'09'.repeat(16)}`;
        f.queue('catalog.page', succeeded('catalog.page', page([{ kind: 'source', value: {
          id: `src_${'08'.repeat(16)}`, index_id: indexId, enabled: true,
        } }])));
        f.queue('catalog.page', succeeded('catalog.page', page(stage === 'create' ? [] : [{ kind: 'project', value: {
          id: projectId, name: 'Docs', scope_revision: 1,
        } }])));
        if (stage === 'member') {
          f.queue('catalog.page', succeeded('catalog.page', page([])));
          f.queue('catalog.detail', succeeded('catalog.detail', { status: 'catalog_detail', detail: {
            revision: 3, item: { kind: 'project', value: { id: projectId, name: 'Docs', scope_revision: 1 } },
          } }));
        }
        f.queue('catalog.preview', succeeded('catalog.preview', nativeCases.review.response, token));
        f.queue('catalog.apply', failed(code));
        click(f.render(actual.MakeReadable, props), 'Make readable');
        await f.idle();
        const tree = f.render(actual.MakeReadable, props);
        assert.equal(f.calls.at(-1).operation, 'catalog.apply', 'never advance after unknown');
        assert.doesNotMatch(text(tree), /The index is readable through/);
        f.queue('catalog.receipt', succeeded('catalog.receipt', { retained: 'complete receipt' }));
        click(tree, stage === 'create' ? 'Read index set receipt' : 'Read membership receipt');
        await f.idle();
        assert.equal(f.calls.at(-1).operation, 'catalog.receipt');
        assert.equal(f.calls.at(-1).body.review, token);
      } finally { f.close(); }
    }
  });
}

test('actual accepted/completed controls preserve success progression and ordinary review tokens', async () => {
  for (const name of ['job_accepted', 'catalog_completed']) {
    const native = nativeCases.cases.find(row => row.name === name).response;
    const f = host();
    try {
      const props = { token, draft: {}, onRefresh() {}, onActivity() {}, onReviewAgain() {} };
      f.queue('catalog.index_setup_apply', succeeded('catalog.index_setup_apply', native));
      click(f.render(actual.ApplySetup, props), 'Create index');
      await f.idle();
      const tree = f.render(actual.ApplySetup, props);
      assert.match(text(tree), /was accepted/);
      assert.ok(button(tree, 'Read receipt'));
    } finally { f.close(); }
  }
});

test('actual expired policy/split receipt guidance never replays the generating mutation', async () => {
  for (const component of [actual.PolicyPanel, actual.SplitStatus]) {
    const f = host();
    try {
      // useCall occupies state/ref pairs: status, generated mutation, receipt.
      f.hooks.slots[4] = done('receipt', observed(failed('admin_review_unknown', { status: 409 })));
      const tree = f.render(component, { indexId });
      const hints = nodes(tree).filter(node => node?.type?.name === 'ReviewHint');
      for (const hint of hints) {
        const rendered = hint.type(hint.props);
        for (const control of nodes(rendered).filter(node => node?.type === 'button')) control.props.onClick();
      }
      await f.idle();
      assert.ok(f.calls.every(call => call.operation !== 'catalog.policy_resume' && call.operation !== 'catalog.split_recover'), 'expired receipt is not permission to regenerate');
      assert.doesNotMatch(hints.map(hint => text(hint.type(hint.props))).join(' '), /run the review again/i);
      const receiptOutcomes = nodes(tree).filter(node => node?.type?.name === 'Outcome'
        && node.props.state?.phase === 'done' && node.props.state.result?.envelope?.error?.code === 'admin_review_unknown');
      assert.ok(receiptOutcomes.length > 0, 'expired receipt remains visible');
      assert.doesNotMatch(receiptOutcomes.map(node => text(node.type(node.props))).join(' '), /run the review again/i, 'receipt-only Outcome never recommends mutation review/replay');
    } finally { f.close(); }
  }
});

test('native unknown/busy controls use the reviewed source evidence roster', () => {
  assert.ok(nativeCases.cases.some(row => row.name === 'catalog_unknown'));
  assert.ok(nativeCases.cases.some(row => /busy/.test(row.name)));
  for (const row of nativeCases.cases) {
    assert.ok(row.response && typeof row.response.status === 'string', row.name);
  }
});

test('custody policy recovery keeps submitted intent after form edits and cannot mint a new key while unresolved', async () => {
  const f = host();
  try {
    const known = receiptCase('policy_known_accepted');
    const missing = receiptCase('policy_receipt_missing');
    assert.equal(known.expect.resolved, true);
    assert.equal(missing.expect.resolved, false);
    const props = { indexId };
    const original = known.request.catalog_policy_resume.operation_id;
    const replacement = '09'.repeat(16);
    let tree = f.render(actual.PolicyPanel, props);
    nodes(tree).find(node => node?.type?.name === 'Field' && node.props.label === 'Policy operation id').props.onInput(original);
    tree = f.render(actual.PolicyPanel, props);
    f.queue('catalog.policy_resume', failed('catalog_apply_outcome_unknown', { handle: true }));
    click(tree, 'Resume policy change');
    button(tree, 'Resume policy change').props.onClick(); // The same captured callback before rerender.
    await f.idle();
    assert.equal(f.calls.filter(call => call.operation === 'catalog.policy_resume').length, 1, 'reserve submitted intent synchronously before awaiting');
    assert.deepEqual(f.calls[0].body, known.request, 'the original method comes from the independently typed native case');
    tree = f.render(actual.PolicyPanel, props);
    nodes(tree).find(node => node?.type?.name === 'Field' && node.props.label === 'Policy operation id').props.onInput(replacement);
    tree = f.render(actual.PolicyPanel, props);
    assert.equal(button(tree, 'Resume policy change').props.disabled, true, 'new generating mutation stays locked');
    for (const result of [missing.expect.native,
      { ...known.expect.native, expired: true },
      { ...known.expect.native, operation_id: replacement },
    ]) {
      f.queue('catalog.policy_resume_receipt', succeeded('catalog.policy_resume_receipt', result));
      click(tree, 'Read resume receipt');
      await f.idle();
      assert.deepEqual(f.calls.at(-1).body, { method: { catalog_policy_resume_receipt: {
        index_id: indexId, operation_id: original,
      } }, review: token }, 'receipt is bound to submitted A, never edited B');
      tree = f.render(actual.PolicyPanel, props);
      assert.equal(button(tree, 'Resume policy change').props.disabled, true, 'null/expired/mismatched receipt is not resolution');
    }
    f.queue(known.operation, succeeded(known.operation, known.expect.native));
    click(tree, 'Read resume receipt');
    await f.idle();
    tree = f.render(actual.PolicyPanel, props);
    assert.notEqual(button(tree, 'Resume policy change').props.disabled, true, 'immutable matching acceptance acknowledges the original request');
    assert.equal(f.calls.filter(call => call.operation === 'catalog.policy_resume').length, 1, 'known acknowledgment never auto-replays');
    f.queue('catalog.policy_resume', failed('admin_review_capacity', { status: 409 }));
    click(tree, 'Resume policy change');
    await f.idle();
    assert.deepEqual(f.calls.at(-1).body, { catalog_policy_resume: { index_id: indexId, operation_id: replacement } }, 'only a new explicit action may submit edited B');
    tree = f.render(actual.PolicyPanel, props);
    const capacityOutcome = nodes(tree).find(node => node?.type?.name === 'Outcome'
      && node.props.state?.result?.envelope?.error?.code === 'admin_review_capacity');
    assert.ok(capacityOutcome, 'capacity refusal remains visible');
    const capacityText = text(capacityOutcome.type(capacityOutcome.props));
    assert.match(capacityText, /not sent|no (new )?operation/i);
    assert.match(capacityText, /receipt/i);
    assert.doesNotMatch(capacityText, /new session|log.?out|sign.?out/i, 'custody is not discarded as a retry shortcut');
    f.queue(known.operation, succeeded(known.operation, known.expect.native));
    click(tree, 'Read resume receipt');
    await f.idle();
    assert.deepEqual(f.calls.at(-1).body, { method: { catalog_policy_resume_receipt: {
      index_id: indexId, operation_id: original,
    } }, review: token }, 'capacity refusal does not discard the previous held receipt');
    assert.deepEqual(f.calls.map(call => call.operation), ['catalog.policy_resume',
      'catalog.policy_resume_receipt', 'catalog.policy_resume_receipt', 'catalog.policy_resume_receipt', 'catalog.policy_resume_receipt', 'catalog.policy_resume', 'catalog.policy_resume_receipt']);
  } finally { f.close(); }
});

test('custody split recovery keeps submitted intent after form edits and cannot mint a new key while unresolved', async () => {
  const f = host();
  try {
    const known = receiptCase('split_already_complete');
    const missing = receiptCase('split_receipt_missing');
    const unknown = receiptCase('split_still_unknown');
    assert.equal(known.expect.resolved, true);
    assert.equal(missing.expect.resolved, false);
    assert.equal(unknown.expect.resolved, false);
    const submitted = known.request.catalog_split_recover.request;
    const original = submitted.operation_id;
    const replacement = '09'.repeat(16);
    let tree = f.render(actual.SplitStatus);
    const setField = (tree, label, value) => {
      const field = nodes(tree).find(node => node?.type?.name === 'Field' && node.props.label === label);
      assert.ok(field, label);
      field.props.onInput(value);
    };
    setField(tree, 'Split operation id', original);
    setField(tree, 'Request digest', submitted.request_digest);
    setField(tree, 'Expected receipt digest', submitted.expected_receipt_digest);
    tree = f.render(actual.SplitStatus);
    f.queue('catalog.split_recover', failed('catalog_apply_outcome_unknown', { handle: true }));
    const confirm = nodes(tree).find(node => node?.type?.name === 'Confirm');
    assert.notEqual(confirm.props.disabled, true);
    confirm.props.onConfirm();
    confirm.props.onConfirm(); // The same captured callback before rerender.
    await f.idle();
    assert.equal(f.calls.filter(call => call.operation === 'catalog.split_recover').length, 1, 'reserve submitted intent synchronously before awaiting');
    assert.deepEqual(f.calls[0].body, known.request);
    tree = f.render(actual.SplitStatus);
    setField(tree, 'Split operation id', replacement);
    setField(tree, 'Request digest', '05'.repeat(32));
    setField(tree, 'Expected receipt digest', '06'.repeat(32));
    nodes(tree).find(node => node?.type === 'select').props.onChange({ currentTarget: { value: 'cancel' } });
    tree = f.render(actual.SplitStatus);
    assert.equal(nodes(tree).find(node => node?.type?.name === 'Confirm').props.disabled, true, 'edited operation/action cannot generate another key');
    for (const result of [missing.expect.native, unknown.expect.native,
      { ...known.expect.native, expired: true },
      { ...known.expect.native, operation_id: replacement },
      { ...known.expect.native, receipt: { ...known.expect.native.receipt,
        request: { ...known.expect.native.receipt.request, expected_receipt_digest: '07'.repeat(32) } } },
    ]) {
      f.queue('catalog.split_recovery_receipt', succeeded('catalog.split_recovery_receipt', result));
      click(tree, 'Read recovery receipt');
      await f.idle();
      assert.deepEqual(f.calls.at(-1).body, { method: { catalog_split_recovery_receipt: { operation_id: original } }, review: token });
      tree = f.render(actual.SplitStatus);
      assert.equal(nodes(tree).find(node => node?.type?.name === 'Confirm').props.disabled, true, 'null/expired/mismatched receipt is not resolution');
    }
    f.queue(known.operation, succeeded(known.operation, known.expect.native));
    click(tree, 'Read recovery receipt');
    await f.idle();
    tree = f.render(actual.SplitStatus);
    const unlocked = nodes(tree).find(node => node?.type?.name === 'Confirm');
    assert.notEqual(unlocked.props.disabled, true, 'matching immutable known acknowledgment unlocks a new explicit action');
    assert.equal(f.calls.filter(call => call.operation === 'catalog.split_recover').length, 1, 'never auto-replay on known acknowledgment');
    f.queue('catalog.split_recover', failed('admin_review_capacity', { status: 409 }));
    unlocked.props.onConfirm();
    await f.idle();
    assert.deepEqual(f.calls.at(-1).body, { catalog_split_recover: { request: {
      operation_id: replacement, request_digest: '05'.repeat(32), expected_receipt_digest: '06'.repeat(32), action: 'cancel',
    } } });
    tree = f.render(actual.SplitStatus);
    f.queue(known.operation, succeeded(known.operation, known.expect.native));
    click(tree, 'Read recovery receipt');
    await f.idle();
    assert.deepEqual(f.calls.at(-1).body, { method: { catalog_split_recovery_receipt: { operation_id: original } }, review: token }, 'capacity refusal retains original receipt custody');
    assert.deepEqual(f.calls.map(call => call.operation), ['catalog.split_recover',
      'catalog.split_recovery_receipt', 'catalog.split_recovery_receipt', 'catalog.split_recovery_receipt',
      'catalog.split_recovery_receipt', 'catalog.split_recovery_receipt', 'catalog.split_recovery_receipt', 'catalog.split_recover', 'catalog.split_recovery_receipt']);
  } finally { f.close(); }
});

test('custody Make readable cannot generate fresh preview identities while original unknown remains unresolved', async () => {
  for (const stage of ['create', 'member']) {
    const f = host();
    try {
      let refreshed = 0;
      const props = { index: { id: indexId, name: 'Docs' }, memberCount: 0, onChanged() { refreshed++; } };
      const page = items => ({ status: 'catalog_page', page: { revision: 3, items, continuation: null } });
      const projectId = `prj_${'09'.repeat(16)}`;
      f.queue('catalog.page', succeeded('catalog.page', page([{ kind: 'source', value: {
        id: `src_${'08'.repeat(16)}`, index_id: indexId, enabled: true,
      } }])));
      f.queue('catalog.page', succeeded('catalog.page', page(stage === 'create' ? [] : [{ kind: 'project', value: {
        id: projectId, name: 'Docs', scope_revision: 1,
      } }])));
      if (stage === 'member') {
        f.queue('catalog.page', succeeded('catalog.page', page([])));
        f.queue('catalog.detail', succeeded('catalog.detail', { status: 'catalog_detail', detail: {
          revision: 3, item: { kind: 'project', value: { id: projectId, name: 'Docs', scope_revision: 1 } },
        } }));
      }
      f.queue('catalog.preview', succeeded('catalog.preview', nativeCases.review.response, token));
      f.queue('catalog.apply', failed('catalog_apply_outcome_unknown'));
      click(f.render(actual.MakeReadable, props), 'Make readable');
      await f.idle();
      let tree = f.render(actual.MakeReadable, props);
      assert.equal(button(tree, 'Make readable').props.disabled, true, 'do not restart the auto-apply chain');
      assert.equal(refreshed, 0);
      const label = stage === 'create' ? 'Read index set receipt' : 'Read membership receipt';
      const known = nativeCases.cases.find(row => row.name === 'catalog_completed').expect.native.receipt;
      for (const result of [
        { status: 'catalog_receipt', receipt: null },
        { status: 'catalog_receipt', receipt: { ...known, request_digest: '07'.repeat(32) } },
      ]) {
        f.queue('catalog.receipt', succeeded('catalog.receipt', result));
        click(tree, label);
        await f.idle();
        tree = f.render(actual.MakeReadable, props);
        assert.equal(button(tree, 'Make readable').props.disabled, true, 'absent/mismatched receipt is not permission to regenerate');
        assert.equal(refreshed, 0);
      }
      f.queue('catalog.receipt', succeeded('catalog.receipt', { status: 'catalog_receipt', receipt: known }));
      click(tree, label);
      await f.idle();
      tree = f.render(actual.MakeReadable, props);
      assert.equal(f.calls.filter(call => call.operation === 'catalog.preview').length, 1, 'receipt never auto-reviews');
      assert.equal(f.calls.filter(call => call.operation === 'catalog.apply').length, 1, 'receipt never auto-applies');
      if (stage === 'create') {
        assert.notEqual(button(tree, 'Make readable').props.disabled, true, 'known CREATE acknowledgment allows an explicit reread/review');
        assert.equal(refreshed, 0);
        f.queue('catalog.page', succeeded('catalog.page', page([])));
        click(tree, 'Make readable');
        await f.idle();
        assert.equal(f.calls.at(-1).operation, 'catalog.page', 'only explicit Go starts the next observation');
      } else {
        assert.equal(refreshed, 1, 'known MEMBERSHIP acknowledgment refreshes the actual library');
        assert.match(text(tree), /The index is readable through/);
        const go = nodes(tree).find(node => node?.type === 'button' && text(node.props.children).trim() === 'Make readable');
        assert.ok(!go || go.props.disabled === true, 'finished membership cannot start another set before refresh');
      }
    } finally { f.close(); }
  }
});

test('review-boundary a known refusal publishes a render after releasing synchronous admission', async () => {
  const f = host();
  try {
    const props = { indexId };
    let tree = f.render(actual.PolicyPanel, props);
    nodes(tree).find(node => node?.type?.name === 'Field' && node.props.label === 'Policy operation id').props.onInput(operationId);
    tree = f.render(actual.PolicyPanel, props);
    f.automaticRenders();
    f.queue('catalog.policy_resume', failed('admin_review_capacity', { status: 409 }));
    click(tree, 'Resume policy change');
    await f.idle();
    // No manual render or user interaction: observe the tree published by the
    // real useCall setter before/after its caller's await continuation.
    assert.notEqual(button(f.latestTree(), 'Resume policy change').props.disabled, true,
      'known no-effect refusal must release pending presentation as well as the admission ref');
    assert.equal(f.calls.length, 1);
  } finally { f.close(); }
});

test('review-boundary a new held request never presents a previous receipt as its own', async () => {
  const f = host();
  try {
    const known = receiptCase('policy_known_accepted');
    const original = known.request.catalog_policy_resume.operation_id;
    const next = '09'.repeat(16);
    const props = { indexId };
    let tree = f.render(actual.PolicyPanel, props);
    const field = current => nodes(current).find(node => node?.type?.name === 'Field' && node.props.label === 'Policy operation id');
    field(tree).props.onInput(original);
    tree = f.render(actual.PolicyPanel, props);
    f.queue('catalog.policy_resume', failed('catalog_apply_outcome_unknown', { handle: true }));
    click(tree, 'Resume policy change');
    await f.idle();
    tree = f.render(actual.PolicyPanel, props);
    f.queue(known.operation, succeeded(known.operation, known.expect.native));
    click(tree, 'Read resume receipt');
    await f.idle();
    tree = f.render(actual.PolicyPanel, props);
    const visibleReceipt = current => nodes(current).filter(node => node?.type?.name === 'Outcome'
      && node.props.state?.phase === 'done' && node.props.state.operation === known.operation);
    assert.equal(visibleReceipt(tree).length, 1, 'legitimate A receipt survives its immutable held-state acknowledgment clone');
    field(tree).props.onInput(next);
    tree = f.render(actual.PolicyPanel, props);
    const unknownB = failed('catalog_apply_outcome_unknown', { handle: true });
    unknownB.body.review = '6e'.repeat(16);
    f.queue('catalog.policy_resume', unknownB);
    click(tree, 'Resume policy change');
    await f.idle();
    tree = f.render(actual.PolicyPanel, props);
    assert.match(text(tree), new RegExp(next), 'the held receipt caption now names B');
    assert.equal(visibleReceipt(tree).length, 0, 'A receipt is not rendered beneath the B receipt caption');
    assert.equal(f.calls.filter(call => call.operation === 'catalog.policy_resume').length, 2);
  } finally { f.close(); }
});

test('review-boundary generated callbacks require the same live session before dispatch and after reply', async () => {
  const f = host();
  try {
    const props = { indexId };
    let tree = f.render(actual.PolicyPanel, props);
    nodes(tree).find(node => node?.type?.name === 'Field' && node.props.label === 'Policy operation id').props.onInput(operationId);
    tree = f.render(actual.PolicyPanel, props);
    const stale = button(tree, 'Resume policy change').props.onClick;
    globalThis.__airkR04Session = { csrf: 'replacement-B' };
    f.queue('catalog.policy_resume', failed('admin_review_capacity', { status: 409 }));
    stale();
    await f.idle();
    assert.equal(f.calls.length, 0, 'A callback cannot submit an A intent using the new B session');
  } finally { f.close(); }

  const g = host();
  try {
    const props = { indexId };
    let release;
    const wait = new Promise(resolve => { release = resolve; });
    let tree = g.render(actual.PolicyPanel, props);
    nodes(tree).find(node => node?.type?.name === 'Field' && node.props.label === 'Policy operation id').props.onInput(operationId);
    tree = g.render(actual.PolicyPanel, props);
    g.queue('catalog.policy_resume', { ...failed('catalog_apply_outcome_unknown', { handle: true }), wait });
    click(tree, 'Resume policy change');
    assert.equal(g.calls.length, 1, 'A was sent before the session changed');
    globalThis.__airkR04Session = { csrf: 'replacement-B' };
    release();
    await g.idle();
    tree = g.render(actual.PolicyPanel, props);
    assert.equal(button(tree, 'Read resume receipt').props.disabled, true,
      'a late A response cannot establish held receipt custody in B before the keyed parent commits');
    assert.doesNotMatch(text(tree), /Receipt for policy operation/);
  } finally { g.close(); }
});

// A fresh hook host discards all component state/refs, as a keyed remount does.
// Reuse the exact session object: a new default host session would hide this
// custody boundary. These are actual callbacks, not DOM lifecycle qualification.
function setRecoveryFields(tree, kind, operation) {
  const set = (label, value) => {
    const field = nodes(tree).find(node => node?.type?.name === 'Field' && node.props.label === label);
    assert.ok(field, label);
    field.props.onInput(value);
  };
  set(kind === 'policy' ? 'Policy operation id' : 'Split operation id', operation);
  if (kind === 'split') {
    const submitted = receiptCase('split_already_complete').request.catalog_split_recover.request;
    set('Request digest', submitted.request_digest);
    set('Expected receipt digest', submitted.expected_receipt_digest);
  }
}
function generatingControl(tree, kind) {
  return kind === 'policy' ? button(tree, 'Resume policy change')
    : nodes(tree).find(node => node?.type?.name === 'Confirm');
}
function invokeGenerating(tree, kind) {
  const control = generatingControl(tree, kind);
  assert.ok(control);
  if (kind === 'policy') control.props.onClick();
  else control.props.onConfirm();
}
async function waitForCalls(f, count) {
  const deadline = Date.now() + 5000;
  while (f.calls.length < count && Date.now() < deadline) await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.calls.length, count, 'observed native admission under deadline');
}

for (const kind of ['policy', 'split']) {
  for (const phase of ['unknown', 'pending']) {
    test(`remount ${kind} ${phase} retains original intent and receipt in the same session`, async () => {
      const session = { csrf: 'shared-remount-session' };
      let f = host(session);
      let release;
      try {
        const known = receiptCase(kind === 'policy' ? 'policy_known_accepted' : 'split_already_complete');
        const method = kind === 'policy' ? known.request.catalog_policy_resume : known.request.catalog_split_recover.request;
        const operation = kind === 'policy' ? 'catalog.policy_resume' : 'catalog.split_recover';
        const component = kind === 'policy' ? actual.PolicyPanel : actual.SplitStatus;
        const props = { indexId };
        const replacement = '09'.repeat(16);
        let tree = f.render(component, props);
        setRecoveryFields(tree, kind, method.operation_id);
        tree = f.render(component, props);
        const wait = phase === 'pending' ? new Promise(resolve => { release = resolve; }) : undefined;
        f.queue(operation, { ...failed('catalog_apply_outcome_unknown', { handle: true }), wait });
        invokeGenerating(tree, kind);
        assert.equal(f.calls.length, 1);
        assert.deepEqual(f.calls[0].body, known.request);
        if (phase === 'unknown') await f.idle();
        const originalHost = f;
        f.close();
        f = host(session);
        tree = f.render(component, props);
        setRecoveryFields(tree, kind, replacement);
        tree = f.render(component, props);
        assert.equal(generatingControl(tree, kind).props.disabled, true, 'remount cannot discard pending/unknown admission');
        invokeGenerating(tree, kind); // Exercise the guard even without the disabled UI.
        assert.equal(f.calls.length, 0, 'edited successor intent cannot mint a new request');
        if (release) {
          release();
          await originalHost.idle();
          tree = f.render(component, props);
        }
        const label = kind === 'policy' ? 'Read resume receipt' : 'Read recovery receipt';
        f.queue(known.operation, succeeded(known.operation, known.expect.native));
        click(tree, label);
        await f.idle();
        const receiptMethod = kind === 'policy'
          ? { catalog_policy_resume_receipt: { index_id: indexId, operation_id: method.operation_id } }
          : { catalog_split_recovery_receipt: { operation_id: method.operation_id } };
        assert.deepEqual(f.calls, [{ operation: known.operation, body: { method: receiptMethod, review: token } }],
          'the successor reads original A with its original handle, never edited B');
        tree = f.render(component, props);
        assert.notEqual(generatingControl(tree, kind).props.disabled, true, 'matching immutable receipt resolves original custody');
        f.close();
        f = host({ csrf: 'separate-session' });
        tree = f.render(component, props);
        setRecoveryFields(tree, kind, replacement);
        tree = f.render(component, props);
        assert.notEqual(generatingControl(tree, kind).props.disabled, true, 'another session has independent admission');
        assert.equal(button(tree, label).props.disabled, true, 'another session cannot read A through the old handle');
        assert.equal(f.calls.length, 0);
      } finally { release?.(); f.close(); }
    });
  }
}

function queueReadableApply(f, stage, applyReply) {
  const page = items => ({ status: 'catalog_page', page: { revision: 3, items, continuation: null } });
  const projectId = `prj_${'09'.repeat(16)}`;
  f.queue('catalog.page', succeeded('catalog.page', page([{ kind: 'source', value: {
    id: `src_${'08'.repeat(16)}`, index_id: indexId, enabled: true,
  } }])));
  f.queue('catalog.page', succeeded('catalog.page', page(stage === 'create' ? [] : [{ kind: 'project', value: {
    id: projectId, name: 'Docs', scope_revision: 1,
  } }])));
  if (stage === 'member') {
    f.queue('catalog.page', succeeded('catalog.page', page([])));
    f.queue('catalog.detail', succeeded('catalog.detail', { status: 'catalog_detail', detail: {
      revision: 3, item: { kind: 'project', value: { id: projectId, name: 'Docs', scope_revision: 1 } },
    } }));
  }
  f.queue('catalog.preview', succeeded('catalog.preview', nativeCases.review.response, token));
  f.queue('catalog.apply', applyReply);
}

for (const stage of ['create', 'member']) {
  for (const phase of ['unknown', 'pending']) {
    test(`remount Make readable ${stage} ${phase} retains reviewed identity and blocks a new preview`, async () => {
      const session = { csrf: 'shared-readable-remount' };
      let f = host(session);
      let release;
      try {
        const props = { index: { id: indexId, name: 'Docs' }, memberCount: 0,
          onChanged() { assert.fail('unresolved remount must not report readable'); } };
        const wait = phase === 'pending' ? new Promise(resolve => { release = resolve; }) : undefined;
        queueReadableApply(f, stage, { ...failed('catalog_apply_outcome_unknown'), wait });
        click(f.render(actual.MakeReadable, props), 'Make readable');
        await waitForCalls(f, stage === 'create' ? 4 : 6);
        assert.equal(f.calls.at(-1).operation, 'catalog.apply');
        assert.equal(f.calls.at(-1).body.review, token);
        if (phase === 'unknown') await f.idle();
        const originalHost = f;
        f.close();
        f = host(session);
        const renamed = { ...props, index: { ...props.index, name: 'Edited name after remount' } };
        let tree = f.render(actual.MakeReadable, renamed);
        assert.equal(button(tree, 'Make readable').props.disabled, true, 'same index remount cannot discard pending/unknown chain');
        button(tree, 'Make readable').props.onClick();
        assert.equal(f.calls.length, 0, 'changed name does not start catalog reads or mint another preview');
        if (release) {
          release();
          await originalHost.idle();
          tree = f.render(actual.MakeReadable, renamed);
        }
        const label = stage === 'create' ? 'Read index set receipt' : 'Read membership receipt';
        f.queue('catalog.receipt', succeeded('catalog.receipt', { status: 'catalog_receipt', receipt: null }));
        click(tree, label);
        await f.idle();
        assert.deepEqual(f.calls, [{ operation: 'catalog.receipt', body: { method: { catalog_receipt: {} }, review: token } }],
          'remount retains original apply review, not a newly generated identity');
        tree = f.render(actual.MakeReadable, renamed);
        assert.equal(button(tree, 'Make readable').props.disabled, true, 'missing original receipt leaves remounted chain locked');
        f.close();
        f = host({ csrf: 'separate-readable-session' });
        tree = f.render(actual.MakeReadable, renamed);
        assert.notEqual(button(tree, 'Make readable').props.disabled, true, 'new session has independent admission');
        assert.equal(nodes(tree).some(node => node?.type === 'button' && text(node.props.children).trim() === label), false,
          'new session cannot inherit the old receipt handle');
        assert.equal(f.calls.length, 0);
      } finally { release?.(); f.close(); }
    });
  }
}

for (const kind of ['policy', 'split', 'readable']) {
  test(`remount subscription ${kind} publishes a pending predecessor reply to the mounted successor`, async () => {
    const session = { csrf: 'subscription-remount-session' };
    let f = host(session);
    let release;
    try {
      const component = kind === 'policy' ? actual.PolicyPanel : kind === 'split' ? actual.SplitStatus : actual.MakeReadable;
      const props = kind === 'readable' ? { index: { id: indexId, name: 'Docs' }, memberCount: 0,
        onChanged() { assert.fail('unknown predecessor cannot complete the workflow'); } } : { indexId };
      const wait = new Promise(resolve => { release = resolve; });
      let tree = f.render(component, props);
      if (kind === 'readable') {
        queueReadableApply(f, 'create', { ...failed('catalog_apply_outcome_unknown'), wait });
        click(tree, 'Make readable');
        await waitForCalls(f, 4);
      } else {
        setRecoveryFields(tree, kind, operationId);
        tree = f.render(component, props);
        f.queue(kind === 'policy' ? 'catalog.policy_resume' : 'catalog.split_recover',
          { ...failed('catalog_apply_outcome_unknown', { handle: true }), wait });
        invokeGenerating(tree, kind);
        assert.equal(f.calls.length, 1);
      }
      const originalHost = f;
      f.close();
      f = host(session);
      f.automaticRenders();
      f.render(component, props);
      release();
      await originalHost.idle();
      await f.idle();
      // No explicit render after release: actual subscriptions must notify B.
      tree = f.latestTree();
      const label = kind === 'policy' ? 'Read resume receipt' : kind === 'split' ? 'Read recovery receipt' : 'Read index set receipt';
      assert.notEqual(button(tree, label).props.disabled, true, 'the successor publishes the original recovery handle');
      const receiptOperation = kind === 'policy' ? 'catalog.policy_resume_receipt'
        : kind === 'split' ? 'catalog.split_recovery_receipt' : 'catalog.receipt';
      f.queue(receiptOperation, succeeded(receiptOperation, kind === 'readable'
        ? { status: 'catalog_receipt', receipt: null }
        : receiptCase(kind === 'policy' ? 'policy_receipt_missing' : 'split_receipt_missing').expect.native));
      click(tree, label);
      await f.idle();
      assert.equal(f.calls.length, 1, 'notification never starts a new generating mutation');
      assert.equal(f.calls[0].body.review, token);
      assert.equal(f.calls[0].operation, receiptOperation);
    } finally { release?.(); f.close(); }
  });
}

for (const kind of ['policy', 'split']) {
  test(`remount settled ${kind} custody is reclaimed after its final subscriber closes`, async () => {
    const session = { csrf: 'settled-remount-session' };
    let f = host(session);
    try {
      const known = receiptCase(kind === 'policy' ? 'policy_known_accepted' : 'split_already_complete');
      const method = kind === 'policy' ? known.request.catalog_policy_resume : known.request.catalog_split_recover.request;
      const component = kind === 'policy' ? actual.PolicyPanel : actual.SplitStatus;
      const props = { indexId };
      let tree = f.render(component, props);
      setRecoveryFields(tree, kind, method.operation_id);
      tree = f.render(component, props);
      f.queue(kind === 'policy' ? 'catalog.policy_resume' : 'catalog.split_recover', failed('catalog_apply_outcome_unknown', { handle: true }));
      invokeGenerating(tree, kind);
      await f.idle();
      tree = f.render(component, props);
      const label = kind === 'policy' ? 'Read resume receipt' : 'Read recovery receipt';
      f.queue(known.operation, succeeded(known.operation, known.expect.native));
      click(tree, label);
      await f.idle();
      tree = f.render(component, props);
      assert.notEqual(generatingControl(tree, kind).props.disabled, true, 'matching immutable receipt settles the held request');
      f.close();
      f = host(session);
      tree = f.render(component, props);
      setRecoveryFields(tree, kind, '09'.repeat(16));
      tree = f.render(component, props);
      assert.notEqual(generatingControl(tree, kind).props.disabled, true);
      assert.equal(button(tree, label).props.disabled, true, 'settled handle is reclaimed after last subscriber cleanup');
      assert.doesNotMatch(text(tree), /Receipt for (policy|split) operation/);
      assert.equal(f.calls.length, 0, 'cleanup/remount never replays the settled mutation');
    } finally { f.close(); }
  });
}

for (const kind of ['policy', 'readable']) {
  test(`receipt ordering ${kind} late missing result cannot replace the newer known acknowledgment`, async () => {
    const f = host();
    let release;
    try {
      const component = kind === 'policy' ? actual.PolicyPanel : actual.MakeReadable;
      const props = kind === 'policy' ? { indexId } : { index: { id: indexId, name: 'Docs' }, memberCount: 0,
        onChanged() { assert.fail('create receipt alone never completes Make readable'); } };
      let tree = f.render(component, props);
      const known = kind === 'policy' ? receiptCase('policy_known_accepted').expect.native
        : { status: 'catalog_receipt', receipt: nativeCases.cases.find(row => row.name === 'catalog_completed').expect.native.receipt };
      if (kind === 'policy') {
        setRecoveryFields(tree, 'policy', receiptCase('policy_known_accepted').request.catalog_policy_resume.operation_id);
        tree = f.render(component, props);
        f.queue('catalog.policy_resume', failed('catalog_apply_outcome_unknown', { handle: true }));
        invokeGenerating(tree, 'policy');
      } else {
        queueReadableApply(f, 'create', failed('catalog_apply_outcome_unknown'));
        click(tree, 'Make readable');
      }
      await f.idle();
      tree = f.render(component, props);
      const operation = kind === 'policy' ? 'catalog.policy_resume_receipt' : 'catalog.receipt';
      const label = kind === 'policy' ? 'Read resume receipt' : 'Read index set receipt';
      const missing = kind === 'policy' ? receiptCase('policy_receipt_missing').expect.native
        : { status: 'catalog_receipt', receipt: null };
      const wait = new Promise(resolve => { release = resolve; });
      f.queue(operation, { ...succeeded(operation, missing), wait });
      click(tree, label);
      f.queue(operation, succeeded(operation, known));
      click(tree, label);
      await f.idle();
      release();
      await f.idle();
      tree = f.render(component, props);
      const visible = nodes(tree).filter(node => node?.type?.name === 'Outcome'
        && node.props.state?.phase === 'done' && node.props.state.operation === operation);
      assert.equal(visible.length, 1, 'one current receipt outcome');
      assert.deepEqual(visible[0].props.state.result.envelope.result, known,
        'the older missing result is returned only to its original caller');
      const control = kind === 'policy' ? generatingControl(tree, 'policy') : button(tree, 'Make readable');
      assert.notEqual(control.props.disabled, true, 'late missing receipt cannot undo the newer immutable acknowledgment');
      assert.equal(f.calls.filter(call => call.operation === operation).length, 2);
      assert.equal(f.calls.filter(call => call.operation === (kind === 'policy' ? 'catalog.policy_resume' : 'catalog.preview')).length, 1,
        'out-of-order receipt completion never generates another mutation');
    } finally { release?.(); f.close(); }
  });
}
