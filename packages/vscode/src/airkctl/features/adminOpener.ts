// SPDX-License-Identifier: Apache-2.0
//! Open AIRK Admin (U97) on a page, either in the default browser
//! (`airkctl admin open`, the sign-in code never reaches the extension) or in
//! VS Code's Simple Browser (`airkctl admin url --output json`). The single-use
//! URL stays in this call only: it is never logged, stored, posted to a
//! webview or reused; a fallback to the browser asks `airkctl` for a new
//! sign-in instead. Plain http is never offered.
import { ADMIN_PAGES, type AdminPage } from '../../explorerContracts';
import { AirkctlCommandError, type InvokeResult } from '../session';

export type AdminOpenIn = 'vscode' | 'external';
export const ADMIN_OPEN_IN_SETTING = 'admin.openIn';
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

/** The `admin url --output json` document's URL; only `https:` is accepted. */
export function adminUrl(stdout: string): string | undefined {
    try {
        const value = JSON.parse(stdout) as { url?: unknown };
        if (typeof value?.url !== 'string' || value.url.length > 8192) { return undefined; }
        return new URL(value.url).protocol === 'https:' ? value.url : undefined;
    } catch { return undefined; }
}

export interface AdminOpenerHost {
    /** `airkctl admin open <args>` (one-shot on the verified image). */
    open(args: readonly string[]): Promise<InvokeResult>;
    /** `airkctl admin url <args> --output json` (one-shot on the verified image). */
    url(args: readonly string[]): Promise<InvokeResult>;
    /** `airk.admin.openIn` (default `external`). */
    setting(): AdminOpenIn;
    remember(choice: AdminOpenIn): Promise<void>;
    /** Ask "In VS Code" / "In my browser"; undefined when dismissed. */
    choose(current: AdminOpenIn): Promise<AdminOpenIn | undefined>;
    /** `simpleBrowser.show`. */
    showInVsCode(url: string): Promise<void>;
    /** Offer "Open in my browser" with `message`; true when chosen. */
    offerBrowser(message: string): Promise<boolean>;
    log(line: string): void;
}

export class AdminOpener {
    /** The certificate follow-up offer of the last VS Code open (tests await it). */
    followUp: Promise<void> = Promise.resolve();
    constructor(private readonly host: AdminOpenerHost) {}

    /** Open `target` where `airk.admin.openIn` says; `choose` asks "In VS Code" / "In my browser" and remembers the answer. */
    async open(target: AdminTarget, choose = false): Promise<AdminOpenIn | undefined> {
        const args = adminArgs(target);
        let where: AdminOpenIn | undefined = this.host.setting();
        if (choose) {
            where = await this.host.choose(where);
            if (!where) { return undefined; }
            await this.host.remember(where);
        }
        if (where === 'external') { await this.external(args, target.page); return where; }
        const argv = ['admin', 'url', ...args, '--output', 'json'];
        const result = await this.host.url(args);
        // The URL is a secret: no channel of a failed or odd result is kept.
        if (result.class !== 'success') { throw new AirkctlCommandError(argv, { ...result, stdout: '' }); }
        const url = adminUrl(result.stdout);
        if (!url) {
            this.host.log(`AIRK Admin (${target.page}): airkctl admin url did not return an https URL.`);
            if (await this.host.offerBrowser('AIRK Admin could not be opened in VS Code: no HTTPS sign-in link was returned.')) { await this.external(args, target.page); }
            return where;
        }
        try {
            await this.host.showInVsCode(url);
        } catch {
            this.host.log(`AIRK Admin (${target.page}): the VS Code browser could not open it.`);
            if (await this.host.offerBrowser('AIRK Admin could not be opened in VS Code.')) { await this.external(args, target.page); }
            return where;
        }
        this.host.log(`AIRK Admin (${target.page}) opened in VS Code.`);
        this.followUp = this.host.offerBrowser('AIRK Admin opened in VS Code. If it reports a certificate problem, open it in your browser.')
            .then(chosen => chosen ? this.external(args, target.page) : undefined)
            .catch(error => this.host.log(`AIRK Admin: ${error instanceof Error ? error.message : String(error)}`));
        return where;
    }

    private async external(args: readonly string[], page: AdminPage): Promise<void> {
        const result = await this.host.open(args);
        if (result.class !== 'success') { throw new AirkctlCommandError(['admin', 'open', ...args], result); }
        this.host.log(`AIRK Admin (${page}) opened in the browser.`);
    }
}
