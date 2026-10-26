// SPDX-License-Identifier: Apache-2.0
import { useState } from 'preact/hooks';
import { request } from './api';
import { ago, observed, shortToken } from './format';
import type { ScreenProps } from './workspaces';

export function ProjectsScreen(props: ScreenProps) {
    const { state } = props;
    const [pending, setPending] = useState<string>();
    const [outcome, setOutcome] = useState<string>();
    const index = async (project: string) => {
        setPending(project); setOutcome(undefined);
        try {
            const result = await request<{ message: string | null }>('manage.index', { project });
            setOutcome(result.message ? `AIRK reported: ${result.message}` : 'Index request accepted. Acceptance means work is queued; it does not mean indexing is finished.');
        } catch (failure) { setOutcome(failure instanceof Error ? failure.message : String(failure)); }
        finally { setPending(undefined); }
    };
    const select = (project: string) => { void request('project.select', { project }).catch(() => undefined); };
    if (!state.trusted) { return <p class="callout">Trust this workspace before AIRK shows or manages registered projects.</p>; }
    return (
        <div class="projects">
            <div class="section-head">
                <p class="muted">Cached native observations · observed {ago(state.projectsObservedAt)}. Published counts update after inspect or configuration jobs; the Overview shows the live published status of the selected project.</p>
                <button type="button" class="ghost" onClick={() => { void request('manage.refresh', {}).catch(() => undefined); }}>Refresh</button>
            </div>
            {outcome && <p class="callout" role="status">{outcome}</p>}
            {state.controlMessage && !outcome && <p class="muted" role="status">Last control outcome: <code>{state.controlMessage}</code></p>}
            {!state.projects.length && <p class="callout">No registered project is visible. Trusted workspace folders register automatically; the service may still be starting.</p>}
            {state.projects.length > 0 && (
                <div class="table-wrap">
                    <table class="grid">
                        <thead><tr>
                            <th scope="col">Project</th><th scope="col">Source root</th><th scope="col">Watching</th><th scope="col">Paused</th>
                            <th scope="col">Publication (cached)</th><th scope="col">Files (cached)</th><th scope="col">Semantic (cached)</th>
                            <th scope="col">Current job</th><th scope="col">Inspected</th><th scope="col"><span class="sr-only">Actions</span></th>
                        </tr></thead>
                        <tbody>
                            {state.projects.map(project => (
                                <tr key={project.project} class={project.project === state.selectedProject ? 'selected' : ''}>
                                    <th scope="row">{project.collection}{project.current && <span class="tag">open here</span>}</th>
                                    <td><code class="path">{project.root || 'not observed'}</code></td>
                                    <td>{observed(project.watching)}</td>
                                    <td>{observed(project.paused)}</td>
                                    <td title={project.publication}>{shortToken(project.publication)}</td>
                                    <td>{observed(project.indexedFiles)}</td>
                                    <td>{project.semantic ?? 'not observed'}</td>
                                    <td>{project.jobState ? `${project.jobKind ?? 'job'} · ${project.jobState}${project.phase ? ` · ${project.phase}` : ''}` : 'none observed'}</td>
                                    <td>{ago(project.observedAt)}</td>
                                    <td class="actions">
                                        {project.project !== state.selectedProject && <button type="button" class="ghost" onClick={() => select(project.project)}>Select</button>}
                                        <button type="button" disabled={pending !== undefined} onClick={() => { void index(project.project); }}>
                                            {pending === project.project ? 'Requesting…' : 'Index now'}</button>
                                    </td>
                                </tr>
                            ))}
                        </tbody>
                    </table>
                </div>
            )}
            {state.moreProjects && <p class="muted">More registered projects exist beyond this cached page.</p>}
        </div>
    );
}
