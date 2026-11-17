# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib

root = Path(__file__).resolve().parents[2]
name = 'crates/airk-mcp/src/response.rs'
original = (root / name).read_text(encoding='utf-8')
anchor = '        // Explicit diagnostic/capability/change tools always disclose their status.\n'
assert original.count(anchor) == 1
changed = original.replace(anchor, '''        // The full operation schemas are explicitly available in full mode.
        // Keep every other capability and verification field, including future
        // metadata. Never walk arbitrary user/evidence objects or rewrite shared
        // capability tables whose content hashes bind combined replies.
        if object.contains_key("adapters")
            && let Some(workbench) = object.get_mut("workbench").and_then(Value::as_object_mut)
            && workbench.get("catalog").is_some_and(Value::is_string)
            && workbench.get("operations").is_some_and(Value::is_object)
        {
            let operations = workbench.remove("operations").expect("validated operation catalog");
            let names: Vec<_> = operations.as_object().expect("validated operations").keys().cloned().collect();
            workbench.insert("operation_names".into(), serde_json::json!(names));
            workbench.insert("operation_schemas".into(), serde_json::json!({
                "included":false,"reason":"terse_reply","omitted_field":"workbench.operations",
                "request":{"response_mode":"full"}
            }));
        }
''' + anchor)
test = '''
    #[test]
    fn terse_capabilities_disclose_catalog_summary_and_full_restores_every_schema() {
        let catalog = airk_kernel::workbench::operation_catalog();
        let original = serde_json::json!({"adapters":[{"id":"rust","future_adapter":true}],
            "workbench":catalog,"future_native_capability":{"kept":true},
            "source_verification":{"schema":"future","sources":[{"path":"large.rs","whole_source_checked":true}]}});
        let mut terse = original.clone();
        Session::default().display(&mut terse, ResponseMode::Terse, false);
        assert!(terse["workbench"].get("operations").is_none());
        assert_eq!(terse["workbench"]["operation_schemas"]["omitted_field"], "workbench.operations");
        assert_eq!(terse["workbench"]["operation_schemas"]["request"]["response_mode"], "full");
        assert_eq!(terse["workbench"]["operation_names"].as_array().unwrap().len(), catalog["operations"].as_object().unwrap().len());
        assert_eq!(terse["adapters"], original["adapters"]);
        assert_eq!(terse["source_verification"], original["source_verification"]);
        assert_eq!(terse["future_native_capability"], original["future_native_capability"]);
        assert!(serde_json::to_vec(&terse).unwrap().len() < serde_json::to_vec(&original).unwrap().len() / 2);
        let mut full = original.clone();
        Session::default().display(&mut full, ResponseMode::Full, false);
        assert_eq!(full, original);
        let mut evidence = serde_json::json!({"workbench":catalog,"user_content":"unchanged"});
        let before = evidence.clone();
        Session::default().display(&mut evidence, ResponseMode::Terse, false);
        assert_eq!(evidence, before, "only the canonical capabilities envelope is summarized");
    }
'''
end = changed.rfind('\n}')
assert end > 0
changed = changed[:end] + test + changed[end:]
(Path(__file__).parent / 'capabilities-response.proposed.rs').write_text(changed, encoding='utf-8', newline='\n')
patch = ''.join(difflib.unified_diff(original.splitlines(keepends=True), changed.splitlines(keepends=True), fromfile='a/'+name, tofile='b/'+name))
(Path(__file__).parent / 'capabilities-summary.patch').write_text(patch, encoding='utf-8', newline='\n')
print('Prepared capabilities-summary.patch; full mode and verification receipts unchanged')
