---
id: explorer.search.recipes
title: Saved Searches
route: search/recipes
area: explorer
aliases: saved search, recipe, preset
related_airk_help: search
---
# Saved Searches

Search › Saved Searches keeps named sets of Search arguments for this workspace: query, mode, path scope, limit, byte budget and full chunks. Recipes are stored in VS Code workspace state, never in the index or in workspace files.

## Steps

1. On Search › Query, fill in the form.
2. Type a name under **Save** and choose **Save**. A recipe with the same name is replaced; at most 50 are kept.
3. On Search › Saved Searches, choose **Run** to fill the Search form and run it, or **Delete** to remove the recipe.

A recipe that names vector or hybrid mode still follows the project's current vector settings when it runs.

## Recipe files

Import search reads a validated AIRK search-recipe JSON file. The Saved search files browser lists .airk/recipes, supports workspace selection, nested folders, breadcrumbs, Up, Refresh and bounded page navigation. Load or double-click a file to preview its query and mode, then explicitly add it or replace the same-named saved recipe. A conflicting intervening saved change is refused. Loading never runs a search. Export JSON uses the guarded workspace save dialog and defaults to .airk/recipes. Imported files are never modified; saved recipes remain workspace state until explicitly exported.
