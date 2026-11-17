# Reopening historical CUDA checkpoint evidence

checkpoint_sections.py validates the three actual checkpoint sections and Ninja
custody. materialized.py validates their source/tool/configuration closure and
reconstructs process-tree request JSON. cuda-static-checkpointed.py owns the
complete proof/2 and delegates only these read-only checks.

Historical producer source files are retained and hash-authenticated as data;
they are not executed by this checkpoint verification path. Do not replace their
recorded hashes with hashes of these maintained validators. The extraction
manifest records exact source files/functions. Tests are literal and in memory.
No CLI here starts a build or model. Full producer reproduction belongs to the
separate versioned build packages; this migration does not claim to replace them.

policy_owner.py supplies fresh-plan cpu_owner and identity_and_parent without
importing the historical R8 producer. It authenticates the tracked CPU owner and
lock, checks effective/default-owner/interactive identity with the pinned
maintained guard, and retains the exact approved parent SDDL. Optional original
parent deadlines are checked before and after the bounded guard process. It
changes no ACL and creates no output. Historical policy source pins remain data.
