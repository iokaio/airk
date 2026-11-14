// SPDX-License-Identifier: Apache-2.0
// Presentation of complete native JSON Schema; this never validates or changes requests.
type Rec = Record<string, unknown>;
const rec = (value: unknown): Rec | null => typeof value === 'object' && value !== null && !Array.isArray(value) ? value as Rec : null;
const str = (value: unknown): string | null => typeof value === 'string' ? value : null;
const num = (value: unknown): number | null => typeof value === 'number' && Number.isFinite(value) ? value : null;
const arr = (value: unknown): unknown[] => Array.isArray(value) ? value : [];

/** A JSON-schema type as a short phrase. */
export function typeText(schema: Rec | null, defs: Rec | null, depth = 0): string {
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
    const parts = options.map((option) => typeText(rec(option), defs, depth + 1));
    return [...new Set(parts)].join(' or ') || 'null';
  }
  const all = arr(schema.allOf);
  if (all.length === 1) return typeText(rec(all[0]), defs, depth + 1);
  const type = schema.type;
  const types = (Array.isArray(type) ? type : [type]).filter((t): t is string => typeof t === 'string');
  return types.length > 0 ? types.map(type => type === 'array' ? `list of ${typeText(rec(schema.items), defs, depth + 1)}` : type).join(' or ') : 'any';
}

export function limitsText(schema: Rec): string[] {
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

export type Param = { name: string; type: string; required: boolean; description: string; limits: string[] };

export function paramsOf(schema: Rec | null): Param[] {
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
  return params.sort((left, right) => Number(right.required) - Number(left.required));
}
