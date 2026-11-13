// SPDX-License-Identifier: Apache-2.0
// Help page: the bundled help topics (`help` tool through `search.call`) and the
// unified tool catalog (`tools.catalog`), shown as readable documents.
//
// Data shapes (read defensively; the complete native result always stays
// available under "Complete result (JSON)"):
// - tools.catalog → `{status: "tools_catalog", document}`; `document` is JSON
//   text: `{tools: [{name, description}]}` for a list, one rmcp `Tool`
//   (`name`, `description`, `inputSchema`, …) for help
//   (crates/airk-frontend/src/tool_catalog.rs).
// - search.call(help) → `{status: "tool_result", result: {content: [{type:
//   "text", text}], isError}}`; `text` is the help reply JSON `{topic, title,
//   content, start_line, end_line, total_lines, next_line, truncated, topics}`
//   or `{error: {code}}` (crates/airk-mcp/src/help.rs `answer_with_catalog`).
//
// Every value is rendered as text nodes; the Markdown renderer below builds
// Preact elements and never uses an HTML sink.
import type { ComponentChildren, JSX } from 'preact';
import { useEffect, useRef, useState } from 'preact/hooks';
import type { CallResult } from './api';
import { body, OPS } from './operations';
import { Outcome, Text, useCall, visible, type CallState } from './render';

type Rec = Record<string, unknown>;
const rec = (value: unknown): Rec | null =>
  typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as Rec) : null;
const str = (value: unknown): string | null => (typeof value === 'string' ? value : null);
const num = (value: unknown): number | null => (typeof value === 'number' && Number.isFinite(value) ? value : null);
const arr = (value: unknown): unknown[] => (Array.isArray(value) ? value : []);

/** The help tool's largest page budget (help.rs: 4096..=32768). */
const HELP_MAX_BYTES = 32768;

function parseJson(text: string): unknown {
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return undefined;
  }
}

// ---------------------------------------------------------------------------
// Failures: the code plus a readable sentence. `Outcome` still shows the rest.

const FAILURES: Record<string, string> = {
  unknown_tool: 'There is no tool with that name in the catalog.',
  unknown_mcp_tool: 'There is no tool with that name in the catalog.',
  unknown_help_topic: 'There is no help topic with that name.',
  invalid_help_selector: 'The topic or tool name is too long or the selectors conflict.',
  invalid_help_budget_or_start_line: 'The help page position is not valid.',
  help_start_line_out_of_range: 'The help page position is past the end of the topic.',
  help_line_exceeds_budget: 'One line of this topic is larger than the help page budget.',
  tool_call_failed: 'The help call failed before it produced an answer.',
  admin_scope_not_granted: 'This session has no search scope; select one on the Search page.',
};

type Problem = { code: string; sentence: string };

function callProblem(result: CallResult): Problem | null {
  switch (result.kind) {
    case 'ok':
      return null;
    case 'no_session':
      return { code: 'no_session', sentence: 'No session. Open AIRK Admin again with airkctl admin open.' };
    case 'transport':
      return { code: 'transport', sentence: result.message };
    case 'failed': {
      const native = rec(result.envelope.error.native);
      const code = str(native?.code) ?? result.envelope.error.code;
      const message = str(native?.message);
      return { code, sentence: FAILURES[code] ?? message ?? 'The service refused the request.' };
    }
  }
}

function ProblemLine(props: { problem: Problem }): JSX.Element {
  return (
    <p class="error" role="alert">
      <code>
        <Text value={props.problem.code} />
      </code>{' '}
      <Text value={props.problem.sentence} />
    </p>
  );
}

/** The complete native result, collapsed (open on failure). */
function Complete(props: { state: CallState; label?: string }): JSX.Element | null {
  if (props.state.phase !== 'done') return null;
  const failed = props.state.result.kind !== 'ok';
  return (
    <details class="complete" open={failed}>
      <summary>{props.label ?? 'Complete result (JSON)'}</summary>
      <Outcome state={props.state} />
    </details>
  );
}

