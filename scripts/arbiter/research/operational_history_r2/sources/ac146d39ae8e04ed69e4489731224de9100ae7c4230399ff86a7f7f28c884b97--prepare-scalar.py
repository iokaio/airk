# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib
root=Path.cwd();base=Path(__file__).parent
names=['packages/vscode/src/explorerController.ts','packages/vscode/src/explorerStatus.ts','packages/vscode/src/test/explorerStatus.test.ts','packages/vscode/src/explorerContracts.ts','packages/vscode/src/explorerWebview/manage.tsx','packages/vscode/src/explorerWebview/overview.tsx','packages/vscode/src/explorerWebview/app.tsx','packages/vscode/src/explorerWebview/search.tsx','packages/vscode/src/explorerManagement.ts','packages/vscode/src/test/explorerShell.test.ts']
patch=[]
for name in names:
 old=(root/name).read_bytes().decode();ending='\r\n' if '\r\n' in old else '\n';new=old.replace('\r\n','\n')
 if name.endswith('explorerController.ts'):
  new=new.replace("import { headline, statusSummary }", "import { headline, statusSummary, scalarWithRetainedDetails }")
  new=new.replace('    private statusLoading?: Promise<void>;', """    private statusLoading?: Promise<void>;
    private statusLoadingKind?: 'scalar' | 'details';
    private retainedDetails?: { project: string; binding: string; identity?: string; summary: StatusSummary; settings?: string };""")
  new=new.replace("""    private loadStatus(): Promise<void> {
        if (this.statusLoading || this.disposed) { return this.statusLoading ?? Promise.resolve(); }""", """    private loadStatus(kind: 'scalar' | 'details' = 'scalar'): Promise<void> {
        if (this.disposed) { return Promise.resolve(); }
        if (this.statusLoading) {
            if (kind !== 'details' || this.statusLoadingKind !== 'scalar') { return this.statusLoading; }
            const project = this.selected, epoch = this.selectionEpoch, identity = this.control.observationIdentity;
            const generation = this.panel.generation;
            const spec = JSON.stringify(this.host.specs().find(entry => entry.service?.project === project));
            return this.statusLoading.then(() => {
                if (!this.disposed && this.host.trusted() && this.panel.open && generation === this.panel.generation
                    && project === this.selected && epoch === this.selectionEpoch && identity === this.control.observationIdentity
                    && spec === JSON.stringify(this.host.specs().find(entry => entry.service?.project === project))) {
                    return this.loadStatus('details');
                }
            });
        }""")
  new=new.replace("    private loadStatus(kind:", """    private detailSettingsKey(project: string): string | undefined {
        const object = (value: unknown): Record<string, unknown> => value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : {};
        const state = this.control.state;
        const page = state.inactiveProjects ? state.activePage : state.page;
        const row = page?.projects.find(entry => object(object(entry.project).registration).binding === project);
        const snapshot = object(object(row?.inspection).snapshot);
        return typeof snapshot.incarnation === 'string' && typeof snapshot.settings_digest === 'string'
            ? JSON.stringify([snapshot.incarnation, snapshot.settings_digest, snapshot.vectors_enabled]) : undefined;
    }

    private loadStatus(kind:""")
  new=new.replace('        this.statusLoading = (async () => {', """        this.statusLoadingKind = kind;
        this.statusLoading = (async () => {""",1)
  new=new.replace('        const identity = this.control.observationIdentity;', '        const identity = this.control.observationIdentity;\n        const settings = this.detailSettingsKey(project);',1)
  new=new.replace('&& identity === this.control.observationIdentity && epoch === this.selectionEpoch', "&& identity === this.control.observationIdentity && epoch === this.selectionEpoch\n            && (kind !== 'details' || settings === this.detailSettingsKey(project))",1)
  new=new.replace("""const result = await this.session().call('airk_diagnostics', { response_mode: 'full' }, { timeout: 90000, supersede: 'status' });
                if (current()) { this.status = statusSummary(result.payload); this.statusFor = project; this.statusIdentity = identity; this.notice = undefined; }""", """const result = await this.session().call(kind === 'details' ? 'airk_diagnostics' : 'airk_status',
                    { response_mode: 'full' }, { timeout: kind === 'details' ? 90000 : 15000, supersede: 'status' });
                if (current()) {
                    const raw = result.payload.observed_unix_ms;
                    const received = Date.now();
                    const observed = kind === 'scalar' && typeof raw === 'number'
                        && Number.isSafeInteger(raw) && raw >= 0 ? Math.min(raw, received) : received;
                    const summary = statusSummary(result.payload, observed);
                    if (kind === 'details') { this.retainedDetails = { project, binding, identity, summary: { ...summary, detailsObservedAt: observed }, settings }; }
                    const prior = this.retainedDetails;
                    const detail = prior?.project === project && prior.binding === binding && prior.identity === identity
                        ? prior.summary : undefined;
                    this.status = kind === 'details' ? { ...summary, detailsObservedAt: observed } : scalarWithRetainedDetails(summary, detail, prior?.settings === this.detailSettingsKey(project));
                    this.statusFor = project; this.statusIdentity = identity; this.notice = undefined;
                }""")
  new=new.replace('                this.statusLoading = undefined;', '                this.statusLoading = undefined; this.statusLoadingKind = undefined;',1)
  new=new.replace("case 'status.refresh': await Promise.all([this.loadStatus(), this.control.refresh()]);", "case 'status.refresh': await Promise.all([this.loadStatus('details'), this.control.refresh()]);")
 elif name.endswith('explorerStatus.test.ts'):
  new=new.replace('headline, indexContents,', 'scalarWithRetainedDetails, headline, indexContents,')
  new+='\n'+base.joinpath('scalar-detail-tests.ts').read_text()+'\n'
 elif name.endswith('explorerContracts.ts'):
  new=new.replace('    readonly observedAt: number;\n}', '    readonly observedAt: number;\n    /** Independent time for retained runtime/configuration details; not scalar cache age. */\n    readonly detailsObservedAt?: number;\n}',1)
 elif name.endswith('manage.tsx'):
  new=new.replace('props.state.status ? `Project runtime observed ${ago(props.state.status.observedAt)}.`', '(props.state.status?.runtimeVersions || props.state.status?.versionsMatch !== undefined) ? `Project runtime details observed ${ago(props.state.status.detailsObservedAt ?? props.state.status.observedAt)}.`')
 elif name.endswith('overview.tsx'):
  new=new.replace("['Published status observed', ago(status?.observedAt)],", "['Published status observed', ago(status?.observedAt)],\n                    ['Detailed observations', ago(status?.detailsObservedAt)],")
 elif name.endswith('search.tsx'):
  new=new.replace('import { countLine, shortToken }', 'import { ago, countLine, shortToken }')
  new=new.replace('{vectorReason && <p class="muted">{vectorReason}</p>}', '{vectorReason && <p class="muted">{vectorReason}</p>}\n            {status?.detailsObservedAt !== undefined && (status.vectorsEnabled !== undefined || status.localModelReady !== undefined) && <p class="muted">Saved policy details observed {ago(status.detailsObservedAt)}.</p>}')
 elif name.endswith('app.tsx'):
  new=new.replace("['Observed', ago(status?.observedAt)],", "['Published status observed', ago(status?.observedAt)],\n                ['Detailed observations', ago(status?.detailsObservedAt)],")
 elif name.endswith('explorerManagement.ts'):
  new=new.replace("['Publication',status?.publication??'Not observed'],['Observed',status?new Date(status.observedAt).toISOString():'Not observed']", "['Publication',status?.publication??'Not observed'],['Published status observed',status?new Date(status.observedAt).toISOString():'Not observed'],\n            ['Detailed observations',status?.detailsObservedAt===undefined?'Not observed':new Date(status.detailsObservedAt).toISOString()]")
 elif name.endswith('explorerShell.test.ts'):
  new=new.replace("assert.equal(tool, 'airk_diagnostics');", "assert.ok(tool === 'airk_status' || tool === 'airk_diagnostics', 'only scalar readiness or explicit details are expected');")
  new=new.replace("assert.equal(tool,'airk_diagnostics');", "assert.ok(tool==='airk_status'||tool==='airk_diagnostics','only scalar readiness or explicit details are expected');")
  new=new.replace("evidence:Array<{project:string;spec:any;result:", "evidence:Array<{tool:string;project:string;spec:any;result:")
  new=new.replace("const pending={project:this.project,spec:", "const pending={tool,project:this.project,spec:")
  new=new.replace("        restartIdentity(){", "        settings(value:string){(page.projects[0] as any).inspection={observed_unix_ms:Date.now(),snapshot:{project,incarnation:'11'.repeat(16),settings_digest:value,vectors_enabled:true}};},\n        restartIdentity(){")
  new+='\n'+base.joinpath('scalar-host-tests.ts').read_text()+'\n'
 else:
  new+= '\n'+base.joinpath('scalar-detail-helper.ts').read_text()+'\n'
 new=new.replace('\n',ending)
 out=base/'source'/name;out.parent.mkdir(parents=True,exist_ok=True);out.write_bytes(new.encode())
 patch.extend(difflib.unified_diff(old.splitlines(True),new.splitlines(True),fromfile='a/'+name,tofile='b/'+name))
base.joinpath('scalar-status-foundation.patch').write_bytes(''.join(patch).encode())
