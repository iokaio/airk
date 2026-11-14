# Indexing failure stages

Kernel owns `IndexFailureStage` and the content-free indexing diagnostic code.
All indexing admission paths, including the early vectors-enabled retained
semantic-upgrade check, attach the existing `vector_settings`, `current_scope`,
`read_snapshot` and `read_configuration` stages at their respective operations.
Repeated current-scope observations retain `current_scope` context as well.
The semantic-upgrade descriptor metadata read also carries `vector_settings`;
its length/hash/JSON checks remain the Retrieval owner's checks.

The early check must not bypass the later path's diagnostic stages. Its first
operation, `vectors_enabled`, calls `vector_policy`, whose ownership check reads
`current_scope` before the indexing branch's explicit scope observation. Native
JSON EOF in that current owner therefore yields
`index_vector_settings_metadata_incomplete`; a failure at the later explicit
scope observation instead carries `current_scope`. The original cause remains
available only in the explicit private failure-detail ring. This changes no
metadata validation, model admission, query inputs, budgets or persisted bytes.
Adding context retains typed causes used for recovery and contention; no
failure is reclassified by matching private exception text.

The Supervisor diagnostic regression uses an actual native-generated lexical
publication, enables the vector setting, then corrupts only its disposable
owner before inference. It compares native preflight and ordinary Index /
Reconcile execution, exact raw causes and restored source/publication identity.
That focused fixture does not inject a malformed captured model descriptor;
existing Retrieval metadata-refusal tests are source evidence for that sibling,
not a new end-to-end native qualification of the descriptor-fault case.
