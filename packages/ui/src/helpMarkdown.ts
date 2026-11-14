// SPDX-License-Identifier: Apache-2.0
// Pure Markdown block parser shared by browser and editor; no HTML interpretation.
export type Block =
  | { kind: 'heading'; level: number; text: string }
  | { kind: 'para'; text: string }
  | { kind: 'list'; ordered: boolean; start?: number; items: { text: string; children: Block[] }[] }
  | { kind: 'table'; headers: string[]; rows: string[][] }
  | { kind: 'code'; text: string };

const MARKER = /^( *)([-*]|\d+[.)])\s+(.*)$/;
const indent = (line: string): number => /^ */.exec(line)?.[0].length ?? 0;
function cells(line: string): string[] {
  const text = line.trim().replace(/^\|/, '').replace(/\|$/, ''), values: string[] = [];
  let current = '', code = false, escaped = false;
  for (const character of text) {
    if (character === '|' && !code && !escaped) { values.push(current.trim()); current = ''; }
    else { current += character; if (character === '`' && !escaped) code = !code; }
    if (character === '\\' && !escaped) escaped = true; else escaped = false;
  }
  values.push(current.trim());
  return values;
}

export function blocks(source: string, depth = 0): Block[] {
  // Keep the complete text even for pathological nesting; never recurse without bound.
  if (depth >= 64) return [{ kind: 'code', text: source }];
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
    const marker = MARKER.exec(line);
    if (marker !== null) {
      const ordered = /^\d/.test(marker[2]!), base = marker[1]!.length;
      const items: { text: string; children: Block[] }[] = [];
      while (i < lines.length) {
        let at = i;
        while (at < lines.length && !(lines[at] ?? '').trim()) at++;
        const item = MARKER.exec(lines[at] ?? '');
        if (item === null || item[1]!.length !== base || /^\d/.test(item[2]!) !== ordered) break;
        i = at;
        const nested: string[] = [];
        i++;
        while (i < lines.length) {
          const next = lines[i] ?? '';
          if (!next.trim()) {
            if (indent(lines[i + 1] ?? '') <= base) break;
            nested.push(''); i++; continue;
          }
          if (indent(next) <= base) break;
          nested.push(next.slice(Math.min(indent(next), base + 2))); i++;
        }
        items.push({ text: item[3] ?? '', children: nested.length ? blocks(nested.join('\n'), depth + 1) : [] });
      }
      i--;
      out.push({ kind: 'list', ordered, ...(ordered ? { start: Number.parseInt(marker[2]!, 10) } : {}), items });
      continue;
    }
    const separator = lines[i + 1];
    if (line.includes('|') && separator !== undefined && cells(separator).every(cell => /^:?-{3,}:?$/.test(cell))) {
      const headers = cells(line), rows: string[][] = [];
      i += 2;
      while (i < lines.length && (lines[i] ?? '').includes('|') && (lines[i] ?? '').trim()) rows.push(cells(lines[i++]!));
      i--;
      out.push({ kind: 'table', headers, rows });
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
