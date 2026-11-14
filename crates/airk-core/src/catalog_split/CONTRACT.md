# Native split grouping contract

Core owns the split request and its digested review. Policy contracts 2 and 3 retain their exact request serialization and meanings. The optional `grouping` field is omitted for these legacy requests. Newly prepared splits require policy contract 4 and a grouping containing the reviewed original name, operation-derived fresh set identity and both whole-index root scopes. This is an approved additive contract decision in advanced-split §7.3.1.

In `Create` mode the set ID is the first 16 bytes of framed digest `airk/split-original-name-set/1` over the split operation ID. `ExistingNamedSet` permits the ordinary initial readable set: preview must find exactly one exact-name set, prove its original-index membership has precisely the whole original scope, and pin its identity, definition/scope revisions and original membership revision. Publication preserves that membership and every other member/scope, adding only the destination. Missing/restricted original membership, case-only name collisions and multiple same-name sets refuse. Original definition/name and all grouping pins are checked before accepting and before publishing. Existing project effects remain explicit; the automatic existing-set effect retains the original and delegates destination addition exclusively to grouping. Grouping grants no evidence authority and rewrites no defaults/selections.

The existing membership publication transaction creates the set and both memberships, confirms their exact scopes, and advances the split receipt together. A failed transaction publishes none. Grouped membership receipts use `airk/split-grouped-memberships/1`; legacy receipts retain their exact digest domain/inputs. Recovery uses the original operation/request key and never creates a second identity. Set deletion reuses DeleteProject and preserves index/source/work rows and other memberships.

Policy 4 uses the existing policy3 physical catalog layout with marker 4, which predecessors refuse. Accepted legacy requests remain recoverable without adding grouping retroactively.

An inherited extraction profile does not downgrade a newly prepared grouped
split to policy 3. Accepting that request saves its exact native extraction
policy and marker 4 together before any build, storage or capacity effects.
Reopening the saved registry retains marker 4 and the same request/replay
identity. Policy 3 remains the existing ungrouped inherited-policy contract.
