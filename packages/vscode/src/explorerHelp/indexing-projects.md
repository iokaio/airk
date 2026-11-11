---
id: indexing.projects
title: Indexing — Projects
route: indexing/projects
area: explorer
aliases: Project catalog, memberships, defaults
related_airk_help: indexing
---
# Projects

Refresh reads a bounded native Project catalog page. Inspect a Project to read its revisions, defaults and paged memberships. Project creation, rename, deletion, assignment of an existing index, membership removal and defaults use a native preview before explicit apply. Index and source IDs come from Index library and Sources; source scope is explicit and never inferred from open folders.

Metadata and saved defaults grant no source access. This window can review saved associations when the trusted host supplies its canonical descriptor; this does not change evidence access. Access-grant controls and new-index setup remain unavailable in this screen. Native operations requiring an authority fence currently refuse with catalog_authority_fence_unavailable. The original failure remains visible. Source files are read-only.

Edit all membership scopes preserves the complete observed atom set, including different Source roots. Add or remove source/kind/path rows explicitly; duplicate atoms are refused. At most 128 atoms and the native 48 KiB request limit apply. Empty scopes grant no source authority. Paths are bounded by UTF-8 bytes and validated by the native canonical path contract.

Stale revisions, namespace replacement, trust revocation and client replacement invalidate reviews. Exact requests, native request digests and native preview digests are saved before apply. Every apply or reconciled receipt must match the reviewed request digest, schema, caller, full key and catalog revision. An interrupted reply requires Check native receipts; no apply is automatically repeated. A missing receipt is not proof of failure or permission to retry. Complete JSON details, copy and export retain all fields of the currently displayed native results, including continuations and receipts.
