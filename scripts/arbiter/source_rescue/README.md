# Authored source rescue

[Embedded source history](embedded_history/README.md) retains authored script fields and source fragments extracted from historical execution/proposal containers. It records exact container hashes, JSON pointers and decoded source identities. Containers, private inputs and results remain ignored. This directory also owns the reproducible read-only extraction planner; it never executes recovered scripts.

[Text source history](text_history/README.md) preserves native proposals, test fragments, source-transfer templates and a runbook found in text-suffixed artifacts, plus one decoded preparation probe. Exact byte maps distinguish source custody from historical results; none of the recovered programs is executed during rescue.

[Qualification runbooks and diagnostic source](runbook_history/README.md) preserves two substantial historical command runbooks and an exact Windows handle diagnostic fragment. Its source-only inverse checks do not execute or qualify historical commands.
