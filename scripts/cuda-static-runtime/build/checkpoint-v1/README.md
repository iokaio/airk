# Successful parent checkpoint execution source

run_checkpoint_section.py captures the first Flash stage; run_checkpoint_sections.py
captures the LLM and final all/re2 successor stages; compose_checkpointed_build.py
captures final metadata composition. The original target order, parallel2 and
10800-second section ceilings are recorded in ../source-map.json. The historical
campaign cutoff is not a reusable new-run deadline.

These programs retain actual predecessor, source, warm/Ninja and process-settlement
checks. Their hardcoded historical source/plan bindings must not be relabelled
for a new run. New input contracts, current output guards and explicit manifest
binding remain required before a supported launch exists. Current proof reopening
uses ../../verification; ../rebuild.py performs read-only new-manifest validation.
