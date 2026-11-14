# Project worker diagnostic context

The worker preserves Core job outcomes and Kernel's fixed indexing failure
stages; raw causes remain in Host's private failure-detail ring. Public job and
log summaries contain the same content-free code, without source paths or raw
exception text.

Index and Reconcile call the same native Kernel indexing entry point. Its early
semantic-upgrade preflight must carry the same fixed diagnostic stages as the
later indexing path. A corrupt current owner yields
`index_vector_settings_metadata_incomplete` before model/inference admission:
`vectors_enabled` calls `vector_policy`, which validates `current_scope` before
the indexing branch's next explicit current-scope observation. The first failed
operation owns the stage; it is not relabeled to a later stage. Snapshot selection alone does not validate
the current owner's JSON; the first selection-helper diagnosis was disproved
by the retained cheap regression and removed. The underlying error chain and
typed recovery markers remain. SourceUnavailable retains existing recovery;
Repair's approved-source/PlanError mapping is unchanged.

The focused regression corrupts only a disposable native-generated owner,
observes the exact code/detail on native preflight and both execution paths, then
restores its original bytes before asserting. Source bytes and publication stay
unchanged. No wire shape, persisted format, retry allowance or job identity is
changed.
