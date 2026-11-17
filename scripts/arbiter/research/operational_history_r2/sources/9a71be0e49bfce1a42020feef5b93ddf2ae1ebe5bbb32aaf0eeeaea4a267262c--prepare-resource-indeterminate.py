# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib,hashlib,json
base=Path(__file__).resolve().parent
relative='crates/airk-cli/tests/combined_reads/split_metrics.rs'
live=base/'p10-worktree'/relative
before=live.read_text(encoding='utf-8')
s=before
def change(old,new):
    global s
    assert s.count(old)==1,(old[:100],s.count(old))
    s=s.replace(old,new)
change('    selection: CombinedConnectRequest,','    selection: CombinedConnectRequest,\n    reconfirm_required: bool,')
change('            selection: selected,','            selection: selected,\n            reconfirm_required: false,')
change('        if !force && observed == self.selection.expected_catalog_revision {','        if !force && !self.reconfirm_required && observed == self.selection.expected_catalog_revision {')
change('                self.selection = selected;','                self.selection = selected;\n                self.reconfirm_required = false;')
old='''                let scope_refused = matches!(
                    refusal["error"]["code"].as_str(),
                    Some("combined_scope_unavailable" | "combined_scope_expired")
                ) && refusal["error"]["retryable"] == false
                    && refusal["error"]["query_replay_safe"] == false
                    && refusal["requested_scope"]["confirmed"] == false
                    && refusal["requested_scope"]["selection"]
                        == serde_json::to_value(&self.selection)?;
                let typed = scope_refused || read_retry::classify(result).is_some();'''
new='''                let exact_nonreplayable = refusal["error"]["retryable"] == false
                    && refusal["error"]["query_replay_safe"] == false
                    && refusal["requested_scope"]["confirmed"] == false
                    && refusal["requested_scope"]["selection"] == serde_json::to_value(&self.selection)?;
                let scope_refused = exact_nonreplayable && matches!(
                    refusal["error"]["code"].as_str(),
                    Some("combined_scope_unavailable" | "combined_scope_expired"));
                // Canonical generic read failure is indeterminate, never proof
                // of a scope-only cause. Preserve it as a failed attempt.
                let indeterminate = exact_nonreplayable
                    && refusal["error"]["code"] == "combined_read_unavailable"
                    && refusal["error"].get("query_submitted") == Some(&Value::Null);
                if scope_refused || indeterminate {
                    self.reconfirm_required = true;
                }
                let typed = scope_refused || indeterminate || read_retry::classify(result).is_some();'''
change(old,new)
change('if scope_refused {"scope_refusal"}else if typed {"typed_rejection"}else{"untyped_rejection"}', 'if scope_refused {"scope_refusal"}else if indeterminate {"indeterminate_read_failure"}else if typed {"typed_rejection"}else{"untyped_rejection"}')
(base/'p10-resource-indeterminate.proposed.rs').write_text(s,encoding='utf-8',newline='\n')
print(hashlib.sha256(live.read_bytes()).hexdigest())
