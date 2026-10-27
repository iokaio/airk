// SPDX-License-Identifier: Apache-2.0
// Explorer help served from the catalog bundled with this VSIX. Nothing here reads
// the workspace, the index, the network or another process.
import { createHash } from 'node:crypto';
import type { HelpCatalog, HelpTopic } from '../explorerContracts';
import { WORKSPACES } from '../explorerRoutes';

export const SEARCH_LIMIT = { default: 5, max: 20 } as const;
export const SEARCH_BYTES = { default: 4096, min: 512, max: 32768 } as const;
export const DETAILS = ['summary', 'steps', 'full'] as const;
export type Detail = typeof DETAILS[number];

const tokens = (text: string) => text.toLowerCase().match(/[a-z0-9]+/g) ?? [];

/** "Management › Repair" for a route key; Help is a drawer, Overview has no tab. */
export function location(route: string | undefined): string | undefined {
    if (!route) { return undefined; }
    const [workspaceId, tabId] = route.split('/');
    const workspace = WORKSPACES.find(entry => entry.id === workspaceId);
    const tab = workspace?.tabs.find(entry => entry.id === tabId);
    return workspace ? ['AIRK Explorer', workspace.title, ...(tab ? [tab.title] : [])].join(' › ') : undefined;
}

/** First prose paragraph after the title heading. */
export function summary(body: string): string {
    const paragraph = body.split(/\n\s*\n/).map(part => part.trim()).find(part => part && !part.startsWith('#')) ?? '';
    return paragraph.replace(/\s+/g, ' ').slice(0, 400);
}

/** Ordered items of the "## Steps" section, when the topic has one. */
export function steps(body: string): string[] {
    const section = /(?:^|\n)## Steps\n([\s\S]*?)(?=\n## |\s*$)/.exec(body)?.[1] ?? '';
    return [...section.matchAll(/^\d+\.\s+(.+)$/gm)].map(match => match[1].trim());
}

export class HelpIndex {
    readonly areas: readonly string[];
    private readonly revisions = new Map<string, string>();
    private readonly words = new Map<string, { title: Set<string>; aliases: Set<string>; body: Map<string, number> }>();

    constructor(readonly catalog: HelpCatalog) {
        this.areas = [...new Set(catalog.topics.map(topic => topic.area))].sort();
        for (const topic of catalog.topics) {
            this.revisions.set(topic.id, createHash('sha256').update(JSON.stringify(topic)).digest('hex').slice(0, 16));
            const body = new Map<string, number>();
            for (const word of tokens(topic.body)) { body.set(word, (body.get(word) ?? 0) + 1); }
            this.words.set(topic.id, { title: new Set(tokens(topic.title)), aliases: new Set(topic.aliases.flatMap(tokens)), body });
        }
    }

    topic(id: string): HelpTopic | undefined { return this.catalog.topics.find(topic => topic.id === id); }
    revision(id: string): string { return this.revisions.get(id) ?? ''; }

    /**
     * Deterministic lexical ranking: id and alias phrases first, then title, alias and
     * bounded body term hits. Ties order by topic id, so equal input gives equal output.
     */
    search(query: string, area?: string): { topic: HelpTopic; score: number }[] {
        const phrase = query.trim().toLowerCase();
        const terms = [...new Set(tokens(phrase))];
        const scored = this.catalog.topics.filter(topic => !area || topic.area === area).map(topic => {
            const words = this.words.get(topic.id)!;
            let score = topic.id === phrase ? 100 : 0;
            if (topic.aliases.some(alias => alias.toLowerCase() === phrase)) { score += 40; }
            if (topic.title.toLowerCase().includes(phrase)) { score += 20; }
            for (const term of terms) {
                if (words.title.has(term)) { score += 8; }
                if (words.aliases.has(term)) { score += 6; }
                if (topic.id.split('.').includes(term)) { score += 4; }
                score += Math.min(words.body.get(term) ?? 0, 3);
            }
            return { topic, score };
        });
        return scored.filter(entry => entry.score > 0).sort((a, b) => b.score - a.score || (a.topic.id < b.topic.id ? -1 : 1));
    }
}
