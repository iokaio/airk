# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('crates/airk-ingest/src/permanent_policy.rs')
s=p.read_text(encoding='utf-8')
a=s.index('fn same('); b=s.index('fn native_relative_path')
s=s[:a]+'''use anyhow::{Result, ensure};
use crate::native_name_boundary::NativeNameBoundaryUnavailable;

fn same(left: &str, right: &str) -> bool {
    left == right || cfg!(any(windows, target_os="macos"))
        && left.is_ascii() && right.is_ascii() && left.eq_ignore_ascii_case(right)
}

/// Unknown Unicode collation is never proof of disjoint physical names.
fn deny_same(left: &str, right: &str) -> Result<bool> {
    if same(left,right) {return Ok(true);}
    if cfg!(any(windows,target_os="macos")) && (!left.is_ascii() || !right.is_ascii()) {
        return Err(NativeNameBoundaryUnavailable::new("unicode_name_equivalence").into());
    }
    Ok(false)
}

fn denied_prefix(candidate: &[String], prefix: &[String]) -> Result<bool> {
    if candidate.len()<prefix.len() {return Ok(false);}
    let mut unknown=None;
    for (left,right) in candidate.iter().zip(prefix) {
        match deny_same(left,right) {
            Ok(false)=>return Ok(false),
            Ok(true)=>(),
            Err(error)=>unknown=Some(error),
        }
    }
    if let Some(error)=unknown {return Err(error);}
    Ok(true)
}

pub fn restricts(policy: &SourcePolicy) -> bool {
    !policy.denies.is_empty() || policy.positive_includes.is_some()
}

/// Metadata-only platform case-mode proof. Does not open source file contents.
pub fn qualify(policy: &SourcePolicy, root: &str, path: &str, directory: bool) -> Result<()> {
    if restricts(policy) {crate::native_name_boundary::qualify(std::path::Path::new(root),path,directory)?;}
    Ok(())
}

''' +s[b:]
# Checked APIs preserve unsupported outcomes. Boolean predicates fail closed
# for the store/watcher seams, which cannot authorize an unknown comparison.
s=s.replace('pub fn allows_absolute_input(policy: &SourcePolicy, path: &std::path::Path) -> bool {', '''pub fn allows_absolute_input(policy: &SourcePolicy, path: &std::path::Path) -> bool {
    allows_absolute_input_checked(policy,path).unwrap_or(false)
}
pub fn allows_absolute_input_checked(policy: &SourcePolicy, path: &std::path::Path) -> Result<bool> {''')
s=s.replace('pub fn allows_directory_input(policy:&SourcePolicy,path:&std::path::Path)->bool {', '''pub fn allows_directory_input(policy:&SourcePolicy,path:&std::path::Path)->bool {
    allows_directory_input_checked(policy,path).unwrap_or(false)
}
pub fn allows_directory_input_checked(policy:&SourcePolicy,path:&std::path::Path)->Result<bool> {''')
s=s.replace('pub fn allows(policy: &SourcePolicy, root: &str, path: &str, directory: bool) -> bool {', '''pub fn allows(policy: &SourcePolicy, root: &str, path: &str, directory: bool) -> bool {
    allows_checked(policy,root,path,directory).unwrap_or(false)
}
pub fn allows_checked(policy: &SourcePolicy, root: &str, path: &str, directory: bool) -> Result<bool> {''')
s=s.replace('return false;', 'return Ok(false);').replace('return true;', 'return Ok(true);')
s=s.replace('if denied_prefix(&candidate,&prefix) {','if denied_prefix(&candidate,&prefix)? {')
s=s.replace('if !allows(policy,&root.canonical_path,&relative,false) {return Ok(false);}', 'if !allows_checked(policy,&root.canonical_path,&relative,false)? {return Ok(false);}')
s=s.replace('    true\n}', '    Ok(true)\n}')
a=s.index('    !policy.denies.iter().any('); b=s.index('\n}',a)
s=s[:a]+'''    for rule in &policy.denies {
        let Some(root)=policy.roots.iter().find(|root|root.id==rule.source_root_id) else {return Ok(false);};
        let mut denied=native_components(&root.canonical_path);
        denied.extend(rule.path.split('/').filter(|part|!part.is_empty()).map(str::to_owned));
        if denied_prefix(&candidate,&denied)? || denied_prefix(&denied,&candidate)? {return Ok(false);}
    }
    Ok(true)'''+s[b:]
a=s.index('    included && !policy.denies.iter().any('); b=s.index('\n}',a)
s=s[:a]+'''    if !included {return Ok(false);}
    for rule in &policy.denies {
        let Some(owner)=policy.roots.iter().find(|source|source.id==rule.source_root_id) else {return Ok(false);};
        let mut denied=native_components(&owner.canonical_path);
        denied.extend(rule.path.split('/').filter(|part|!part.is_empty()).map(str::to_owned));
        if (rule.kind==PathScope::Subtree || !directory && candidate.len()==denied.len())
            && denied_prefix(&candidate,&denied)? {return Ok(false);}
    }
    Ok(true)'''+s[b:]
a=s.index('    #[cfg(target_os="macos")]\n    #[test]'); s=s[:a]+'''    #[test]
    fn unknown_name_equivalence_is_not_a_successful_partial_result() -> Result<()> {
        assert!(deny_same("Private","private")? == cfg!(any(windows,target_os="macos")));
        if cfg!(any(windows,target_os="macos")) {
            assert!(deny_same("caf\\u{e9}","cafe\\u{301}").is_err());
            assert!(deny_same("Stra\\u{df}e","STRASSE").is_err());
            assert!(denied_prefix(&["source".into(),"caf\\u{e9}".into()],&["source".into(),"cafe".into()]).is_err());
        }
        // A disjoint component proves the paths disjoint even if another
        // component's comparison would be unsupported.
        assert!(!denied_prefix(&["allowed".into(),"caf\\u{e9}".into()],&["private".into(),"cafe".into()])?);
        Ok(())
    }
}
'''
s=s.replace('use anyhow::{Result, ensure};','use anyhow::Result;')
p.write_text(s,encoding='utf-8')
