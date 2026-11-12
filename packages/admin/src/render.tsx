// SPDX-License-Identifier: Apache-2.0
// Shared display pieces. Review M3: every value, including indexed evidence, is
// rendered as text nodes; no HTML sink is used anywhere in this package. Bidi and
// C0/C1 control characters are shown as visible escapes.
import type { ComponentChildren, JSX } from 'preact';
import { useState } from 'preact/hooks';
import { call, NOT_RUNNING, reviewOf, type CallResult } from './api';
import type { Operation } from './operations';

// C0 except tab/newline, DEL, C1, ALM, LRM/RLM, LRE..RLO, LRI..PDI, BOM.
const INVISIBLE = /[\u0000-\u0008\u000b-\u001f\u007f-\u009f؜‎‏‪-‮⁦-⁩﻿]/g;

export function visible(text: string): ComponentChildren[] {
  const parts: ComponentChildren[] = [];
  let last = 0;
  for (const match of text.matchAll(INVISIBLE)) {
    const index = match.index ?? 0;
    if (index > last) parts.push(text.slice(last, index));
    const code = match[0].codePointAt(0)!.toString(16).toUpperCase().padStart(4, '0');
    parts.push(<span class="ctl" title="invisible or direction-changing character">{`<U+${code}>`}</span>);
    last = index + match[0].length;
  }
  if (last < text.length) parts.push(text.slice(last));
  return parts;
}

export function Text(props: { value: string }): JSX.Element {
  return <>{visible(props.value)}</>;
}

/** Exports are attachment-only JSON blobs (M3), never text/html. */
function download(name: string, value: unknown): void {
  const blob = new Blob([`${JSON.stringify(value, null, 2)}\n`], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = name;
  anchor.rel = 'noopener noreferrer';
  anchor.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 0);
}

function copy(value: unknown): void {
  void navigator.clipboard?.writeText(`${JSON.stringify(value, null, 2)}\n`);
}

export function JsonBlock(props: { value: unknown; name: string }): JSX.Element {
  const text = JSON.stringify(props.value, null, 2) ?? 'undefined';
  return (
    <div class="json">
      <div class="json-actions">
        <button type="button" class="small" onClick={() => copy(props.value)}>
          Copy JSON
        </button>
        <button type="button" class="small" onClick={() => download(props.name, props.value)}>
          Download JSON
        </button>
      </div>
      <pre>{visible(text)}</pre>
    </div>
  );
}

/** A `document` string (tool catalog/help, usage reports) shown as its own text. */
function DocumentText(props: { result: unknown }): JSX.Element | null {
  const r = props.result;
  if (typeof r !== 'object' || r === null) return null;
  const doc = (r as Record<string, unknown>).document;
  if (typeof doc !== 'string') return null;
  return (
    <details open>
      <summary>Document text</summary>
      <pre class="doc">{visible(doc)}</pre>
    </details>
  );
}

export type CallState =
  | { phase: 'idle' }
  | { phase: 'pending'; operation: Operation }
  | { phase: 'done'; operation: Operation; result: CallResult; at: number };

export function useCall(): [CallState, (operation: Operation, body: unknown) => Promise<CallResult>] {
  const [state, setState] = useState<CallState>({ phase: 'idle' });
  const run = async (operation: Operation, body: unknown): Promise<CallResult> => {
    setState({ phase: 'pending', operation });
    const result = await call(operation, body);
    setState({ phase: 'done', operation, result, at: Date.now() });
    return result;
  };
  return [state, run];
}

const REQUEST_STATE_TEXT: Record<string, string> = {
  submitted: 'Submitted; the service has not confirmed it yet.',
  accepted: 'Accepted by the service journal.',
  completed: 'Completed.',
  refused: 'Refused by the service.',
  unknown: 'Outcome unknown. Check Activity → Saved requests before trying again.',
};

/** Reviewed-call codes (contracts §4.3b, U98). */
const ERROR_TEXT: Record<string, string> = {
  admin_review_unknown: 'Review expired; run the review again.',
  admin_confirmation_mismatch:
    "The typed text did not equal the target's current name exactly, so nothing was sent. Reload the details, then type the name exactly as shown.",
};