// ---------------------------------------------------------------------------
// A small, safe Markdown renderer. Supports headings, paragraphs, bullet and
// numbered lists, fenced code, indented blocks (CLI help), inline code, bold
// and https links. Help topics keep one paragraph per line, so each text line
// is its own paragraph.

const INLINE = /(`[^`]+`)|(\*\*[^*]+\*\*)|(https:\/\/[^\s<>()]+[^\s<>().,;:!?'"])/g;

function inline(text: string): ComponentChildren[] {
  const parts: ComponentChildren[] = [];
  let last = 0;
  for (const match of text.matchAll(INLINE)) {
    const index = match.index ?? 0;
    if (index > last) parts.push(...visible(text.slice(last, index)));
    const token = match[0];
    if (match[1] !== undefined) parts.push(<code>{visible(token.slice(1, -1))}</code>);
    else if (match[2] !== undefined) parts.push(<strong>{visible(token.slice(2, -2))}</strong>);
    else
      parts.push(
        <a href={token} target="_blank" rel="noopener noreferrer">
          {visible(token)}
        </a>,
      );
    last = index + token.length;
  }
  if (last < text.length) parts.push(...visible(text.slice(last)));
  return parts;
}

type Block =
  | { kind: 'heading'; level: number; text: string }
  | { kind: 'para'; text: string }
  | { kind: 'list'; ordered: boolean; items: string[] }
  | { kind: 'code'; text: string };

const BULLET = /^\s{0,3}[-*]\s+(.*)$/;
const NUMBERED = /^\s{0,3}\d+[.)]\s+(.*)$/;

function blocks(source: string): Block[] {
  const out: Block[] = [];
  const lines = source.split(/\r?\n/);
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i] ?? '';
    if (line.trim() === '') continue;
    if (/^\s*```/.test(line)) {
      const code: string[] = [];
      for (i++; i < lines.length && !/^\s*```/.test(lines[i] ?? ''); i++) code.push(lines[i] ?? '');
      out.push({ kind: 'code', text: code.join('\n') });
      continue;
    }
    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    if (heading) {
      out.push({ kind: 'heading', level: heading[1]!.length, text: heading[2] ?? '' });
      continue;
    }
    const ordered = NUMBERED.test(line);
    const pattern = ordered ? NUMBERED : BULLET.test(line) ? BULLET : null;
    if (pattern !== null) {
      const items: string[] = [];
      while (i < lines.length && pattern.test(lines[i] ?? '')) {
        items.push(pattern.exec(lines[i] ?? '')?.[1] ?? '');
        i++;
      }
      i--;
      out.push({ kind: 'list', ordered, items });
      continue;
    }
    if (/^( {2,}|\t)/.test(line)) {
      const code: string[] = [];
      while (i < lines.length && (/^( {2,}|\t)/.test(lines[i] ?? '') || (lines[i] ?? '').trim() === '')) {
        code.push(lines[i] ?? '');
        i++;
      }
      i--;
      while (code.length > 0 && code[code.length - 1]!.trim() === '') code.pop();
      out.push({ kind: 'code', text: code.join('\n') });
      continue;
    }
    out.push({ kind: 'para', text: line });
  }
  return out;
}

export function Markdown(props: { source: string }): JSX.Element {
  return (
    <div class="markdown">
      {blocks(props.source).map((block) => {
        switch (block.kind) {
          case 'heading': {
            // Page headings use h2; document headings start at h3.
            const level = Math.min(6, block.level + 2);
            const Tag = `h${level}` as 'h3' | 'h4' | 'h5' | 'h6';
            return <Tag>{inline(block.text)}</Tag>;
          }
          case 'para':
            return <p>{inline(block.text)}</p>;
          case 'list': {
            const items = block.items.map((item) => <li>{inline(item)}</li>);
            return block.ordered ? <ol>{items}</ol> : <ul>{items}</ul>;
          }
          case 'code':
            return <pre class="doc">{visible(block.text)}</pre>;
        }
      })}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Tools: grouping and the input-schema parameter table.

type ToolEntry = { name: string; description: string };

/** Public names drop the single `airk_` compatibility prefix. */
const bare = (name: string): string => (name.startsWith('airk_') ? name.slice(5) : name);

const GROUPS: [string, (name: string) => boolean][] = [
  ['Search and read', (n) => ['search', 'read_file', 'context', 'file_info', 'changes', 'query_scope', 'parse'].includes(n)],
  [
    'Code navigation',
    (n) => ['symbols', 'references', 'code_facts', 'outline', 'imports', 'relationships'].includes(n) || n.startsWith('code'),
  ],
  ['Documents', (n) => n.startsWith('document')],
  ['Status and help', (n) => ['status', 'diagnostics', 'capabilities', 'projects', 'help', 'usage'].includes(n)],
  [
    'Explorer drafts and handoffs',
    (n) =>
      n.startsWith('draft_') ||
      n.endsWith('_template') ||
      n.endsWith('_templates') ||
      ['validate_draft', 'proposal_status', 'get_handoff', 'suggest_view'].includes(n),
  ],
];

function groupOf(name: string): string {
  const n = bare(name);
  return GROUPS.find(([, test]) => test(n))?.[0] ?? 'Other tools';
}

function toolsIn(state: CallState): ToolEntry[] | null {
  const document = str(rec(resultOfDone(state))?.document);
  if (document === null) return null;
  return arr(rec(parseJson(document))?.tools)
    .map((tool) => rec(tool))
    .filter((tool): tool is Rec => tool !== null && typeof tool.name === 'string')
    .map((tool) => ({ name: tool.name as string, description: str(tool.description) ?? '' }));
}

function resultOfDone(state: CallState): unknown {
  return state.phase === 'done' && state.result.kind === 'ok' ? state.result.envelope.result : undefined;
}

/** A JSON-schema type as a short phrase. */
function typeText(schema: Rec | null, defs: Rec | null, depth = 0): string {
  if (schema === null || depth > 4) return 'any';
  const ref = str(schema.$ref);
  if (ref !== null) {
    const name = ref.split('/').pop() ?? ref;
    const target = rec(defs?.[name]);
    return target !== null ? typeText(target, defs, depth + 1) : name;
  }
  const values = arr(schema.enum);
  if (values.length > 0) return `one of ${values.map((v) => JSON.stringify(v)).join(', ')}`;
  if (schema.const !== undefined) return `exactly ${JSON.stringify(schema.const)}`;
  const options = arr(schema.anyOf ?? schema.oneOf);
  if (options.length > 0) {
    const parts = options.map((option) => typeText(rec(option), defs, depth + 1)).filter((t) => t !== 'null');
    return [...new Set(parts)].join(' or ') || 'null';
  }
  const all = arr(schema.allOf);
  if (all.length === 1) return typeText(rec(all[0]), defs, depth + 1);
  const type = schema.type;
  const types = (Array.isArray(type) ? type : [type]).filter((t): t is string => typeof t === 'string');
  const named = types.filter((t) => t !== 'null');
  if (named.includes('array')) return `list of ${typeText(rec(schema.items), defs, depth + 1)}`;
  return named.length > 0 ? named.join(' or ') : 'any';
}

function limitsText(schema: Rec): string[] {
  const facts: string[] = [];
  const min = num(schema.minimum);
  const max = num(schema.maximum);
  if (min !== null && max !== null) facts.push(`${min}–${max}`);
  else if (min !== null) facts.push(`at least ${min}`);
  else if (max !== null) facts.push(`at most ${max}`);
  const maxLength = num(schema.maxLength);
  if (maxLength !== null) facts.push(`up to ${maxLength} characters`);
  const maxItems = num(schema.maxItems);
  if (maxItems !== null) facts.push(`up to ${maxItems} items`);
  if (schema.default !== undefined) facts.push(`default ${JSON.stringify(schema.default)}`);
  return facts;
}

type Param = { name: string; type: string; required: boolean; description: string; limits: string[] };

function paramsOf(schema: Rec | null): Param[] {
  if (schema === null) return [];
  const defs = rec(schema.$defs) ?? rec(schema.definitions);
  const required = new Set(arr(schema.required).filter((r): r is string => typeof r === 'string'));
  const params: Param[] = [];
  const add = (props: Rec | null, req: Set<string>): void => {
    for (const [name, value] of Object.entries(props ?? {})) {
      const p = rec(value) ?? {};
      params.push({
        name,
        type: typeText(p, defs),
        required: req.has(name),
        description: str(p.description) ?? '',
        limits: limitsText(p),
      });
    }
  };
  add(rec(schema.properties), required);
  // Flattened parts (`#[serde(flatten)]`) can appear as allOf members.
  for (const part of arr(schema.allOf)) {
    const resolved = rec(part);
    const ref = str(resolved?.$ref);
    const target = ref !== null ? rec(defs?.[ref.split('/').pop() ?? '']) : resolved;
    add(rec(target?.properties), new Set(arr(target?.required).filter((r): r is string => typeof r === 'string')));
  }
  return params;
}

function ToolDocument(props: { state: CallState }): JSX.Element | null {
  const document = str(rec(resultOfDone(props.state))?.document);
  const tool = document === null ? null : rec(parseJson(document));
  if (tool === null) return null;
  const name = str(tool.name) ?? '';
  const title = str(tool.title);
  const description = str(tool.description) ?? '';
  const params = paramsOf(rec(tool.inputSchema));
  const annotations = rec(tool.annotations);
  const hints = annotations === null
    ? []
    : [
        annotations.readOnlyHint === true ? 'read-only' : null,
        annotations.destructiveHint === true ? 'may change data' : null,
        annotations.idempotentHint === true ? 'idempotent' : null,
        annotations.openWorldHint === false ? 'local only' : null,
      ].filter((h): h is string => h !== null);
  return (
    <article class="help-doc">
      <h3>
        <code>
          <Text value={name} />
        </code>
        {title !== null ? (
          <>
            {' '}
            <Text value={title} />
          </>
        ) : null}
      </h3>
      <p class="muted">
        {groupOf(name)}
        {hints.length > 0 ? ` · ${hints.join(' · ')}` : ''}
      </p>
      <Markdown source={description} />
      <h4>Parameters</h4>
      {params.length === 0 ? (
        <p class="muted">This tool takes no parameters.</p>
      ) : (
        <div class="table-wrap">
          <table class="data">
            <caption class="visually-hidden">Parameters of {name}</caption>
            <thead>
              <tr>
                <th scope="col">Name</th>
                <th scope="col">Type</th>
                <th scope="col">Required</th>
                <th scope="col">Description</th>
              </tr>
            </thead>
            <tbody>
              {params.map((p) => (
                <tr key={p.name}>
                  <th scope="row">
                    <code>
                      <Text value={p.name} />
                    </code>
                  </th>
                  <td>
                    <Text value={p.type} />
                    {p.limits.length > 0 ? (
                      <div class="muted small">
                        <Text value={p.limits.join(' · ')} />
                      </div>
                    ) : null}
                  </td>
                  <td>{p.required ? <span class="badge badge-warn">required</span> : <span class="muted">optional</span>}</td>
                  <td>
                    <Text value={p.description} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </article>
  );
}

// ---------------------------------------------------------------------------
// Help topics: the help tool's reply, with line continuation.

type TopicEntry = { id: string; title: string };
type HelpReply = Rec & { content: string };

/** The help reply JSON inside the tool envelope, or the tool's error code. */
function helpReplyOf(result: CallResult): { reply: HelpReply } | { problem: Problem } {
  const problem = callProblem(result);
  if (problem !== null) return { problem };
  const envelope = rec(rec(resultOfOk(result))?.result);
  const structured = rec(envelope?.structuredContent);
  const text = arr(envelope?.content)
    .map((block) => rec(block))
    .find((block) => block?.type === 'text');
  const value = structured ?? rec(parseJson(str(text?.text) ?? ''));
  const error = rec(value?.error);
  if (envelope?.isError === true || error !== null) {
    const code = str(error?.code) ?? 'help_error';
    return { problem: { code, sentence: FAILURES[code] ?? str(error?.message) ?? 'The help tool returned an error.' } };
  }
  if (value === null || typeof value.content !== 'string') {
    return { problem: { code: 'unexpected_help_reply', sentence: 'The help reply has no readable content.' } };
  }
  return { reply: value as HelpReply };
}

function resultOfOk(result: CallResult): unknown {
  return result.kind === 'ok' ? result.envelope.result : undefined;
}

/** `body.helpTopic` with the largest page budget and a continuation line. */
function helpBody(topic: string, startLine: number): unknown {
  const base = body.helpTopic(topic);
  return {
    tools_call: {
      ...base.tools_call,
      arguments: { ...base.tools_call.arguments, max_bytes: HELP_MAX_BYTES, start_line: startLine },
    },
  };
}

type Viewing = { kind: 'topic'; id: string } | { kind: 'tool'; name: string };

type TopicView = { reply: HelpReply; content: string } | { problem: Problem } | null;

export function Help(): JSX.Element {
  const [catalog, runCatalog] = useCall();
  const [toolHelp, runToolHelp] = useCall();
  const [topicCall, runTopic] = useCall();
  const [topics, setTopics] = useState<TopicEntry[]>([]);
  const [view, setView] = useState<Viewing>({ kind: 'topic', id: '' });
  const [topicView, setTopicView] = useState<TopicView>(null);
  const [filter, setFilter] = useState('');
  // Only the latest topic request may update the reading pane.
  const latest = useRef(0);

  /** Loads a topic from line `start`; a continuation appends to the shown text. */
  const openTopic = async (id: string, start = 1, previous = ''): Promise<void> => {
    setView({ kind: 'topic', id });
    if (start === 1) setTopicView(null);
    const ticket = ++latest.current;
    const answer = helpReplyOf(await runTopic(OPS.searchCall, helpBody(id, start)));
    if (ticket !== latest.current) return;
    if ('problem' in answer) {
      setTopicView(answer);
      return;
    }
    const content = previous === '' ? answer.reply.content : `${previous}\n${answer.reply.content}`;
    setTopicView({ reply: answer.reply, content });
    const listed = arr(answer.reply.topics)
      .map((t) => rec(t))
      .filter((t): t is Rec => t !== null && typeof t.id === 'string')
      .map((t) => ({ id: t.id as string, title: str(t.title) ?? (t.id as string) }));
    if (listed.length > 0) setTopics(listed);
  };
  const openTool = (name: string): void => {
    latest.current++;
    setView({ kind: 'tool', name });
    void runToolHelp(OPS.toolsCatalog, body.toolsHelp(name));
  };

  useEffect(() => {
    void runCatalog(OPS.toolsCatalog, body.toolsList());
    void openTopic('');
  }, []);

  const needle = filter.trim().toLowerCase();
  const matches = (...texts: string[]): boolean => needle === '' || texts.some((t) => t.toLowerCase().includes(needle));
  const shownTopics = topics.filter((t) => t.id !== 'overview' && matches(t.id, t.title));
  const tools = toolsIn(catalog);
  const grouped = new Map<string, ToolEntry[]>();
  for (const tool of tools ?? []) {
    if (!matches(tool.name, tool.description)) continue;
    const group = groupOf(tool.name);
    grouped.set(group, [...(grouped.get(group) ?? []), tool]);
  }
  const groupOrder = [...GROUPS.map(([name]) => name), 'Other tools'].filter((g) => grouped.has(g));
  const catalogProblem = catalog.phase === 'done' ? callProblem(catalog.result) : null;

  return (
    <div class="help-layout">
      <aside class="help-index" aria-label="Help topics and tools">
        <label class="field">
          Filter topics and tools
          <input
            type="search"
            autocomplete="off"
            spellcheck={false}
            placeholder="for example: search, jobs"
            value={filter}
            onInput={(event) => setFilter(event.currentTarget.value)}
          />
        </label>
        <h3>Topics</h3>
        <ul class="help-list">
          <li>
            <button
              type="button"
              class={view.kind === 'topic' && view.id === '' ? 'link current' : 'link'}
              onClick={() => void openTopic('')}
            >
              Overview
            </button>
          </li>
          {shownTopics.map((t) => (
            <li key={t.id}>
              <button
                type="button"
                class={view.kind === 'topic' && view.id === t.id ? 'link current' : 'link'}
                title={t.id}
                onClick={() => void openTopic(t.id)}
              >
                <Text value={t.title} />
              </button>
            </li>
          ))}
        </ul>
        {topics.length > 0 && shownTopics.length === 0 ? <p class="muted small">No topic matches.</p> : null}
        <h3>Tools</h3>
        {catalog.phase === 'pending' ? <p class="muted small">Loading the tool catalog…</p> : null}
        {catalogProblem !== null ? <ProblemLine problem={catalogProblem} /> : null}
        {catalog.phase === 'done' && catalogProblem === null && tools === null ? (
          <p class="error small">The catalog answer has no readable tool list; see its complete result.</p>
        ) : null}
        {groupOrder.map((group) => (
          <div key={group}>
            <h4>{group}</h4>
            <ul class="help-list">
              {(grouped.get(group) ?? []).map((tool) => (
                <li key={tool.name}>
                  <button
                    type="button"
                    class={view.kind === 'tool' && view.name === tool.name ? 'link current' : 'link'}
                    title={tool.description}
                    onClick={() => openTool(tool.name)}
                  >
                    <code>
                      <Text value={tool.name} />
                    </code>
                  </button>
                </li>
              ))}
            </ul>
          </div>
        ))}
        {tools !== null && tools.length > 0 && groupOrder.length === 0 ? <p class="muted small">No tool matches.</p> : null}
        <Complete state={catalog} label="Tool catalog: complete result (JSON)" />
      </aside>

      <section class="panel help-view" aria-live="polite">
        {view.kind === 'topic' ? (
          <TopicPane
            state={topicCall}
            view={topicView}
            onMore={(_reply, content) => {
              if (view.kind === 'topic' && topicView !== null && 'reply' in topicView) {
                void openTopic(view.id, num(topicView.reply.next_line) ?? 1, content);
              }
            }}
          />
        ) : (
          <>
            {toolHelp.phase === 'pending' ? <p class="muted">Loading help for {view.name}…</p> : null}
            {toolHelp.phase === 'done' && callProblem(toolHelp.result) !== null ? (
              <ProblemLine problem={callProblem(toolHelp.result)!} />
            ) : null}
            <ToolDocument state={toolHelp} />
            <Complete state={toolHelp} />
          </>
        )}
      </section>
    </div>
  );
}

function TopicPane(props: {
  state: CallState;
  view: TopicView;
  onMore: (reply: HelpReply, content: string) => void;
}): JSX.Element {
  const v = props.view;
  return (
    <>
      {props.state.phase === 'pending' && v === null ? <p class="muted">Loading help…</p> : null}
      {v !== null && 'problem' in v ? <ProblemLine problem={v.problem} /> : null}
      {v !== null && 'reply' in v ? (
        <article class="help-doc">
          <h2>
            <Text value={str(v.reply.title) ?? str(v.reply.topic) ?? 'Help'} />
          </h2>
          <p class="muted small">
            Topic <code>{visible(str(v.reply.topic) ?? '')}</code>
            {str(v.reply.version) !== null ? ` · AIRK ${str(v.reply.version)}` : ''}
          </p>
          <Markdown source={v.content} />
          {v.reply.truncated === true ? (
            <div class="truncated" role="status">
              Showing lines 1–{num(v.reply.end_line) ?? '?'} of {num(v.reply.total_lines) ?? '?'}.{' '}
              <button
                type="button"
                class="secondary"
                disabled={props.state.phase === 'pending' || num(v.reply.next_line) === null}
                onClick={() => props.onMore(v.reply, v.content)}
              >
                Show the rest
              </button>
            </div>
          ) : null}
        </article>
      ) : null}
      <Complete state={props.state} label="Last help call: complete result (JSON)" />
    </>
  );
}
