# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('crates/airk-kernel/src/source_watch.rs')
s=p.read_text(encoding='utf-8')
# Events schedule a checked source turn even when their name is unsupported.
a=s.index('airk_ingest::permanent_policy::allows(')
b=s.index('\n                                    )',a)
s=s[:a]+s[a:b].replace('::allows(','::allows_checked(')+s[b:].replace('\n                                    )','\n                                    ).unwrap_or(true)',1)
for probe in ['false','true']:
    a=s.index('let allowed = walk',s.index('Some(Ok(entry))') if probe=='false' else s.index('else if let Some((path, depth))'))
    b=s.index('\n                        }' if probe=='false' else '\n                }',s.index('if !allowed',a))
    # Includes the if body but leaves the outer policy conditional's brace.
    s=s[:a]+'''match policy_directory_allows(policy,&walk.task.root,&path,'''+probe+''') {
                        Ok(true)=>(),
                        Ok(false)=>continue,
                        Err(_)=>{self.fail(WatchFailure::MetadataUnavailable);continue;}
                    }'''+s[b:]
at=s.index('const MAX_PROJECTS:')
s=s[:at]+'''fn policy_directory_allows(policy: &airk_core::source_policy::SourcePolicy, root: &Path, path: &Path, qualify: bool) -> Result<bool> {
    let root_text=root.to_str().ok_or_else(||anyhow::anyhow!("non-UTF-8 watcher root"))?;
    let relative=path.strip_prefix(root)?.to_str().ok_or_else(||anyhow::anyhow!("non-UTF-8 watcher path"))?.replace('\\\\',"/");
    let allowed=airk_ingest::permanent_policy::allows_checked(policy,root_text,&relative,true)?;
    if allowed && qualify {airk_ingest::permanent_policy::qualify_discovery_directory(policy,root_text,&relative)?;}
    Ok(allowed)
}

'''+s[at:]
p.write_text(s,encoding='utf-8')
