# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib
root=Path.cwd();name='crates/airk-mcp/src/help.rs';old=(root/name).read_text(encoding='utf-8');new=old
new=new.replace('''        result["content"] = json!(text);
        result["end_line"]''','''        result["end_line"]''',1)
new=new.replace('''        if serde_json::to_vec(&result)
            .map_err(|_| "help_serialization_failed")?
            .len()
            > args.max_bytes''','''        if candidate_bytes(&result, &text).map_err(|_| "help_serialization_failed")? > args.max_bytes''',1)
new=new.replace('''            result["content"] = json!(text);
            result["end_line"]''','''            result["end_line"]''',1)
new=new.replace('''    Ok(result)
}

pub fn reply''','''    result["content"] = Value::String(text);
    Ok(result)
}

/// Serialize the exact prospective object without cloning content or retaining wire bytes.
fn candidate_bytes(base: &Value, content: &str) -> Result<usize, serde_json::Error> {
    struct Candidate<'a> {
        base: &'a Value,
        content: &'a str,
    }
    impl Serialize for Candidate<'_> {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            use serde::ser::SerializeMap;
            let fields = self.base.as_object().expect("help reply object");
            let mut map = serializer.serialize_map(Some(fields.len()))?;
            for (key, value) in fields {
                if key == "content" {
                    map.serialize_entry(key, self.content)?;
                } else {
                    map.serialize_entry(key, value)?;
                }
            }
            map.end()
        }
    }
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.checked_add(bytes.len())
                .ok_or_else(|| std::io::Error::other("help size overflow"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(0);
    serde_json::to_writer(&mut counter, &Candidate { base, content })?;
    Ok(counter.0)
}

pub fn reply''',1)
assert new !=old and 'candidate_bytes(&result' in new
anchor='''    #[test]
    fn every_topic_is_bounded_and_can_be_read_without_a_collection()'''
extra='''    #[test]
    fn supplied_tool_catalog_remains_request_local_and_unknown_names_still_fail() {
        let mut supplied = crate::local::tools().remove(0);
        supplied.name = "request_local_fixture".into();
        supplied.description = Some("Original caller-specific description".into());
        std::sync::Arc::make_mut(&mut supplied.input_schema)
            .insert("fixture_root".into(), json!("fictional-root"));
        let expected = serde_json::to_string_pretty(&supplied).unwrap();
        let result = answer_with_catalog(args(json!({"tool":"request_local_fixture","max_bytes":32768})), Some(vec![supplied])).unwrap();
        assert_eq!(result["content"], expected);
        assert_eq!(answer(args(json!({"tool":"request_local_fixture"}))).unwrap_err(), "unknown_mcp_tool");
    }

    #[test]
    fn candidate_size_matches_complete_reply_serialization_at_escape_and_digit_boundaries() {
        let texts = ["", "plain", r#"quotes" and slash\\"#, "é中😀", "line\\nnext\\tcolumn"];
        for end in [1usize, 9, 10, 99, 100, 999, 1000] {
            for next in [Value::Null, json!(end + 1)] {
                let base = json!({"content":"", "end_line":end, "next_line":next,
                    "truncated":!next.is_null(), "title":"é中", "start_line":1,
                    "command":null,"tool":"airk_status","topics":[{"id":"fixture"}]});
                for text in texts {
                    let mut actual = base.clone();
                    actual["content"] = Value::String(text.into());
                    assert_eq!(candidate_bytes(&base, text).unwrap(), serde_json::to_vec(&actual).unwrap().len());
                    assert_eq!(base["content"], "", "sizing cannot mutate the candidate template");
                }
                let controls: String = (0u8..=31).map(char::from).collect();
                let mut actual = base.clone();
                actual["content"] = Value::String(controls.clone());
                assert_eq!(candidate_bytes(&base, &controls).unwrap(), serde_json::to_vec(&actual).unwrap().len());
            }
        }
    }

'''
assert new.count(anchor)==1;new=new.replace(anchor,extra+anchor,1)
# Correct the Rust raw quote/backslash sample without relying on nested string escaping.
lines=new.splitlines()
for i,line in enumerate(lines):
 if 'let texts = ' in line:
  lines[i]='        let texts = ["", "plain", r#"quotes" and slash\\"#, "é中😀", "line\\nnext\\tcolumn"];'
new='\n'.join(lines)+'\n'
p=root/'artifacts/optimizations/f02-draft/exact-help/source'/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(new,encoding='utf-8',newline='\n')
patch=''.join(difflib.unified_diff(old.splitlines(True),new.splitlines(True),fromfile='a/'+name,tofile='b/'+name))
(root/'artifacts/optimizations/f02-draft/exact-help/mcp.patch').write_text(patch,encoding='utf-8',newline='\n')
