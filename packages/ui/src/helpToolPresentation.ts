// SPDX-License-Identifier: Apache-2.0
import { paramsOf } from './helpSchema';

export type HelpNode = { tag: 'section' | 'h3' | 'p' | 'table' | 'thead' | 'tbody' | 'tr' | 'th' | 'td' | 'code' | 'details' | 'summary' | 'pre'; children: readonly (HelpNode | string)[] };
const node = (tag: HelpNode['tag'], ...children: (HelpNode | string)[]): HelpNode => ({ tag, children });
const rec = (value: unknown): Record<string, unknown> | null => value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : null;
/** The native tool remains opaque and complete; this is a labeled reading aid. */
export function helpToolPresentation(value: unknown): HelpNode {
  const tool = rec(value), parameters = paramsOf(rec(tool?.inputSchema));
  return node('section', node('h3', typeof tool?.name === 'string' ? tool.name : 'Tool reference'),
    node('p', typeof tool?.description === 'string' ? tool.description : ''),
    node('p', 'Bundled public tool schema. Required and nullable describe separate constraints. Complete schemas, including nested constraints and annotations, remain below.'),
    node('table', node('thead', node('tr', ...['Parameter', 'Required', 'Type', 'Description and limits'].map(label => node('th', label)))),
      node('tbody', ...parameters.map(parameter => node('tr', node('td', node('code', parameter.name)),
        node('td', parameter.required ? 'required' : 'optional'), node('td', parameter.type),
        node('td', [parameter.description, ...parameter.limits].filter(Boolean).join(' · ')))))),
    node('details', node('summary', 'Complete native tool definition (JSON)'), node('pre', JSON.stringify(value, null, 2))));
}
export function renderHelpNode<T>(create: (tag: HelpNode['tag'], children: readonly (T | string)[]) => T, tree: HelpNode): T {
  return create(tree.tag, tree.children.map(child => typeof child === 'string' ? child : renderHelpNode(create, child)));
}
