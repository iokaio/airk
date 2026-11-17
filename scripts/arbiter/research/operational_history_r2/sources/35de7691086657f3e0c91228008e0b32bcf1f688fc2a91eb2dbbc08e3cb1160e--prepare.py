# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib
root=Path.cwd();changes={}
for name in ['crates/airk-mcp/src/local.rs','crates/airk-mcp/src/unified.rs']:
 old=(root/name).read_text(encoding='utf-8')
 anchor='pub fn tools() -> Vec<Tool> {\n'
 assert old.count(anchor)==1
 new=old.replace(anchor,'''pub fn tools() -> Vec<Tool> {
    static TOOLS: std::sync::OnceLock<Vec<Tool>> = std::sync::OnceLock::new();
    TOOLS.get_or_init(build_tools).clone()
}

// Exact bundled definitions only; per-session projects/admission/runtime state stays outside.
fn build_tools() -> Vec<Tool> {
''',1)
 anchor='''    #[test]
    fn '''
 pos=new.index(anchor,new.index('mod tests'))
 extra='''    #[test]
    fn immutable_catalog_preserves_exact_definitions_and_isolates_caller_mutation() {
        let expected = serde_json::to_vec(&build_tools()).unwrap();
        assert_eq!(serde_json::to_vec(&tools()).unwrap(), expected);
        let mut changed = tools();
        changed[0].description = Some("caller-local description".into());
        std::sync::Arc::make_mut(&mut changed[0].input_schema)
            .insert("caller-local".into(), serde_json::json!(true));
        changed.reverse();
        changed.pop();
        assert_ne!(serde_json::to_vec(&changed).unwrap(), expected);
        assert_eq!(serde_json::to_vec(&tools()).unwrap(), expected);
    }

'''
 new=new[:pos]+extra+new[pos:];changes[name]=(old,new)
patch=''.join(''.join(difflib.unified_diff(a.splitlines(True),b.splitlines(True),fromfile='a/'+n,tofile='b/'+n)) for n,(a,b) in changes.items())
(root/'artifacts/optimizations/f02-draft/static-catalogs/mcp.patch').write_text(patch,encoding='utf-8',newline='\n')
for n,(_,body) in changes.items():
 p=root/'artifacts/optimizations/f02-draft/static-catalogs/source'/n;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(body,encoding='utf-8',newline='\n')
