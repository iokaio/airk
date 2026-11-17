// SPDX-License-Identifier: Apache-2.0
// Private read-only capability observation; credentials remain only in memory.
const fs = require('node:fs');
const { getAzureCredentialAccessToken } = require('../../packages/vscode/node_modules/@vscode/vsce/out/auth.js');
const { getSecurityRolesAPI } = require('../../packages/vscode/node_modules/@vscode/vsce/out/util.js');
(async () => {
  const summary = {schema:'airk.marketplace-capability/1', publisher:'iokaio', mutation_performed:false};
  try {
    const credential = await getAzureCredentialAccessToken();
    const response = await fetch('https://app.vssps.visualstudio.com/_apis/profile/profiles/me', {
      headers: { Authorization: 'Bearer ' + credential }, signal: AbortSignal.timeout(30000)
    });
    summary.profile_status = response.status;
    if (!response.ok) throw Object.assign(new Error('profile unavailable'), {statusCode:response.status});
    const profile = await response.json();
    const api = await getSecurityRolesAPI(credential);
    const assignments = await api.getRoleAssignments('gallery.publisher', 'iokaio');
    summary.publisher_api_authenticated = true;
    const mine = assignments.filter(row => String(row.identity?.id).toLowerCase() === String(profile.id).toLowerCase());
    summary.current_identity_roles = mine.map(row => ({name:row.role?.name, displayName:row.role?.displayName,
      allowPermissions:row.role?.allowPermissions, denyPermissions:row.role?.denyPermissions}));
    summary.direct_publishing_role_observed = mine.some(row => ['Owner','Contributor','Creator'].includes(row.role?.name));
  } catch (error) {
    summary.error_type = error.name;
    summary.status = error.statusCode ?? null;
    summary.publisher_api_authenticated ??= false;
    process.exitCode = 1;
  }
  fs.writeFileSync('artifacts/advanced-split/marketplace-capability.json', JSON.stringify(summary,null,2)+'\n');
  console.log(JSON.stringify(summary));
})();
