# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f06-draft/rebased-f05/source/crates/airk-cli/src');f=p/'project_client.rs';s=f.read_text()
# Tests explicitly model the RootNative selector's sole scope authority.
s=s.replace('''let backend = source_tools(root.clone(), old.project);
            let mut request''','''let backend = source_tools(root.clone(), old.project);
            backend.status.source_scope(old);
            let mut request''')
s=s.replace('''            assert!(backend.accept_source_fence(newer));
            peer.broker_release''','''            assert!(backend.accept_source_fence(newer));
            backend.status.source_scope(newer);
            peer.broker_release''')
s=s.replace('''                assert!(state.cached_status.is_none());''','''                assert!(backend.status.cache_empty());''')
s=s.replace('''            assert!(state.cached_status.is_none());''','''            assert!(backend.status.cache_empty());''')
# Dedicated status no longer retains a worker channel. Preserve the real retained
# channel identity regression through diagnostics, which uses the same Backend
# brokerage/reuse path and still performs exactly one pre-submit attempt.
start=s.index('    async fn cold_newer_grant_submits_but_newer_observed_epoch_discards_retained_channel()')
end=s.index('    #[tokio::test]',start)
part=s[start:end].replace('"airk_status"','"airk_diagnostics"')
part=part.replace('''        assert_eq!(
            body(&second)?["index_error"]["code"],
            "index_worker_unavailable"
        );''','''        assert!(body(&second)?["publication"].is_null(),
            "fresh brokerage failure must not report retained old publication");''')
s=s[:start]+part+s[end:]
# Submitted evidence still returns its original result/receipt; it must not adopt
# that receipt into a selector-replaced status owner.
start=s.index('    async fn submitted_old_reply_preserves_own_receipt_but_cannot_repopulate_source_status_cache()')
part=s[start:].replace('"airk_status"','"airk_read_file"',1)
part=part.replace('''        assert!(backend.accept_source_fence(fence(2, 10)));''','''        assert!(backend.accept_source_fence(fence(2, 10)));
        backend.status.source_scope(fence(2, 10));''',1)
s=s[:start]+part
f.write_text(s,newline='\n')
f=p/'status_observation.rs';s=f.read_text().replace('''    pub(crate) fn receipt(&self)''','''    #[cfg(test)]
    pub(super) fn cache_empty(&self)->bool{self.cache.lock().unwrap_or_else(|e|e.into_inner()).is_none()}
    pub(crate) fn receipt(&self)''');f.write_text(s,newline='\n')
