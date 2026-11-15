// SPDX-License-Identifier: Apache-2.0
// Full inert native evidence export shared by guided and general reads.
export function completeEvidenceMarkdown(original: unknown, request: unknown): string {
  const raw = JSON.stringify(original, null, 2), input = JSON.stringify(request, null, 2);
  const fence = '`'.repeat(Math.max(3, ...[...`${raw}${input}`.matchAll(/`+/g)].map(match => match[0].length + 1)));
  return `# AIRK indexed evidence\n\nComplete original request:\n\n${fence}json\n${input}\n${fence}\n\nComplete original native response (including limits and provenance):\n\n${fence}json\n${raw}\n${fence}\n`;
}
