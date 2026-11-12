---
id: indexing.library
title: Indexing — Index library
route: indexing/library
area: explorer
aliases: indexes, sources, catalog
related_airk_help: indexing
---
# Indexing — Index library

The index library lists the indexes stored on this computer, read from the AIRK Service with the same complete data AIRK Admin and airkctl index list|show return. It is read-only: create, change, split and delete indexes in AIRK Admin.

The library loads automatically, again when the window regains focus and after Check again. Refresh indexes reads it again; Load more indexes continues a long library from the service's own cursor. A failed read shows the service's error code and is never shown as an empty library.

Show index details reads the index's detail, sources, the index sets that use it and its path-exclusion and cleanup status. Split status reads a split by its operation ID. Every section keeps the complete native response with Copy JSON.

Open in AIRK Admin on a row opens that index in the AIRK Admin index library. When AIRK has no index yet, Create an index for this folder opens the index library prefilled with that window folder.

Choose Open AIRK Admin to open the admin pages. Choose where... picks In VS Code (the Simple Browser, with a single-use sign-in link kept only in memory) or In my browser; the choice is remembered in the airk.admin.openIn setting. If the VS Code browser reports a certificate problem, choose Open in my browser.
