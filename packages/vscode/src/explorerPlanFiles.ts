// SPDX-License-Identifier: Apache-2.0
// Bounded inert plan imports. No general YAML evaluation or instruction execution.
import { ImportedPlan, PLAN_LIMIT_BYTES, PLAN_SCHEMA, PlanTask, validatePlan } from './explorerStrategies';

export type { ImportedPlan } from './explorerStrategies';
export function parsePlanFile(input: string): ImportedPlan {
    if (new TextEncoder().encode(input).length > PLAN_LIMIT_BYTES * 2) { throw new Error('Plan file exceeds 256 KiB.'); }
    const text = input.replace(/^\uFEFF/, '').replaceAll('\r\n', '\n');
    if (text.trimStart().startsWith('{')) {
        const checked = validatePlan(JSON.parse(text));
        if (!checked.ok) { throw new Error(checked.errors.slice(0, 5).join('; ')); }
        return { plan: checked.plan, format: 'json', warnings: ['Imported JSON retains task states and evidence metadata. Verify them against this project before handoff.'] };
    }
    const front = /^---\n([\s\S]*?)\n---\n/.exec(text);
    if (!front || !front[1].split('\n').includes(`schema: ${PLAN_SCHEMA}`)) { throw new Error('Choose a validated AIRK plan JSON file or Markdown saved by AIRK Planner.'); }
    const field = (name: string) => front[1].split('\n').find(line => line.startsWith(`${name}: `))?.slice(name.length + 2);
    const title = JSON.parse(field('title') ?? 'null');
    if (typeof title !== 'string') { throw new Error('The Markdown plan title is missing or invalid.'); }
    const body = text.slice(front[0].length).replace(/^\n/, '');
    const heading = `# ${title}\n\n`, preflight = '\n\n## Before you start\n\n';
    if (!body.startsWith(heading)) { throw new Error('The plan heading and front-matter title disagree; review the file before loading it.'); }
    const objectiveEnd = body.indexOf(preflight, heading.length);
    if (objectiveEnd < 0) { throw new Error('The AIRK plan preflight section is missing.'); }
    const metadata = [...front[1].matchAll(/^  - id: (T[1-9]\d{0,2})\n    depends_on: \[([^\]\n]*)\]\n    status: [a-z_]+\n    evidence: ([^\n]+)$/gm)];
    const declared = [...front[1].matchAll(/^  - id: /gm)];
    const headings = [...body.matchAll(/^## (T[1-9]\d{0,2})\. ([^\n]+)$/gm)];
    if (metadata.length !== declared.length || metadata.length !== headings.length || headings.some((match, at) => match[1] !== metadata[at][1])) {
        throw new Error('Task headings and metadata are ambiguous or disagree. Use a plan JSON export for an exact round trip.');
    }
    const tasks: PlanTask[] = headings.map((match, at) => {
        const metadataDeps = metadata[at][2].split(',').map(value => value.trim()).filter(Boolean);
        const section = body.slice(match.index! + match[0].length, headings[at + 1]?.index ?? body.length);
        const stateLine = /^\n\nDepends on: ([^\n]+) · Status: [a-z_]+\n\n/.exec(section);
        if (!stateLine || stateLine[1] !== (metadataDeps.length ? metadataDeps.join(', ') : 'nothing')) { throw new Error(`${match[1]}: dependencies disagree; review the Markdown file.`); }
        let instructions = section.slice(stateLine[0].length);
        if (metadata[at][3] !== 'none') {
            const evidence = /^Evidence: [^\n]+\n\n/.exec(instructions);
            if (!evidence) { throw new Error(`${match[1]}: the evidence summary is missing.`); }
            instructions = instructions.slice(evidence[0].length);
        }
        instructions = instructions.trim();
        return { id: match[1], title: match[2], instructions: instructions === '(No instructions yet.)' ? '' : instructions, dependsOn: metadataDeps, state: 'draft' };
    });
    const objective = body.slice(heading.length, objectiveEnd);
    const checked = validatePlan({ schema: PLAN_SCHEMA, title, objective: objective === '(No objective recorded.)' ? '' : objective,
        ...(field('project') ? { project: field('project') } : {}), ...(field('publication') ? { publication: JSON.parse(field('publication')!) } : {}), tasks });
    if (!checked.ok) { throw new Error(checked.errors.slice(0, 5).join('; ')); }
    return { plan: checked.plan, format: 'markdown', warnings: ['Markdown contains abbreviated evidence summaries. Tasks were loaded as drafts without verified evidence metadata; review/rebuild evidence before handoff. Use Export plan JSON for an exact state/provenance round trip.'] };
}
