---
id: explorer.search.recipes
title: Search recipes
route: search/recipes
area: explorer
aliases: saved search, recipe, preset
related_airk_help: search
---
# Search recipes

Search › Recipes keeps named sets of Search arguments for this workspace: query, mode, path scope, limit, byte budget and full chunks. Recipes are stored in VS Code workspace state, never in the index or in workspace files.

## Steps

1. On Search › Query, fill in the form.
2. Type a name under **Save as recipe** and choose **Save recipe**. A recipe with the same name is replaced; at most 50 are kept.
3. On Search › Recipes, choose **Run** to fill the Search form and run it, or **Delete** to remove the recipe.

A recipe that names vector or hybrid mode still follows the project's current vector settings when it runs.
