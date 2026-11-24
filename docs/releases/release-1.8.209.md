# AIRK 1.8.209 local source candidate

**Date:** 24 November 2026. **Status:** local Windows source candidate, built for
Focus and Arbiter qualification work. It is not published, and installed and
platform qualification are recorded separately in the
[execution ledger](../simplify/one-process-ledger.md).

1.8.209 is [1.8.208](release-1.8.208.md) plus the unified user interface:

- **Explorer, Admin and Workbench on loopback HTTP.** The Explorer hosts the
  shared management tasks (Index library, Activity, Agents, Service, Storage,
  Recovery, Logs, Usage) itself, with its own navigation and sign-in. Standalone
  Admin opens only in the default browser through `airkctl admin open`; the
  `airk.admin.openIn` setting and the VS Code Simple Browser path are removed.
- **Admin console.** Grouped navigation (Manage, Investigate, Help), a compact
  menu, page history, a Runtime/Models switch on the Service page (Focus and
  Arbiter controls are under Models), and a Logs filter for a job or index that
  states how many loaded records it shows.
- **Workbench** is qualified against the current native Answer.

Everything in the 1.8.208 record still applies: plain HTTP on loopback with no
certificate, token or grant; automatic migration of `https` installations with
removal of the old AIRK certificate authority; fences as consistency checks;
operation-record pruning; and service-owned Answer jobs.