export function Outcome(props: { state: CallState }): JSX.Element | null {
  const s = props.state;
  if (s.phase === 'idle') return null;
  if (s.phase === 'pending') return <p class="muted">Calling {s.operation}…</p>;
  const r = s.result;
  const stamp = new Date(s.at).toISOString().replace(/[:.]/g, '-');
  const file = `airk-${s.operation}-${stamp}.json`;
  switch (r.kind) {
    case 'no_session':
      return <p class="error">No session. Open with <code>airkctl admin open</code>.</p>;
    case 'transport':
      return (
        <p class="error">
          {r.status === null ? NOT_RUNNING : <Text value={`${r.message}`} />}
        </p>
      );
    case 'failed': {
      const e = r.envelope.error;
      return (
        <div class="outcome failed">
          <p class="error">
            {s.operation} failed: <code><Text value={e.code} /></code> ({e.class}, HTTP {r.status})
            {e.retryable ? ' — retryable' : ''}
            {typeof e.retry_after_ms === 'number' ? ` after ${e.retry_after_ms} ms` : ''}
          </p>
          {ERROR_TEXT[e.code] !== undefined ? <p class="error">{ERROR_TEXT[e.code]}</p> : null}
          {e.native !== undefined ? <JsonBlock name={file} value={e.native} /> : null}
        </div>
      );
    }
    case 'ok': {
      const request = r.envelope.request;
      return (
        <div class="outcome ok">
          {request ? (
            <p class={`request request-${request.state}`}>
              Request <code>{request.nonce}</code>: {request.state}.{' '}
              {REQUEST_STATE_TEXT[request.state] ?? ''}
            </p>
          ) : null}
          {reviewOf(r) !== null ? (
            <p class="muted">
              This review is held by this session; its request identity stays in the service and is not
              part of the result below.
            </p>
          ) : null}
          <DocumentText result={r.envelope.result} />
          <JsonBlock name={file} value={r.envelope.result} />
        </div>
      );
    }
  }
}

/** Typed confirmation (plan §7.2): the button stays disabled until the phrase matches. */
export function Confirm(props: {
  phrase: string;
  action: string;
  disabled?: boolean;
  danger?: boolean;
  onConfirm: () => void;
}): JSX.Element {
  const [typed, setTyped] = useState('');
  const ready = typed === props.phrase && !props.disabled;
  return (
    <form
      class="confirm"
      onSubmit={(event) => {
        event.preventDefault();
        if (!ready) return;
        setTyped('');
        props.onConfirm();
      }}
    >
      <label>
        Type <code>{props.phrase}</code> to confirm
        <input
          type="text"
          autocomplete="off"
          spellcheck={false}
          value={typed}
          onInput={(event) => setTyped(event.currentTarget.value)}
        />
      </label>
      <button type="submit" class={props.danger ? 'danger' : ''} disabled={!ready}>
        {props.action}
      </button>
    </form>
  );
}

export function Field(props: {
  label: string;
  value: string;
  onInput: (value: string) => void;
  placeholder?: string;
  invalid?: boolean;
  wide?: boolean;
}): JSX.Element {
  return (
    <label class={props.wide ? 'field wide' : 'field'}>
      {props.label}
      <input
        type="text"
        autocomplete="off"
        spellcheck={false}
        class={props.invalid ? 'invalid' : ''}
        placeholder={props.placeholder ?? ''}
        value={props.value}
        onInput={(event) => props.onInput(event.currentTarget.value)}
      />
    </label>
  );
}

export function NumberField(props: {
  label: string;
  value: number;
  min: number;
  max: number;
  onInput: (value: number) => void;
}): JSX.Element {
  return (
    <label class="field narrow">
      {props.label}
      <input
        type="number"
        min={props.min}
        max={props.max}
        value={props.value}
        onInput={(event) => {
          const n = Number.parseInt(event.currentTarget.value, 10);
          if (Number.isFinite(n)) props.onInput(Math.min(props.max, Math.max(props.min, n)));
        }}
      />
    </label>
  );
}

export function Command(props: { text: string }): JSX.Element {
  return (
    <div class="command">
      <code>{visible(props.text)}</code>
      <button type="button" class="small" onClick={() => void navigator.clipboard?.writeText(props.text)}>
        Copy
      </button>
    </div>
  );
}

export function Panel(props: { title: string; children: ComponentChildren }): JSX.Element {
  return (
    <section class="panel">
      <h2>{props.title}</h2>
      {props.children}
    </section>
  );
}

/** Splits a comma, space or newline separated list. */
export function list(value: string): string[] {
  return value.split(/[\s,]+/).filter((item) => item.length > 0);
}

/** Collects 64-hex `binding` fields from a result, for convenience prefills. */
export function bindingsIn(value: unknown, found: Set<string> = new Set()): string[] {
  if (Array.isArray(value)) {
    for (const item of value) bindingsIn(item, found);
  } else if (typeof value === 'object' && value !== null) {
    for (const [key, item] of Object.entries(value)) {
      if (key === 'binding' && typeof item === 'string' && /^[0-9a-f]{64}$/.test(item)) found.add(item);
      else bindingsIn(item, found);
    }
  }
  return [...found];
}

export function resultOf(state: CallState): unknown {
  return state.phase === 'done' && state.result.kind === 'ok' ? state.result.envelope.result : undefined;
}
