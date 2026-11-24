// SPDX-License-Identifier: Apache-2.0
//! Open standalone AIRK Admin in the OS default browser through airkctl.
import { ADMIN_PAGES, type AdminPage } from '../../explorerContracts';
import { AirkctlCommandError, type InvokeResult } from '../session';

export interface AdminTarget {
    readonly page: AdminPage;
    /** Index library: prefill a new index for this folder. */
    readonly forFolder?: string;
    /** Index library: show this index (`idx_…`). */
    readonly showIndex?: string;
    /** Index library: show this index set (`prj_…`). */
    readonly showSet?: string;
}

const INDEX = /^idx_[0-9a-f]{32}$/, SET = /^prj_[0-9a-f]{32}$/;
export class AdminTargetInvalid extends Error {
    readonly code = 'admin_target_invalid';
    constructor(detail: string) { super(`AIRK Admin cannot open this page: ${detail}`); }
}

/** `--page <page>` and the prefill flags, in `airkctl`'s `AdminPage` order. */
export function adminArgs(target: AdminTarget): string[] {
    if (!(ADMIN_PAGES as readonly string[]).includes(target.page)) { throw new AdminTargetInvalid(`unknown page ${String(target.page)}`); }
    const prefill = [target.forFolder, target.showIndex, target.showSet].filter(value => value !== undefined).length;
    if (prefill > 1) { throw new AdminTargetInvalid('choose one folder, index or index set'); }
    if (prefill && target.page !== 'index-library') { throw new AdminTargetInvalid('prefill belongs to the index library'); }
    if (target.forFolder !== undefined && (!target.forFolder || target.forFolder.startsWith('-'))) { throw new AdminTargetInvalid('invalid folder'); }
    if (target.showIndex !== undefined && !INDEX.test(target.showIndex)) { throw new AdminTargetInvalid('invalid index'); }
    if (target.showSet !== undefined && !SET.test(target.showSet)) { throw new AdminTargetInvalid('invalid index set'); }
    return ['--page', target.page,
        ...(target.forFolder !== undefined ? ['--for-folder', target.forFolder] : []),
        ...(target.showIndex !== undefined ? ['--show-index', target.showIndex] : []),
        ...(target.showSet !== undefined ? ['--show-set', target.showSet] : [])];
}

export interface AdminOpenerHost {
    open(args: readonly string[]): Promise<InvokeResult>;
    log(line: string): void;
}

export class AdminOpener {
    constructor(private readonly host: AdminOpenerHost) {}

    async open(target: AdminTarget): Promise<void> {
        const args = adminArgs(target);
        const result = await this.host.open(args);
        if (result.class !== 'success') {
            throw new AirkctlCommandError(['admin', 'open', ...args], { ...result, stdout: '' });
        }
        this.host.log(`AIRK Admin (${target.page}) opened in the browser.`);
    }
}
