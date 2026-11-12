// SPDX-License-Identifier: Apache-2.0
// Usage (usage.document, N1): the complete settings or usage document the CLI
// reads, for one index's project binding. The full native result is always
// shown; the summary only names what it contains.
import type { JSX } from 'preact';
import { useEffect, useState } from 'preact/hooks';
import { body, isDigest, OPS, type UsageRead } from './operations';
import { Field, NumberField, Outcome, Panel, resultOf, Text, useCall } from './render';

const SETTINGS_VIEWS = [
  'extraction_policy',
  'vector_selection',
  'vector_status',
  'roots',
  'scan_limits',
  'quota_policy',
  'quota_status',
] as const;

type ReadKind = 'report' | 'policy' | 'settings';
type Format = 'json' | 'csv' | 'markdown';

type Choice = { binding: string; label: string };

/** Index backings from a catalog `indexes` page: one usage binding each. */
function bindingsOf(result: unknown): Choice[] {
  const page = (result as { page?: { items?: unknown[] } } | undefined)?.page;
  const choices: Choice[] = [];
  for (const entry of page?.items ?? []) {
    const item = entry as { kind?: unknown; value?: Record<string, unknown> };
    if (item.kind !== 'index' || item.value === undefined) continue;
    const backing = item.value.backing as { binding?: unknown } | undefined;
    if (typeof backing?.binding !== 'string' || !isDigest(backing.binding)) continue;
    const name = typeof item.value.name === 'string' ? item.value.name : backing.binding;
    const retired = item.value.tombstone === true ? ' (retired)' : '';
    choices.push({ binding: backing.binding, label: `${name}${retired}` });
  }
  return choices;
}

/** A readable outline of a document: its kind and top-level fields. */
function Summary(props: { result: unknown }): JSX.Element | null {
  const r = props.result as { status?: unknown; document?: unknown; message?: unknown } | undefined;
  if (r === undefined || r === null || typeof r !== 'object') return null;
  const doc = r.document;
  const lines: string[] = [`status: ${String(r.status)}`];
  if (typeof r.message === 'string') lines.push(`message: ${r.message}`);
  if (typeof doc === 'string') {
    lines.push(`text document, ${doc.split('\n').length} lines (shown below)`);
  } else if (Array.isArray(doc)) {
    lines.push(`list of ${doc.length} entries`);
  } else if (typeof doc === 'object' && doc !== null) {
    for (const [key, value] of Object.entries(doc)) {
      const shape = Array.isArray(value)
        ? `${value.length} entries`
        : typeof value === 'object' && value !== null
          ? `${Object.keys(value).length} fields`
          : JSON.stringify(value);
      lines.push(`${key}: ${shape}`);
    }
  }
  return (
    <ul class="summary-lines">
      {lines.map((line) => (
        <li>
          <Text value={line} />
        </li>
      ))}
    </ul>
  );
}

export function Usage(): JSX.Element {
  const [indexes, runIndexes] = useCall();
  const [usageDoc, runDocument] = useCall();
  const [binding, setBinding] = useState('');
  const [kind, setKind] = useState<ReadKind>('report');
  const [days, setDays] = useState(30);
  const [format, setFormat] = useState<Format>('markdown');
  const [view, setView] = useState<string>('roots');
  useEffect(() => {
    void runIndexes(OPS.catalogPage, body.catalogIndexes(null));
  }, []);
  const choices = bindingsOf(resultOf(indexes));
  const valid = isDigest(binding);
  const read = (): UsageRead => {
    if (kind === 'policy') return 'policy';
    if (kind === 'settings') return { settings: { view } };
    return { report: { days, format } };
  };
  return (
    <>
      <Panel title="Usage and settings">
        <p class="muted">
          Reads the same document as <code>airkctl usage</code> and <code>airkctl settings</code> for one index.
        </p>
        <div class="row">
          <label class="field wide">
            Index
            <select value={binding} onChange={(event) => setBinding(event.currentTarget.value)}>
              <option value="">Choose an index…</option>
              {choices.map((choice) => (
                <option value={choice.binding}>{`${choice.label} (${choice.binding.slice(0, 12)})`}</option>
              ))}
            </select>
          </label>
          <Field
            label="or project binding (64 hex)"
            wide
            value={binding}
            invalid={binding !== '' && !valid}
            onInput={setBinding}
          />
        </div>
        <div class="row">
          <label class="field">
            Read
            <select
              value={kind}
              onChange={(event) => {
                const value = event.currentTarget.value;
                if (value === 'report' || value === 'policy' || value === 'settings') setKind(value);
              }}
            >
              <option value="report">Usage report</option>
              <option value="policy">Usage policy</option>
              <option value="settings">Settings</option>
            </select>
          </label>
          {kind === 'report' ? (
            <>
              <NumberField label="Days" value={days} min={1} max={365} onInput={setDays} />
              <label class="field narrow">
                Format
                <select
                  value={format}
                  onChange={(event) => {
                    const value = event.currentTarget.value;
                    if (value === 'json' || value === 'csv' || value === 'markdown') setFormat(value);
                  }}
                >
                  <option value="markdown">markdown</option>
                  <option value="json">json</option>
                  <option value="csv">csv</option>
                </select>
              </label>
            </>
          ) : null}
          {kind === 'settings' ? (
            <label class="field">
              View
              <select value={view} onChange={(event) => setView(event.currentTarget.value)}>
                {SETTINGS_VIEWS.map((name) => (
                  <option value={name}>{name}</option>
                ))}
              </select>
            </label>
          ) : null}
          <button
            type="button"
            disabled={!valid}
            onClick={() => void runDocument(OPS.usageDocument, body.usageDocument(binding, read()))}
          >
            Read
          </button>
        </div>
        <Summary result={resultOf(usageDoc)} />
        <Outcome state={usageDoc} />
      </Panel>
      <Panel title="Indexes">
        <button
          type="button"
          class="secondary"
          onClick={() => void runIndexes(OPS.catalogPage, body.catalogIndexes(null))}
        >
          Reload
        </button>
        <Outcome state={indexes} />
      </Panel>
    </>
  );
}
