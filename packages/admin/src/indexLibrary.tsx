// SPDX-License-Identifier: Apache-2.0
// Index library (split plan §7.3 "Index library and index sets"; U6, U97, U98).
//
// Index management lives only in these pages. Every change follows the
// service's own review: a draft or preview read (a "generate" row that answers
// a session-held `review` token), the complete review shown as text, then the
// apply as a ReviewedCall carrying that token and, for destructive catalog
// operations, the target's display name typed by the user (contracts §4.3b).
// Fragment routes (`#index-library?root=|index=|set=`) only prefill; they
// never start a call that changes anything.
import type { ComponentChildren, JSX } from 'preact';
import { useEffect, useState } from 'preact/hooks';
import { reviewedCall, reviewOf } from './api';
import {
  body,
  isCatalogId,
  isDigest,
  isIncarnation,
  newProjectId,
  OPS,
  type CatalogKind,
  type CatalogRequestBody,
  type Operation,
  type ScopeAtom,
  type SourceMutationSelection,
} from './operations';
import { Confirm, Field, JsonBlock, list, Outcome, Panel, resultOf, Text, useCall, type CallState } from './render';

// ---------------------------------------------------------------------------
// Result helpers. Native results are read defensively and always also shown
// complete through Outcome; the readable summaries never replace them.

type Rec = Record<string, unknown>;

const rec = (value: unknown): Rec | null =>
  typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as Rec) : null;
const str = (value: unknown): string | null => (typeof value === 'string' ? value : null);
const num = (value: unknown): number | null => (typeof value === 'number' && Number.isFinite(value) ? value : null);
const arr = (value: unknown): unknown[] => (Array.isArray(value) ? value : []);
const okResult = (state: CallState): Rec | null => rec(resultOf(state));
const reviewIn = (state: CallState): string | null => (state.phase === 'done' ? reviewOf(state.result) : null);
const failedCode = (state: CallState): string | null =>
  state.phase === 'done' && state.result.kind === 'failed' ? state.result.envelope.error.code : null;
const short = (id: string): string => id.slice(0, 12);
const text = (value: unknown): string => (typeof value === 'string' ? value : JSON.stringify(value) ?? 'undefined');

/** Finds the first string field named `key` anywhere in a value. */
function findString(value: unknown, key: string, test: (v: string) => boolean): string | null {
  if (Array.isArray(value)) {
    for (const item of value) {
      const found = findString(item, key, test);
      if (found !== null) return found;
    }
  } else if (typeof value === 'object' && value !== null) {
    for (const [name, item] of Object.entries(value)) {
      if (name === key && typeof item === 'string' && test(item)) return item;
      const found = findString(item, key, test);
      if (found !== null) return found;
    }
  }
  return null;
}

/** Same rule as airkctl's `collection_from`: lowercase letters, digits and dashes. */
export function collectionFrom(name: string): string {
  let out = '';
  for (const character of name) {
    if (/^[A-Za-z0-9]$/.test(character)) out += character.toLowerCase();
    else if (out !== '' && !out.endsWith('-')) out += '-';
  }
  const trimmed = out.replace(/-+$/, '').slice(0, 64);
  return trimmed === '' ? 'index' : trimmed;
}

/** The last path component of a Windows or POSIX folder path. */
function folderName(path: string): string {
  const parts = path.split(/[\\/]+/).filter((part) => part.length > 0);
  const last = parts[parts.length - 1] ?? '';
  return /^[A-Za-z]:$/.test(last) ? '' : last;
}

/** `ALIAS=PATH` lines; null marks a line that is neither empty nor valid. */
function parseAdditional(value: string): ({ alias: string; path: string } | null)[] {
  return value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0)
    .map((line) => {
      const at = line.indexOf('=');
      if (at <= 0 || at === line.length - 1) return null;
      return { alias: line.slice(0, at).trim(), path: line.slice(at + 1).trim() };
    });
}

/** A non-negative safe integer from decimal text. */
function parseCount(value: string): number | null {
  if (!/^\d+$/.test(value)) return null;
  const n = Number(value);
  return Number.isSafeInteger(n) ? n : null;
}

