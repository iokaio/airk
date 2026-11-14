// SPDX-License-Identifier: Apache-2.0
// Search presentation metadata and complete authored/native article text.
// This UI model owns no native request or response shape.
export function helpScore(topic: { id: string; title: string; aliases: readonly string[]; body: string }, query: string): number {
  const phrase = query.trim().toLowerCase(), terms = [...new Set(phrase.match(/[\p{L}\p{N}]+/gu) ?? [])];
  if (!terms.length) return 0;
  const title = topic.title.toLowerCase(), aliases = topic.aliases.join(' ').toLowerCase(), body = topic.body.toLowerCase();
  return (topic.id === phrase ? 40 : 0) + (title.includes(phrase) ? 24 : 0) + (aliases.includes(phrase) ? 16 : 0)
    + terms.reduce((score, term) => score + (title.includes(term) ? 8 : 0) + (aliases.includes(term) ? 6 : 0) + (body.includes(term) ? 1 : 0), 0);
}

/** A labeled preview; the selected article always retains the complete body. */
export function helpExcerpt(body: string, query: string): string {
  const text = body.replace(/\s+/g, ' '), lower = text.toLowerCase(), phrase = query.trim().toLowerCase();
  const terms = [phrase, ...(phrase.match(/[\p{L}\p{N}]+/gu) ?? [])].filter(Boolean);
  const position = terms.map(term => lower.indexOf(term)).find(index => index >= 0) ?? 0;
  const points = [...text], index = [...text.slice(0, position)].length, start = Math.max(0, index - 40), end = start + 160;
  return `${start ? '…' : ''}${points.slice(start, end).join('')}${end < points.length ? '…' : ''}`;
}
