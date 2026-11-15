// SPDX-License-Identifier: Apache-2.0
// Editor capability adapter; all bounded review algorithms are shared with explicit browser inputs.
import * as path from 'node:path';
import {createHash} from 'node:crypto';
import * as vscode from 'vscode';
import {caseFold,displayPath,inside} from './explorerSource';
import {createReviewOperations,DEFAULT_TEST_PATTERN,type ChangeKind,type ChangesReview} from './explorerReviewCore';
export * from './explorerReviewCore';
// Minimal shape of the built-in Git extension API (vscode.git, getAPI(1)).
interface GitChange { readonly uri: vscode.Uri; readonly status: number; readonly originalUri?: vscode.Uri }
interface GitRepository { readonly rootUri: vscode.Uri; readonly state: { readonly indexChanges: readonly GitChange[]; readonly workingTreeChanges: readonly GitChange[]; readonly mergeChanges: readonly GitChange[] } }
interface GitApi { readonly repositories: readonly GitRepository[] }

/**
 * vscode.git Status → change kind. 0–4 index, 5–11 working tree (9 intent to add,
 * 10 intent to rename, 11 type changed), 12–18 merge states.
 */
export function kind(status: number): ChangeKind {
    if (status === 7) { return 'untracked'; }
    if ([1, 9].includes(status)) { return 'added'; }
    if ([2, 6].includes(status)) { return 'deleted'; }
    if ([3, 4, 10].includes(status)) { return 'renamed'; }
    if (status >= 12 && status <= 18) { return 'conflicted'; }
    return 'modified';
}

async function gitApi(): Promise<GitApi | undefined> {
    const extension = vscode.extensions.getExtension<{ getAPI(version: 1): GitApi }>('vscode.git');
    if (!extension) { return undefined; }
    try { return (extension.isActive ? extension.exports : await extension.activate()).getAPI(1); } catch { return undefined; }
}

/** Collection-relative paths of saved changes under the project root, from every repository that overlaps it. */
export async function changedPaths(root: string): Promise<{ git: ChangesReview['git']; changes: { path: string; file: string; change: ChangeKind; staged: boolean; workingTree?: boolean; originalPath?: string }[] }> {
    root = displayPath(root);
    const api = await gitApi();
    if (!api) { return { git: 'unavailable', changes: [] }; }
    const repositories = api.repositories.filter(repository => inside(displayPath(repository.rootUri.fsPath), displayPath(root)) || inside(displayPath(root), displayPath(repository.rootUri.fsPath)));
    if (!repositories.length) { return { git: 'no_repository', changes: [] }; }
    const seen = new Map<string, { path: string; file: string; change: ChangeKind; staged: boolean; workingTree?: boolean; originalPath?: string }>();
    for (const repository of repositories) {
        for (const [list, staged] of [[repository.state.mergeChanges, false], [repository.state.indexChanges, true], [repository.state.workingTreeChanges, false]] as const) {
            for (const change of list) {
                const file = displayPath(change.uri.fsPath);
                if (!inside(root, file)) { continue; }
                const relative = path.relative(root, file).split(path.sep).join('/');
                const key = caseFold(relative);
                const prior = seen.get(key);
                const old = change.originalUri?.fsPath;
                const originalPath = old && inside(root, old) ? path.relative(root, old).split(path.sep).join('/') : undefined;
                seen.set(key, { ...(prior ?? { path: relative, file, change: kind(change.status), staged }),
                    workingTree: prior?.workingTree || !staged, ...(originalPath ? { originalPath } : {}) });
            }
        }
    }
    return { git: 'available', changes: [...seen.values()].sort((a, b) => a.path.localeCompare(b.path)) };
}

const dirty = (file: string) => vscode.workspace.textDocuments.some(document => document.uri.scheme === 'file' && caseFold(document.uri.fsPath) === caseFold(file) && document.isDirty);

function testPattern(): RegExp {
    const configured = vscode.workspace.getConfiguration('airk').get<string>('explorer.review.testPathPattern', DEFAULT_TEST_PATTERN);
    try { return new RegExp(typeof configured === 'string' && configured.trim() && configured.length <= 512 ? configured : DEFAULT_TEST_PATTERN); }
    catch { return new RegExp(DEFAULT_TEST_PATTERN); }
}

const operations=createReviewOperations({changedPaths,dirty,testPattern,sha256:text=>createHash('sha256').update(text).digest('hex')});
export const reviewChanges=operations.reviewChanges;
export const reviewImpact=operations.reviewImpact;
export const reviewTests=operations.reviewTests;