function parseJsonArray(value: string): unknown[] | null {
  try {
    const parsed: unknown = JSON.parse(value);
    return Array.isArray(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------------------
// Catalog items (CatalogItem: adjacently tagged `{kind, value}`).

type Item = { kind: string; value: Rec };

function itemOf(value: unknown): Item | null {
  const r = rec(value);
  const kind = str(r?.kind);
  const inner = rec(r?.value);
  return kind !== null && inner !== null ? { kind, value: inner } : null;
}

type Described = { id: string; name: string; facts: string[]; retired: boolean };

function atomText(atom: unknown): string {
  const a = rec(atom);
  if (a === null) return text(atom);
  const path = str(a.path) ?? '';
  return `${text(a.source_root_id)} ${text(a.kind)} ${path === '' ? '(whole root)' : path}`;
}

function describe(item: Item): Described {
  const v = item.value;
  switch (item.kind) {
    case 'index': {
      const backing = rec(v.backing);
      return {
        id: text(v.id),
        name: text(v.name),
        retired: v.tombstone === true,
        facts: [
          v.tombstone === true ? 'retired (tombstone)' : 'active',
          `definition revision ${text(v.definition_revision)}`,
          `source revision ${text(v.source_revision)}`,
          `source import ${text(v.source_import)}`,
          v.policy === null || v.policy === undefined ? 'no policy' : `policy ${text(v.policy)}`,
          `root ${text(backing?.root)}`,
          `data root ${text(backing?.data_root)}`,
          `collection ${text(backing?.collection)}`,
          `binding ${text(backing?.binding)}`,
        ],
      };
    }
    case 'project':
      return {
        id: text(v.id),
        name: text(v.name),
        retired: false,
        facts: [
          `definition revision ${text(v.definition_revision)}`,
          `scope revision ${text(v.scope_revision)}`,
          `defaults revision ${text(v.defaults_revision)}`,
          `default indexes: ${arr(v.default_indexes).map(text).join(', ') || 'none'}`,
        ],
      };
    case 'membership':
      return {
        id: `${text(v.project_id)}/${text(v.index_id)}`,
        name: '',
        retired: false,
        facts: [
          `index set ${text(v.project_id)}`,
          `index ${text(v.index_id)}`,
          `revision ${text(v.revision)}`,
          `scope: ${arr(v.scope).map(atomText).join('; ') || 'none (no authority)'}`,
        ],
      };
    case 'source':
      return {
        id: text(v.id),
        name: text(v.canonical_path),
        retired: v.enabled !== true,
        facts: [
          v.owner === true ? 'primary root' : 'additional root',
          v.enabled === true ? 'enabled' : 'disabled',
          `aliases: ${arr(v.aliases).map(text).join(', ') || 'none'}`,
          `index ${text(v.index_id)}`,
        ],
      };
    case 'grant': {
      const grant = rec(v.grant);
      return {
        id: text(grant?.id),
        name: '',
        retired: grant?.revoked === true,
        facts: [
          grant?.revoked === true ? 'revoked' : 'active',
          `revision ${text(grant?.revision)}`,
          `host ${text(grant?.host)}`,
          `${arr(grant?.memberships).length} memberships`,
          `provenance ${text(v.provenance)}`,
        ],
      };
    }
    default:
      return { id: text(v.id), name: text(v.name ?? ''), retired: false, facts: [text(v)] };
  }
}

function ItemView(props: { item: Item; children?: ComponentChildren }): JSX.Element {
  const d = describe(props.item);
  return (
    <li class={d.retired ? 'item retired' : 'item'}>
      <div>
        {d.name !== '' ? (
          <strong>
            <Text value={d.name} />
          </strong>
        ) : null}{' '}
        <code>
          <Text value={d.id} />
        </code>
      </div>
      <div class="facts">
        {d.facts.map((fact) => (
          <span class="fact">
            <Text value={fact} />
          </span>
        ))}
      </div>
      {props.children !== undefined && props.children !== null ? <div class="actions">{props.children}</div> : null}
    </li>
  );
}

// ---------------------------------------------------------------------------
// Catalog pages with continuation.

function CatalogList(props: {
  kind: CatalogKind;
  indexId?: string | null;
  projectId?: string | null;
  reload: number;
  onRevision: (revision: number) => void;
  onItems?: (items: Item[]) => void;
  actions?: (item: Item) => ComponentChildren;
  empty: string;
}): JSX.Element {
  const [state, run] = useCall();
  const [items, setItems] = useState<Item[]>([]);
  const [cursor, setCursor] = useState<unknown>(null);
  const [header, setHeader] = useState<{ namespace: string; revision: number } | null>(null);
  const filter = { indexId: props.indexId ?? null, projectId: props.projectId ?? null };
  const load = async (after: unknown): Promise<void> => {
    const expected = after === null ? null : num(rec(after)?.revision);
    const result = await run(OPS.catalogPage, body.catalogPage(props.kind, filter, after, expected));
    const page = result.kind === 'ok' ? rec(rec(result.envelope.result)?.page) : null;
    if (page === null) return;
    const got = arr(page.items)
      .map(itemOf)
      .filter((item): item is Item => item !== null);
    const merged = after === null ? got : [...items, ...got];
    setItems(merged);
    props.onItems?.(merged);
    setCursor(page.continuation ?? null);
    const revision = num(page.revision);
    if (revision !== null) {
      setHeader({ namespace: text(page.namespace), revision });
      props.onRevision(revision);
    }
  };
  useEffect(() => {
    void load(null);
  }, [props.kind, props.indexId, props.projectId, props.reload]);
  const failed = state.phase === 'done' && state.result.kind !== 'ok';
  return (
    <div class="catalog-list">
      <div class="row">
        <button type="button" class="secondary" onClick={() => void load(null)}>
          Reload
        </button>
        <button type="button" class="secondary" disabled={cursor === null} onClick={() => void load(cursor)}>
          Load more
        </button>
        <span class="muted">
          {items.length} shown{cursor !== null ? ', more available' : ''}
          {header !== null ? ` · catalog revision ${header.revision} · namespace ${header.namespace}` : ''}
        </span>
      </div>
      {items.length === 0 && state.phase === 'done' && !failed ? <p class="muted">{props.empty}</p> : null}
      <ul class="items">
        {items.map((item) => (
          <ItemView key={`${item.kind}:${describe(item).id}`} item={item}>
            {props.actions?.(item)}
          </ItemView>
        ))}
      </ul>
      <details open={failed}>
        <summary>Last page (complete result)</summary>
        <Outcome state={state} />
      </details>
      {items.length > 0 ? (
        <details>
          <summary>All shown rows (JSON)</summary>
          <JsonBlock name={`airk-catalog-${props.kind}.json`} value={items} />
        </details>
      ) : null}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Reviewed catalog changes: preview or draft → catalog.apply → catalog.receipt.

type Staged = { request: unknown; previewDigest: string };

/** A failed reviewed call: what to do next (contracts §4.3b codes). */
function ReviewHint(props: { state: CallState; confirm: string | null; onReviewAgain: () => void }): JSX.Element | null {
  const code = failedCode(props.state);
  if (code === 'admin_review_unknown') {
    return (
      <div class="actions">
        <p class="error">Review expired; run the review again.</p>
        <button type="button" onClick={props.onReviewAgain}>
          Run the review again
        </button>
      </div>
    );
  }
  if (code === 'admin_confirmation_mismatch') {
    return (
      <p class="error">
        The confirmation must equal the target's current display name exactly
        {props.confirm !== null ? (
          <>
            {' '}
            (this page read <code>{props.confirm}</code>)
          </>
        ) : null}
        . The name may have changed since this page read it: reload the details and run the review again.
      </p>
    );
  }
  return null;
}

function ApplyCatalog(props: {
  token: string;
  staged: Staged;
  confirm: string | null;
  label: string;
  danger?: boolean;
  onApplied?: (result: Rec) => void;
  onReviewAgain: () => void;
}): JSX.Element {
  const [apply, runApply] = useCall();
  const [receipt, runReceipt] = useCall();
  const go = async (): Promise<void> => {
    const method = body.catalogApply(props.staged.request, props.staged.previewDigest);
    const result = await runApply(OPS.catalogApply, reviewedCall(method, props.token, props.confirm));
    if (result.kind === 'ok') props.onApplied?.(rec(result.envelope.result) ?? {});
  };
  const applied = apply.phase === 'done' && apply.result.kind === 'ok';
  return (
    <div class="apply">
      <p class="muted">Applying sends exactly the reviewed request and preview digest shown above.</p>
      {props.confirm !== null ? (
        <Confirm phrase={props.confirm} action={props.label} danger={props.danger ?? true} onConfirm={() => void go()} />
      ) : (
        <button type="button" disabled={apply.phase === 'pending'} onClick={() => void go()}>
          {props.label}
        </button>
      )}
      <Outcome state={apply} />
      <ReviewHint state={apply} confirm={props.confirm} onReviewAgain={props.onReviewAgain} />
      {applied || failedCode(apply) === 'service_request_outcome_unknown' ? (
        <div class="row">
          <button
            type="button"
            class="secondary"
            onClick={() => void runReceipt(OPS.catalogReceipt, reviewedCall(body.catalogReceipt(), props.token))}
          >
            Read receipt
          </button>
        </div>
      ) : null}
      <Outcome state={receipt} />
      <ReviewHint state={receipt} confirm={null} onReviewAgain={props.onReviewAgain} />
    </div>
  );
}

/**
 * One reviewed catalog change. `makeDraft` returns the generate-row method body
 * (catalog.preview, catalog.source_draft or catalog.grant_draft), or null when
 * the form is incomplete; `extract` finds the request and preview digest the
 * apply must repeat.
 */
function ReviewedChange(props: {
  reviewLabel: string;
  disabled?: boolean;
  draftOperation: Operation;
  makeDraft: () => unknown;
  extract: (result: Rec, sent: unknown) => Staged | null;
  confirm: string | null;
  applyLabel: string;
  danger?: boolean;
  onApplied?: (result: Rec) => void;
}): JSX.Element {
  const [draft, runDraft] = useCall();
  const [sent, setSent] = useState<unknown>(null);
  const review = async (): Promise<void> => {
    const method = props.makeDraft();
    if (method === null) return;
    setSent(method);
    await runDraft(props.draftOperation, method);
  };
  const token = reviewIn(draft);
  const result = okResult(draft);
  const staged = result !== null && token !== null ? props.extract(result, sent) : null;
  return (
    <div class="reviewed">
      <button type="button" class="secondary" disabled={props.disabled ?? false} onClick={() => void review()}>
        {props.reviewLabel}
      </button>
      <Outcome state={draft} />
      {result !== null && token === null ? (
        <p class="error">The service answered without a review token, so this change cannot be applied here.</p>
      ) : null}
      {result !== null && token !== null && staged === null ? (
        <p class="error">The review has no request and preview digest to apply.</p>
      ) : null}
      {staged !== null && token !== null ? (
        <ApplyCatalog
          key={token}
          token={token}
          staged={staged}
          confirm={props.confirm}
          label={props.applyLabel}
          danger={props.danger ?? props.confirm !== null}
          onApplied={props.onApplied}
          onReviewAgain={() => void review()}
        />
      ) : null}
    </div>
  );
}

/** catalog.preview: the apply repeats the previewed request and its digest. */
function fromPreview(result: Rec, sent: unknown): Staged | null {
  const request = rec(rec(sent)?.catalog_preview)?.request;
  const digest = str(result.preview_digest);
  return request !== undefined && digest !== null ? { request, previewDigest: digest } : null;
}

/** catalog.source_draft / catalog.grant_draft: the draft's request (identity removed) and digest. */
function fromDraft(result: Rec): Staged | null {
  const draft = rec(result.draft);
  const digest = str(draft?.preview_digest);
  return draft !== null && draft.request !== undefined && digest !== null
    ? { request: draft.request, previewDigest: digest }
    : null;
}

function previewOf(revision: number | null, operation: CatalogRequestBody['operation']): unknown {
  if (revision === null) return null;
  return body.catalogPreview({ expected_catalog_revision: revision, operation });
}

// ---------------------------------------------------------------------------
// Set up an index: index_setup_draft → full draft → index_setup_apply → receipt.

const DATA_ROOT_HINT =
  'On Windows the service keeps index data under %LOCALAPPDATA%\\AIRK Service\\data\\indexes by default; type the folder this index should use.';

function DraftSummary(props: { draft: Rec }): JSX.Element {
  const d = props.draft;
  const request = rec(d.request);
  const registration = rec(request?.registration);
  return (
    <dl class="summary">
      <dt>Name</dt>
      <dd>
        <Text value={text(request?.name)} />
      </dd>
      <dt>Primary root</dt>
      <dd>
        <Text value={text(registration?.root)} />
      </dd>
      <dt>Data root</dt>
      <dd>
        <Text value={text(registration?.data_root)} />
      </dd>
      <dt>Collection</dt>
      <dd>
        <Text value={text(registration?.collection)} />
      </dd>
      <dt>Binding</dt>
      <dd>
        <code>
          <Text value={text(registration?.binding)} />
        </code>
      </dd>
      <dt>Sources</dt>
      <dd>
        <ul>
          {arr(request?.sources).map((source) => {
            const s = rec(source);
            return (
              <li>
                <Text
                  value={`${text(s?.canonical_path)} (${s?.owner === true ? 'primary' : 'additional'}; aliases: ${
                    arr(s?.aliases).map(text).join(', ') || 'none'
                  })`}
                />
              </li>
            );
          })}
        </ul>
      </dd>
      <dt>Watch for edits</dt>
      <dd>{request?.setup_watch === true ? 'on' : 'off'}</dd>
      <dt>Storage action</dt>
      <dd>
        <Text value={text(d.storage_action)} />
      </dd>
      <dt>Catalog revision</dt>
      <dd>{text(request?.expected_catalog_revision)}</dd>
      <dt>Review digest</dt>
      <dd>
        <code>{text(d.review_digest)}</code>
      </dd>
    </dl>
  );
}

function ApplySetup(props: {
  token: string;
  draft: Rec;
  onRefresh: () => void;
  onActivity: () => void;
  onReviewAgain: () => void;
}): JSX.Element {
  const [apply, runApply] = useCall();
  const [receipt, runReceipt] = useCall();
  const applied = okResult(apply);
  const indexId = applied !== null ? findString(applied, 'index_id', (v) => isCatalogId('idx_', v)) : null;
  return (
    <div class="apply">
      <button
        type="button"
        disabled={apply.phase === 'pending'}
        onClick={() =>
          void runApply(OPS.catalogIndexSetupApply, reviewedCall(body.indexSetupApply(props.draft), props.token))
        }
      >
        Create index
      </button>
      <Outcome state={apply} />
      <ReviewHint state={apply} confirm={null} onReviewAgain={props.onReviewAgain} />
      {applied !== null || failedCode(apply) === 'service_request_outcome_unknown' ? (
        <div class="row">
          <button
            type="button"
            class="secondary"
            onClick={() =>
              void runReceipt(OPS.catalogIndexSetupReceipt, reviewedCall(body.indexSetupReceipt(), props.token))
            }
          >
            Read receipt
          </button>
          <button type="button" class="secondary" onClick={props.onRefresh}>
            Refresh
          </button>
          <button type="button" class="secondary" onClick={props.onActivity}>
            Open Activity
          </button>
        </div>
      ) : null}
      {applied !== null ? (
        <p>
          Index {indexId !== null ? <code>{indexId}</code> : null} was accepted. Indexing runs as a service job;
          follow it on the Activity page (Events), then refresh this library.
        </p>
      ) : null}
      <Outcome state={receipt} />
      <ReviewHint state={receipt} confirm={null} onReviewAgain={props.onReviewAgain} />
    </div>
  );
}

function SetupIndex(props: {
  revision: number | null;
  root: string | null;
  seq: number;
  onRefresh: () => void;
  onActivity: () => void;
}): JSX.Element {
  const [name, setName] = useState('');
  const [primary, setPrimary] = useState('');
  const [dataRoot, setDataRoot] = useState('');
  const [collection, setCollection] = useState('');
  const [additional, setAdditional] = useState('');
  const [watch, setWatch] = useState(true);
  const [nameTouched, setNameTouched] = useState(false);
  const [collectionTouched, setCollectionTouched] = useState(false);
  const [draft, runDraft] = useCall();

  const applyRoot = (root: string, force: boolean): void => {
    setPrimary(root);
    const folder = folderName(root);
    if (force || !nameTouched) setName(folder);
    if (force || !collectionTouched) setCollection(collectionFrom(folder));
    if (force) {
      setNameTouched(false);
      setCollectionTouched(false);
    }
  };
  useEffect(() => {
    if (props.root !== null && props.root !== '') applyRoot(props.root, true);
  }, [props.seq]);

  const extra = parseAdditional(additional);
  const extraValid = extra.every((entry) => entry !== null);
  const collectionValid = /^[A-Za-z0-9._-]{1,128}$/.test(collection) && collection !== '.' && collection !== '..';
  const ready =
    props.revision !== null &&
    name.trim() !== '' &&
    primary.trim() !== '' &&
    dataRoot.trim() !== '' &&
    collectionValid &&
    extraValid;
  const review = (): void => {
    if (!ready || props.revision === null) return;
    void runDraft(
      OPS.catalogIndexSetupDraft,
      body.indexSetupDraft({
        expected_catalog_revision: props.revision,
        name: name.trim(),
        primary_root: primary.trim(),
        data_root: dataRoot.trim(),
        collection,
        additional_roots: extra.filter((entry): entry is { alias: string; path: string } => entry !== null),
        setup_watch: watch,
      }),
    );
  };
  const result = okResult(draft);
  const draftValue = rec(result?.draft);
  const token = reviewIn(draft);
  return (
    <>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          review();
        }}
      >
        <div class="row">
          <Field
            label="Primary root (the folder to index)"
            wide
            value={primary}
            onInput={(value) => applyRoot(value, false)}
          />
        </div>
        <div class="row">
          <Field
            label="Name"
            value={name}
            onInput={(value) => {
              setName(value);
              setNameTouched(true);
            }}
          />
          <Field
            label="Collection"
            value={collection}
            invalid={collection !== '' && !collectionValid}
            onInput={(value) => {
              setCollection(value);
              setCollectionTouched(true);
            }}
          />
        </div>
        <div class="row">
          <Field label="Data root" wide value={dataRoot} onInput={setDataRoot} />
        </div>
        <p class="muted">{DATA_ROOT_HINT}</p>
        <label class="field wide">
          Additional roots (one ALIAS=PATH per line, optional)
          <textarea
            rows={3}
            spellcheck={false}
            class={extraValid ? '' : 'invalid'}
            value={additional}
            onInput={(event) => setAdditional(event.currentTarget.value)}
          />
        </label>
        <div class="row">
          <label class="check">
            <input type="checkbox" checked={watch} onChange={(event) => setWatch(event.currentTarget.checked)} />
            Keep the index current after edits (watch sources)
          </label>
        </div>
        <p class="muted">
          {props.revision === null
            ? 'Load the index list first; the review is made against the current catalog revision.'
            : `Reviewed against catalog revision ${props.revision}.`}
        </p>
        <button type="submit" disabled={!ready}>
          Review setup
        </button>
      </form>
      {draftValue !== null ? <DraftSummary draft={draftValue} /> : null}
      <Outcome state={draft} />
      {result !== null && token === null ? (
        <p class="error">The service answered without a review token, so this setup cannot be applied here.</p>
      ) : null}
      {draftValue !== null && token !== null ? (
        <ApplySetup
          key={token}
          token={token}
          draft={draftValue}
          onRefresh={props.onRefresh}
          onActivity={props.onActivity}
          onReviewAgain={review}
        />
      ) : null}
    </>
  );
}

// ---------------------------------------------------------------------------
// Index sets.

function CreateSet(props: { revision: number | null; onApplied: () => void }): JSX.Element {
  const [name, setName] = useState('');
  return (
    <>
      <div class="row">
        <Field label="Index set name" wide value={name} onInput={setName} />
      </div>
      <ReviewedChange
        reviewLabel="Review new index set"
        disabled={props.revision === null || name.trim() === ''}
        draftOperation={OPS.catalogPreview}
        makeDraft={() =>
          previewOf(props.revision, { operation: 'create_project', id: newProjectId(), name: name.trim() })
        }
        extract={fromPreview}
        confirm={null}
        applyLabel="Create index set"
        onApplied={props.onApplied}
      />
    </>
  );
}

function AddMember(props: {
  project: Rec;
  revision: number | null;
  indexes: Item[];
  onApplied: () => void;
}): JSX.Element {
  const [indexId, setIndexId] = useState('');
  const [sources, runSources] = useCall();
  const valid = isCatalogId('idx_', indexId);
  const page = rec(okResult(sources)?.page);
  const sourceItems = arr(page?.items)
    .map(itemOf)
    .filter((item): item is Item => item !== null && item.kind === 'source');
  const scope: ScopeAtom[] = sourceItems
    .filter((item) => item.value.enabled === true && str(item.value.index_id) === indexId)
    .map((item) => ({ source_root_id: text(item.value.id), kind: 'subtree', path: '' }));
  const projectId = text(props.project.id);
  const scopeRevision = num(props.project.scope_revision);
  const pageRevision = num(page?.revision);
  const truncated = page !== null && page.continuation !== null && page.continuation !== undefined;
  return (
    <>
      <div class="row">
        <label class="field wide">
          Index to add
          <select
            value={indexId}
            onChange={(event) => {
              setIndexId(event.currentTarget.value);
            }}
          >
            <option value="">Choose an index…</option>
            {props.indexes
              .filter((item) => item.kind === 'index' && item.value.tombstone !== true)
              .map((item) => (
                <option value={text(item.value.id)}>{`${text(item.value.name)} (${text(item.value.id)})`}</option>
              ))}
          </select>
        </label>
        <Field label="or index id" value={indexId} invalid={indexId !== '' && !valid} onInput={setIndexId} />
        <button
          type="button"
          class="secondary"
          disabled={!valid}
          onClick={() =>
            void runSources(OPS.catalogPage, body.catalogPage('sources', { indexId }, null, props.revision))
          }
        >
          Read its sources
        </button>
      </div>
      <p class="muted">
        The member's scope is every enabled source root of that index, whole (the same scope airkctl's setup
        uses). An empty scope would grant nothing.
      </p>
      <Outcome state={sources} />
      {truncated ? <p class="error">The index has more sources than one page; add it with airkctl.</p> : null}
      <ReviewedChange
        reviewLabel="Review adding this index"
        disabled={!valid || scope.length === 0 || truncated || scopeRevision === null || pageRevision === null}
        draftOperation={OPS.catalogPreview}
        makeDraft={() =>
          scopeRevision === null
            ? null
            : previewOf(pageRevision ?? props.revision, {
                operation: 'set_membership',
                membership: { project_id: projectId, index_id: indexId, revision: 1, scope },
                expected_scope_revision: scopeRevision,
                expected_membership_revision: null,
              })
        }
        extract={fromPreview}
        confirm={null}
        applyLabel="Add to index set"
        onApplied={props.onApplied}
      />
    </>
  );
}

function SetDetail(props: {
  id: string;
  reload: number;
  revision: number | null;
  indexes: Item[];
  onRevision: (revision: number) => void;
  onChanged: () => void;
  onOpenIndex: (id: string) => void;
}): JSX.Element {
  const [detail, runDetail] = useCall();
  const [newName, setNewName] = useState('');
  const load = (): void => void runDetail(OPS.catalogDetail, body.catalogDetail({ kind: 'project', id: props.id }, null));
  useEffect(load, [props.id, props.reload]);
  const d = rec(okResult(detail)?.detail);
  const item = itemOf(d?.item);
  const project = item !== null && item.kind === 'project' ? item.value : null;
  const revision = num(d?.revision);
  useEffect(() => {
    if (revision !== null) props.onRevision(revision);
  }, [revision]);
  const name = project !== null ? text(project.name) : null;
  return (
    <Panel title={`Index set ${name ?? props.id}`}>
      <div class="row">
        <button type="button" class="secondary" onClick={load}>
          Reload details
        </button>
      </div>
      {d !== null && d.item === null ? <p class="error">This index set does not exist at this revision.</p> : null}
      {item !== null ? (
        <ul class="items">
          <ItemView item={item} />
        </ul>
      ) : null}
      <Outcome state={detail} />
      {project !== null && revision !== null ? (
        <>
          <h3>Member indexes</h3>
          <CatalogList
            kind="memberships"
            projectId={props.id}
            reload={props.reload}
            onRevision={props.onRevision}
            empty="No member indexes."
            actions={(member) => (
              <>
                <button type="button" class="small" onClick={() => props.onOpenIndex(text(member.value.index_id))}>
                  Open index
                </button>
                <details>
                  <summary>Remove from this set</summary>
                  <ReviewedChange
                    reviewLabel="Review removal"
                    draftOperation={OPS.catalogPreview}
                    makeDraft={() =>
                      previewOf(revision, {
                        operation: 'remove_membership',
                        project_id: props.id,
                        index_id: text(member.value.index_id),
                        expected_scope_revision: num(project.scope_revision),
                        expected_membership_revision: num(member.value.revision),
                      })
                    }
                    extract={fromPreview}
                    confirm={name}
                    applyLabel="Remove member"
                    onApplied={props.onChanged}
                  />
                </details>
              </>
            )}
          />
          <h3>Add an index</h3>
          <AddMember project={project} revision={revision} indexes={props.indexes} onApplied={props.onChanged} />
          <h3>Rename</h3>
          <div class="row">
            <Field label="New name" wide value={newName} onInput={setNewName} />
          </div>
          <ReviewedChange
            reviewLabel="Review rename"
            disabled={newName.trim() === ''}
            draftOperation={OPS.catalogPreview}
            makeDraft={() =>
              previewOf(revision, {
                operation: 'rename_project',
                id: props.id,
                expected_definition_revision: num(project.definition_revision),
                name: newName.trim(),
              })
            }
            extract={fromPreview}
            confirm={null}
            applyLabel="Rename index set"
            onApplied={props.onChanged}
          />
          <h3>Delete</h3>
          <p>Deleting an index set removes the set and its memberships. Its indexes and their data stay.</p>
          <ReviewedChange
            reviewLabel="Review deletion"
            draftOperation={OPS.catalogPreview}
            makeDraft={() =>
              previewOf(revision, {
                operation: 'delete_project',
                id: props.id,
                expected_definition_revision: num(project.definition_revision),
                expected_scope_revision: num(project.scope_revision),
                expected_defaults_revision: num(project.defaults_revision),
              })
            }
            extract={fromPreview}
            confirm={name}
            applyLabel="Delete index set"
            onApplied={props.onChanged}
          />
        </>
      ) : null}
    </Panel>
  );
}

// ---------------------------------------------------------------------------
// Index detail: rename, sources, retire, policy, split.

function SourceChange(props: { index: Rec; revision: number; onApplied: () => void }): JSX.Element {
  const [mode, setMode] = useState<'add_root' | 'remove_root' | 'trim_path' | 'unblock_path'>('add_root');
  const [alias, setAlias] = useState('');
  const [path, setPath] = useState('');
  const [rootId, setRootId] = useState('');
  const [kind, setKind] = useState<'exact' | 'subtree'>('subtree');
  const [ruleId, setRuleId] = useState('');
  const indexId = text(props.index.id);
  let change: SourceMutationSelection | null = null;
  if (mode === 'add_root' && alias !== '' && path !== '') change = { operation: 'add_root', alias, path };
  if (mode === 'remove_root' && alias !== '') change = { operation: 'remove_root', alias };
  if (mode === 'trim_path' && isCatalogId('src_', rootId) && path !== '')
    change = { operation: 'trim_path', source_root_id: rootId, kind, path };
  if (mode === 'unblock_path' && isCatalogId('deny_', ruleId)) change = { operation: 'unblock_path', rule_id: ruleId };
  return (
    <>
      <div class="row">
        <label class="field">
          Change
          <select
            value={mode}
            onChange={(event) => {
              const value = event.currentTarget.value;
              if (value === 'add_root' || value === 'remove_root' || value === 'trim_path' || value === 'unblock_path')
                setMode(value);
            }}
          >
            <option value="add_root">Add a root</option>
            <option value="remove_root">Remove a root</option>
            <option value="trim_path">Exclude a path</option>
            <option value="unblock_path">Remove an exclusion</option>
          </select>
        </label>
        {mode === 'add_root' || mode === 'remove_root' ? <Field label="Alias" value={alias} onInput={setAlias} /> : null}
        {mode === 'trim_path' ? (
          <>
            <Field
              label="Source root id"
              value={rootId}
              placeholder="src_…"
              invalid={rootId !== '' && !isCatalogId('src_', rootId)}
              onInput={setRootId}
            />
            <label class="field narrow">
              Scope
              <select
                value={kind}
                onChange={(event) => setKind(event.currentTarget.value === 'exact' ? 'exact' : 'subtree')}
              >
                <option value="subtree">subtree</option>
                <option value="exact">exact</option>
              </select>
            </label>
          </>
        ) : null}
        {mode === 'add_root' || mode === 'trim_path' ? (
          <Field
            label={mode === 'add_root' ? 'Folder path' : 'Path inside the root'}
            wide
            value={path}
            onInput={setPath}
          />
        ) : null}
        {mode === 'unblock_path' ? (
          <Field
            label="Exclusion rule id"
            value={ruleId}
            placeholder="deny_…"
            invalid={ruleId !== '' && !isCatalogId('deny_', ruleId)}
            onInput={setRuleId}
          />
        ) : null}
      </div>
      <ReviewedChange
        reviewLabel="Review source change"
        disabled={change === null}
        draftOperation={OPS.catalogSourceDraft}
        makeDraft={() => (change === null ? null : body.sourceDraft(props.revision, indexId, change))}
        extract={(result) => fromDraft(result)}
        confirm={text(props.index.name)}
        applyLabel="Apply source change"
        onApplied={props.onApplied}
      />
    </>
  );
}

function PendingSource(props: { indexId: string }): JSX.Element {
  const [pending, runPending] = useCall();
  const [observation, runObservation] = useCall();
  const [resolve, runResolve] = useCall();
  const [begin, setBegin] = useState('');
  const found = findString(okResult(pending), 'begin_operation', isIncarnation);
  useEffect(() => {
    if (found !== null) setBegin(found);
  }, [found]);
  const valid = isIncarnation(begin);
  return (
    <>
      <div class="row">
        <button
          type="button"
          class="secondary"
          onClick={() => void runPending(OPS.catalogPendingSource, body.pendingSource(props.indexId))}
        >
          Check for a pending source change
        </button>
      </div>
      <p class="muted">An empty answer is not evidence that a change completed.</p>
      <Outcome state={pending} />
      <div class="row">
        <Field label="Begin operation" value={begin} invalid={begin !== '' && !valid} onInput={setBegin} />
        <button
          type="button"
          class="secondary"
          disabled={!valid}
          onClick={() => void runObservation(OPS.catalogSourceObservation, body.sourceObservation(begin))}
        >
          Observe
        </button>
        <button
          type="button"
          disabled={!valid || resolve.phase === 'pending'}
          onClick={() => void runResolve(OPS.catalogSourceResolve, body.sourceResolve(begin))}
        >
          Resolve
        </button>
      </div>
      <Outcome state={observation} />
      <Outcome state={resolve} />
    </>
  );
}

function RetireIndex(props: { index: Rec; revision: number; onApplied: () => void }): JSX.Element {
  const [evidence, runEvidence] = useCall();
  const indexId = text(props.index.id);
  const read = evidence.phase === 'done' && evidence.result.kind === 'ok';
  return (
    <>
      <p>
        Retiring keeps the index's history as a tombstone and removes it from every index set and grant. Read the
        retirement evidence first.
      </p>
      <div class="row">
        <button
          type="button"
          class="secondary"
          onClick={() => void runEvidence(OPS.catalogRetirementEvidence, body.retirementEvidence(indexId))}
        >
          Read retirement evidence
        </button>
      </div>
      <Outcome state={evidence} />
      <ReviewedChange
        reviewLabel="Review retirement"
        disabled={!read}
        draftOperation={OPS.catalogSourceDraft}
        makeDraft={() => body.sourceDraft(props.revision, indexId, { operation: 'retire_index' })}
        extract={(result) => fromDraft(result)}
        confirm={text(props.index.name)}
        applyLabel="Retire index"
        onApplied={props.onApplied}
      />
    </>
  );
}

function PolicyPanel(props: { indexId: string }): JSX.Element {
  const [status, runStatus] = useCall();
  const [resume, runResume] = useCall();
  const [receipt, runReceipt] = useCall();
  const [operationId, setOperationId] = useState('');
  const found = findString(okResult(status), 'operation_id', isIncarnation);
  useEffect(() => {
    if (found !== null) setOperationId(found);
  }, [found]);
  const valid = isIncarnation(operationId);
  const token = reviewIn(resume);
  return (
    <>
      <div class="row">
        <button
          type="button"
          class="secondary"
          onClick={() => void runStatus(OPS.catalogPolicyStatus, body.policyStatus(props.indexId))}
        >
          Policy status
        </button>
      </div>
      <Outcome state={status} />
      <div class="row">
        <Field
          label="Policy operation id"
          value={operationId}
          invalid={operationId !== '' && !valid}
          onInput={setOperationId}
        />
        <button
          type="button"
          disabled={!valid || resume.phase === 'pending'}
          onClick={() => void runResume(OPS.catalogPolicyResume, body.policyResume(props.indexId, operationId))}
        >
          Resume policy change
        </button>
        <button
          type="button"
          class="secondary"
          disabled={!valid || token === null}
          onClick={() =>
            void runReceipt(
              OPS.catalogPolicyResumeReceipt,
              reviewedCall(body.policyResumeReceipt(props.indexId, operationId), token),
            )
          }
        >
          Read resume receipt
        </button>
      </div>
      <Outcome state={resume} />
      <Outcome state={receipt} />
      <ReviewHint
        state={receipt}
        confirm={null}
        onReviewAgain={() => void runResume(OPS.catalogPolicyResume, body.policyResume(props.indexId, operationId))}
      />
    </>
  );
}

function SplitPanel(props: { index: Rec; revision: number; onApplied: () => void }): JSX.Element {
  const indexId = text(props.index.id);
  const [state, runState] = useCall();
  const [draft, runDraft] = useCall();
  const [selectors, setSelectors] = useState('[]');
  const [projects, setProjects] = useState('[]');
  const [name, setName] = useState('');
  const [dataRoot, setDataRoot] = useState('');
  const [collection, setCollection] = useState('');
  const [growth, setGrowth] = useState('0');
  const [temporary, setTemporary] = useState('0');
  const [rewrite, setRewrite] = useState('0');
  const [cleanup, setCleanup] = useState('0');
  const [semantic, setSemantic] = useState(false);
  const [watch, setWatch] = useState(true);
  const selectorList = parseJsonArray(selectors);
  const projectList = parseJsonArray(projects);
  const capacity = [growth, temporary, rewrite, cleanup].map(parseCount);
  const ready =
    selectorList !== null &&
    selectorList.length > 0 &&
    projectList !== null &&
    name.trim() !== '' &&
    dataRoot.trim() !== '' &&
    /^[A-Za-z0-9._-]{1,128}$/.test(collection) &&
    capacity.every((n) => n !== null);
  const review = (): void => {
    if (!ready || selectorList === null || projectList === null) return;
    void runDraft(
      OPS.catalogSplitDraft,
      body.splitDraft({
        expected_catalog_revision: props.revision,
        index_id: indexId,
        selectors: selectorList as ScopeAtom[],
        destination_name: name.trim(),
        destination_data_root: dataRoot.trim(),
        destination_collection: collection,
        projects: projectList,
        capacity: {
          destination_growth: capacity[0] ?? 0,
          destination_temporary: capacity[1] ?? 0,
          original_rewrite: capacity[2] ?? 0,
          cleanup_allowance: capacity[3] ?? 0,
        },
        require_semantic_ready: semantic,
        destination_watch_enabled: watch,
      }),
    );
  };
  const token = reviewIn(draft);
  const draftValue = okResult(draft)?.draft;
  return (
    <>
      <p>
        A split moves selected source paths of <strong>{text(props.index.name)}</strong> into a new, independent
        index. Read the source state first; it lists the source root ids the selectors use.
      </p>
      <div class="row">
        <button
          type="button"
          class="secondary"
          onClick={() => void runState(OPS.catalogSplitSourceState, body.splitSourceState(indexId, props.revision))}
        >
          Read split source state
        </button>
      </div>
      <Outcome state={state} />
      <label class="field wide">
        Selectors: JSON array of {'{"source_root_id","kind":"exact"|"subtree","path"}'}
        <textarea
          rows={3}
          spellcheck={false}
          class={selectorList === null ? 'invalid' : ''}
          value={selectors}
          onInput={(event) => setSelectors(event.currentTarget.value)}
        />
      </label>
      <label class="field wide">
        Index set choices: JSON array of{' '}
        {'{"project_id","retain_original","membership":{"destination":"omit"|"preserve_reviewed_scope"|"selected","scope"?}}'}
        <textarea
          rows={3}
          spellcheck={false}
          class={projectList === null ? 'invalid' : ''}
          value={projects}
          onInput={(event) => setProjects(event.currentTarget.value)}
        />
      </label>
      <div class="row">
        <Field label="New index name" value={name} onInput={setName} />
        <Field label="New collection" value={collection} onInput={setCollection} />
        <Field label="New data root" wide value={dataRoot} onInput={setDataRoot} />
      </div>
      <p class="muted">Capacity ceilings in bytes (peak growth the split may use):</p>
      <div class="row">
        <Field label="Destination growth" value={growth} invalid={parseCount(growth) === null} onInput={setGrowth} />
        <Field
          label="Destination temporary"
          value={temporary}
          invalid={parseCount(temporary) === null}
          onInput={setTemporary}
        />
        <Field label="Original rewrite" value={rewrite} invalid={parseCount(rewrite) === null} onInput={setRewrite} />
        <Field label="Cleanup allowance" value={cleanup} invalid={parseCount(cleanup) === null} onInput={setCleanup} />
      </div>
      <div class="row">
        <label class="check">
          <input type="checkbox" checked={semantic} onChange={(event) => setSemantic(event.currentTarget.checked)} />
          Require semantic readiness
        </label>
        <label class="check">
          <input type="checkbox" checked={watch} onChange={(event) => setWatch(event.currentTarget.checked)} />
          Watch the new index's sources
        </label>
      </div>
      <button type="button" class="secondary" disabled={!ready} onClick={review}>
        Review split
      </button>
      <Outcome state={draft} />
      {draftValue !== undefined && token !== null ? (
        <SplitApply
          key={token}
          token={token}
          draft={draftValue}
          originalName={text(props.index.name)}
          onReviewAgain={review}
          onApplied={props.onApplied}
        />
      ) : null}
      <SplitStatus />
    </>
  );
}

function SplitApply(props: {
  token: string;
  draft: unknown;
  originalName: string;
  onReviewAgain: () => void;
  onApplied: () => void;
}): JSX.Element {
  const [apply, runApply] = useCall();
  const [receipt, runReceipt] = useCall();
  const go = async (): Promise<void> => {
    const result = await runApply(
      OPS.catalogSplitApply,
      reviewedCall(body.splitApply(props.draft), props.token, props.originalName),
    );
    if (result.kind === 'ok') props.onApplied();
  };
  return (
    <div class="apply">
      <Confirm phrase={props.originalName} action="Split index" danger onConfirm={() => void go()} />
      <Outcome state={apply} />
      <ReviewHint state={apply} confirm={props.originalName} onReviewAgain={props.onReviewAgain} />
      <div class="row">
        <button
          type="button"
          class="secondary"
          disabled={apply.phase !== 'done'}
          onClick={() => void runReceipt(OPS.catalogSplitReceipt, reviewedCall(body.splitReceipt(), props.token))}
        >
          Read split receipt
        </button>
      </div>
      <Outcome state={receipt} />
      <ReviewHint state={receipt} confirm={null} onReviewAgain={props.onReviewAgain} />
    </div>
  );
}

function SplitStatus(): JSX.Element {
  const [status, runStatus] = useCall();
  const [recover, runRecover] = useCall();
  const [recoveryReceipt, runRecoveryReceipt] = useCall();
  const [operationId, setOperationId] = useState('');
  const [requestDigest, setRequestDigest] = useState('');
  const [receiptDigest, setReceiptDigest] = useState('');
  const [finalization, setFinalization] = useState('');
  const [action, setAction] = useState<'resume' | 'cancel'>('resume');
  const valid = isIncarnation(operationId);
  const finalizationValue = finalization === '' ? null : parseCount(finalization);
  const recoverReady =
    valid &&
    isDigest(requestDigest) &&
    isDigest(receiptDigest) &&
    (finalization === '' || (finalizationValue !== null && finalizationValue > 0));
  const token = reviewIn(recover);
  const recoverNow = (): void => {
    if (!recoverReady) return;
    void runRecover(
      OPS.catalogSplitRecover,
      body.splitRecover({
        operation_id: operationId,
        request_digest: requestDigest,
        expected_receipt_digest: receiptDigest,
        ...(finalizationValue !== null ? { expected_finalization_revision: finalizationValue } : {}),
        action,
      }),
    );
  };
  return (
    <>
      <h4>Split status and recovery</h4>
      <div class="row">
        <Field
          label="Split operation id"
          value={operationId}
          invalid={operationId !== '' && !valid}
          onInput={setOperationId}
        />
        <button
          type="button"
          class="secondary"
          disabled={!valid}
          onClick={() => void runStatus(OPS.catalogSplitStatus, body.splitStatus(operationId))}
        >
          Split status
        </button>
      </div>
      <Outcome state={status} />
      <div class="row">
        <Field
          label="Request digest"
          wide
          value={requestDigest}
          invalid={requestDigest !== '' && !isDigest(requestDigest)}
          onInput={setRequestDigest}
        />
        <Field
          label="Expected receipt digest"
          wide
          value={receiptDigest}
          invalid={receiptDigest !== '' && !isDigest(receiptDigest)}
          onInput={setReceiptDigest}
        />
      </div>
      <div class="row">
        <Field
          label="Expected finalization revision (optional)"
          value={finalization}
          invalid={finalization !== '' && (finalizationValue === null || finalizationValue === 0)}
          onInput={setFinalization}
        />
        <label class="field narrow">
          Action
          <select value={action} onChange={(event) => setAction(event.currentTarget.value === 'cancel' ? 'cancel' : 'resume')}>
            <option value="resume">resume</option>
            <option value="cancel">cancel</option>
          </select>
        </label>
        <Confirm
          phrase={`${action} ${short(operationId)}`}
          action="Recover split"
          danger={action === 'cancel'}
          disabled={!recoverReady}
          onConfirm={recoverNow}
        />
        <button
          type="button"
          class="secondary"
          disabled={!valid || token === null}
          onClick={() =>
            void runRecoveryReceipt(
              OPS.catalogSplitRecoveryReceipt,
              reviewedCall(body.splitRecoveryReceipt(operationId), token),
            )
          }
        >
          Read recovery receipt
        </button>
      </div>
      <Outcome state={recover} />
      <Outcome state={recoveryReceipt} />
      <ReviewHint state={recoveryReceipt} confirm={null} onReviewAgain={recoverNow} />
    </>
  );
}

function IndexDetail(props: {
  id: string;
  reload: number;
  onRevision: (revision: number) => void;
  onChanged: () => void;
}): JSX.Element {
  const [detail, runDetail] = useCall();
  const [newName, setNewName] = useState('');
  const load = (): void => void runDetail(OPS.catalogDetail, body.catalogDetail({ kind: 'index', id: props.id }, null));
  useEffect(load, [props.id, props.reload]);
  const d = rec(okResult(detail)?.detail);
  const item = itemOf(d?.item);
  const index = item !== null && item.kind === 'index' ? item.value : null;
  const revision = num(d?.revision);
  useEffect(() => {
    if (revision !== null) props.onRevision(revision);
  }, [revision]);
  const name = index !== null ? text(index.name) : null;
  return (
    <Panel title={`Index ${name ?? props.id}`}>
      <div class="row">
        <button type="button" class="secondary" onClick={load}>
          Reload details
        </button>
      </div>
      {d !== null && d.item === null ? <p class="error">This index does not exist at this revision.</p> : null}
      {item !== null ? (
        <ul class="items">
          <ItemView item={item} />
        </ul>
      ) : null}
      <Outcome state={detail} />
      {index !== null && revision !== null ? (
        <>
          <h3>Sources</h3>
          <CatalogList
            kind="sources"
            indexId={props.id}
            reload={props.reload}
            onRevision={props.onRevision}
            empty="No sources listed."
          />
          <h3>Index sets containing it</h3>
          <CatalogList
            kind="memberships"
            indexId={props.id}
            reload={props.reload}
            onRevision={props.onRevision}
            empty="It is not a member of any index set."
          />
          {index.tombstone === true ? (
            <p class="muted">This index is retired; it can no longer be changed.</p>
          ) : (
            <>
              <h3>Rename</h3>
              <div class="row">
                <Field label="New name" wide value={newName} onInput={setNewName} />
              </div>
              <ReviewedChange
                reviewLabel="Review rename"
                disabled={newName.trim() === ''}
                draftOperation={OPS.catalogPreview}
                makeDraft={() =>
                  previewOf(revision, {
                    operation: 'rename_index',
                    id: props.id,
                    expected_definition_revision: num(index.definition_revision),
                    name: newName.trim(),
                  })
                }
                extract={fromPreview}
                confirm={null}
                applyLabel="Rename index"
                onApplied={props.onChanged}
              />
              <h3>Change sources</h3>
              <SourceChange index={index} revision={revision} onApplied={props.onChanged} />
              <h4>Pending source change</h4>
              <PendingSource indexId={props.id} />
              <h3>Source policy</h3>
              <PolicyPanel indexId={props.id} />
              <h3>Split</h3>
              <SplitPanel index={index} revision={revision} onApplied={props.onChanged} />
              <h3>Retire</h3>
              <RetireIndex index={index} revision={revision} onApplied={props.onChanged} />
            </>
          )}
        </>
      ) : null}
    </Panel>
  );
}

// ---------------------------------------------------------------------------
// Grants.

function Grants(props: { revision: number | null; reload: number; onRevision: (r: number) => void; onChanged: () => void }): JSX.Element {
  const [contextKey, setContextKey] = useState('');
  const [projectIds, setProjectIds] = useState('');
  const [indexIds, setIndexIds] = useState('');
  const projects = list(projectIds);
  const indexes = list(indexIds);
  const valid =
    isDigest(contextKey) &&
    projects.length > 0 &&
    projects.every((id) => isCatalogId('prj_', id)) &&
    indexes.every((id) => isCatalogId('idx_', id));
  return (
    <>
      <CatalogList
        kind="grants"
        reload={props.reload}
        onRevision={props.onRevision}
        empty="No grants."
        actions={(item) => {
          const grant = rec(item.value.grant);
          const id = text(grant?.id);
          if (grant === null || grant.revoked === true) return null;
          return (
            <details>
              <summary>Revoke</summary>
              <ReviewedChange
                reviewLabel="Review revocation"
                draftOperation={OPS.catalogPreview}
                makeDraft={() =>
                  previewOf(props.revision, { operation: 'revoke_grant', id, expected_revision: num(grant.revision) })
                }
                extract={fromPreview}
                confirm={id}
                applyLabel="Revoke grant"
                onApplied={props.onChanged}
              />
            </details>
          );
        }}
      />
      <h3>Draft a grant</h3>
      <p class="muted">
        A grant lets one agent context read the chosen index sets. The service builds the complete grant; nothing
        is issued until you apply it.
      </p>
      <div class="row">
        <Field
          label="Context key (64 hex)"
          wide
          value={contextKey}
          invalid={contextKey !== '' && !isDigest(contextKey)}
          onInput={setContextKey}
        />
      </div>
      <div class="row">
        <Field label="Index set ids" wide value={projectIds} placeholder="prj_…" onInput={setProjectIds} />
        <Field label="Index ids (optional)" wide value={indexIds} placeholder="idx_…" onInput={setIndexIds} />
      </div>
      <ReviewedChange
        reviewLabel="Review grant"
        disabled={!valid || props.revision === null}
        draftOperation={OPS.catalogGrantDraft}
        makeDraft={() =>
          props.revision === null
            ? null
            : body.grantDraft(contextKey, props.revision, projects, indexes.length > 0 ? indexes : null)
        }
        extract={(result) => fromDraft(result)}
        confirm={null}
        applyLabel="Create grant"
        onApplied={props.onChanged}
      />
    </>
  );
}

// ---------------------------------------------------------------------------
// The page.

export function IndexLibrary(props: { params: URLSearchParams; seq: number; onActivity: () => void }): JSX.Element {
  const [revision, setRevision] = useState<number | null>(null);
  const [reload, setReload] = useState(0);
  const [indexItems, setIndexItems] = useState<Item[]>([]);
  const [openIndex, setOpenIndex] = useState<string | null>(null);
  const [openSet, setOpenSet] = useState<string | null>(null);

  useEffect(() => {
    const index = props.params.get('index');
    const set = props.params.get('set');
    if (index !== null && isCatalogId('idx_', index)) setOpenIndex(index);
    if (set !== null && isCatalogId('prj_', set)) setOpenSet(set);
  }, [props.seq]);

  const refresh = (): void => setReload((n) => n + 1);
  // The revision of the latest catalog read; drafts and previews are reviewed against it.
  const onRevision = (next: number): void => setRevision(next);

  return (
    <>
      <Panel title="Indexes">
        <CatalogList
          kind="indexes"
          reload={reload}
          onRevision={onRevision}
          onItems={setIndexItems}
          empty="No indexes yet. Set one up below."
          actions={(item) => (
            <button type="button" class="small" onClick={() => setOpenIndex(text(item.value.id))}>
              Open
            </button>
          )}
        />
      </Panel>
      {openIndex !== null ? (
        <>
          <div class="row">
            <button type="button" class="small secondary" onClick={() => setOpenIndex(null)}>
              Close index details
            </button>
          </div>
          <IndexDetail id={openIndex} reload={reload} onRevision={onRevision} onChanged={refresh} />
        </>
      ) : null}
      <Panel title="Set up an index">
        <SetupIndex
          revision={revision}
          root={props.params.get('root')}
          seq={props.seq}
          onRefresh={refresh}
          onActivity={props.onActivity}
        />
      </Panel>
      <Panel title="Index sets">
        <CatalogList
          kind="projects"
          reload={reload}
          onRevision={onRevision}
          empty="No index sets yet."
          actions={(item) => (
            <button type="button" class="small" onClick={() => setOpenSet(text(item.value.id))}>
              Open
            </button>
          )}
        />
        <h3>Create an index set</h3>
        <CreateSet revision={revision} onApplied={refresh} />
      </Panel>
      {openSet !== null ? (
        <>
          <div class="row">
            <button type="button" class="small secondary" onClick={() => setOpenSet(null)}>
              Close index set details
            </button>
          </div>
          <SetDetail
            id={openSet}
            reload={reload}
            revision={revision}
            indexes={indexItems}
            onRevision={onRevision}
            onChanged={refresh}
            onOpenIndex={setOpenIndex}
          />
        </>
      ) : null}
      <Panel title="Grants">
        <Grants revision={revision} reload={reload} onRevision={onRevision} onChanged={refresh} />
      </Panel>
    </>
  );
}
