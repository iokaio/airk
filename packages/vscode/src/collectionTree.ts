// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';

export class CollectionItem extends vscode.TreeItem {
    constructor(readonly collection: string, readonly dataDir: string, readonly active: boolean, removable: boolean, name: string,
        state: string, coverage: string, bytes: number | undefined, lastIndexed: string | undefined, root: string | undefined) {
        super(name, vscode.TreeItemCollapsibleState.None);
        this.contextValue = active ? 'airkActiveCollection' : removable ? 'airkInactiveCollection' : 'airkUnavailableCollection';
        const size = bytes == null ? 'size unknown' : `${(bytes / 1048576).toFixed(2)} MiB`;
        const last = lastIndexed ? new Date(lastIndexed).toLocaleString() : 'never';
        this.description = `${state} · ${coverage} · ${size} · indexed ${last}`;
        this.tooltip = `${name}\n${root ?? collection}\nState: ${state}\nCoverage: ${coverage}\nDisk use: ${size}\nLast indexed: ${last}`;
        this.iconPath = new vscode.ThemeIcon(state === 'paused' ? 'debug-pause' : state === 'indexed' ? 'database' : 'clock');
    }
}
export class CollectionTree implements vscode.TreeDataProvider<CollectionItem> {
    readonly changed = new vscode.EventEmitter<void>();
    readonly onDidChangeTreeData = this.changed.event;
    constructor(private readonly controller: { collectionTreeItems(): Promise<CollectionItem[]> }) {}
    getTreeItem(item: CollectionItem): vscode.TreeItem { return item; }
    getChildren(): Promise<CollectionItem[]> { return this.controller.collectionTreeItems(); }
    refresh(): void { this.changed.fire(); }
    dispose(): void { this.changed.dispose(); }
}
