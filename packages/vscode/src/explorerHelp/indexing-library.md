---
id: indexing.library
title: Indexes — Index library
route: indexing/library
area: explorer
aliases: indexes, sources, catalog
related_airk_help: indexing
---
# Indexes — Index library

The index library lists the indexes stored on this computer, read from the AIRK Service with the same complete data AIRK Admin and airkctl index list|show return. Use this library to create, update, split and retire indexes.

The library loads automatically, again when the window regains focus and after Check again. Refresh indexes reads it again; Load more indexes continues a long library from the service's own cursor. A failed read shows the service's error code and is never shown as an empty library.

Show index details reads the index's detail, sources, the index sets that use it and its path-exclusion and cleanup status. Split status reads a split by its operation ID. Every section keeps the complete native response with Copy JSON.

Manage on a row opens that index in this library. When AIRK has no index yet, Create an index for this folder opens the index library prefilled with that window folder.
