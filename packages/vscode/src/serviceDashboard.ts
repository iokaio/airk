// SPDX-License-Identifier: Apache-2.0
/** Setup guidance uses registration observations; it never implies index readiness. */
export function renderServiceDashboard(trusted:boolean,folders:number,registered:number):string {
    const prerequisite=!trusted?'Trust this workspace in VS Code to allow AIRK setup.':
        folders===0?'Open a project folder in VS Code to get started.':'';
    return `<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<style>body{box-sizing:border-box;width:465px;max-width:100%;overflow-wrap:anywhere;font-family:var(--vscode-font-family);color:var(--vscode-foreground);padding:12px 16px;line-height:1.5}h1{font-size:24px}h2{font-size:16px;margin-bottom:4px}a{color:var(--vscode-textLink-foreground)}.primary{display:block;background:var(--vscode-button-background);color:var(--vscode-button-foreground);padding:8px 12px;text-align:center;text-decoration:none;margin:16px 0}.note{color:var(--vscode-descriptionForeground)}li{margin-bottom:14px}ol{padding-left:22px}</style>
</head><body><h1>${registered?'Your AIRK workspace':'Get started with AIRK'}</h1>
<p>Local code and document search for your coding agent.</p>
${prerequisite?`<p role="status">${prerequisite}</p>`:
    registered?`<p role="status">${registered} workspace project${registered===1?' is':'s are'} registered. Check the Service Console for indexing progress and readiness.</p><a class="primary" href="command:airk.serviceConnect">View indexing progress</a>`:
    '<p>AIRK automatically sets up trusted workspace folders and starts indexing. If automatic setup is disabled or failed, you can retry below.</p><a class="primary" href="command:airk.installCli">Install or repair AIRK</a>'}
<h2>Setup is automatic</h2><p>AIRK installs the runtime and terminal command, prepares the offline model, and starts indexing new trusted workspace folders. Existing 1.4 storage and watching selections are kept on upgrade.</p>
${!prerequisite&&registered?'<a href="command:airk.index">Index now / set up another workspace project</a>':''}<p class="note">Job acceptance means work is queued; it does not mean indexing is finished. The Service Console shows progress, readiness and failures.</p>
<h2>Next: connect your coding agent</h2><p><a href="command:airk.setupAgent">Connect a coding agent</a>, then enable or reload its AIRK MCP connection. Ask it to use AIRK to explore this project.</p><h2>Your settings</h2><p><a href="command:airk.chooseStorage">Change index storage location</a><br><a href="command:airk.indexMode">Change automatic watching</a></p><p class="note">Storage changes review a new binding; existing index files stay available for separate cleanup.</p>
<p class="note">Watching continues after editor windows close. Start at login is a separate opt-in setting.</p>
<h2>Terminal is ready in a new session</h2><p>Open a new VS Code terminal and run <code>airk --version</code>. Restart external terminals to refresh PATH.</p><details><summary>Installation help</summary><p>Use <a href="command:airk.installCli">Install or Repair CLI</a> if setup failed. Check command selection with <code>Get-Command airk -All</code> (PowerShell) or <code>type -a airk</code> (Bash/Zsh). The User setting <code>airk.cli.autoInstall: false</code> disables automatic installation and registration.</p></details>
<details><summary>Upgrading from 1.3?</summary><p>1.4 builds fresh indexes and needs additional disk space. Old indexes and agent entries remain for separate review; they are not converted automatically.</p></details>
</body></html>`;
}
